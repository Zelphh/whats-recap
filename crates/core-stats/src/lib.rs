//! Estatísticas determinísticas (features 1–16). "Código conta, LLM classifica."

mod emoji;
mod emoji_table;
mod exclusive;
pub mod media;
mod response;
pub mod text;

pub use exclusive::{ExclusiveWord, ExclusiveWords};
pub use media::{AudioStats, MediaStats, StickerCount};
pub use response::ResponseStats;

use chrono::{DateTime, Datelike, NaiveDate, Timelike};
use core_parser::MessageKind;
use core_storage::{Db, MediaRow, MessageRow};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Incrementar quando o formato de [`Stats`] mudar, para invalidar o cache.
pub const STATS_VERSION: u32 = 2;
const TOP_N: usize = 10;
const CHART_WIDTH: usize = 25;
pub const WEEKDAY_LABELS: [&str; 7] = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatsSettings {
    pub session_gap_secs: i64,
    pub min_exclusive_freq: u64,
    pub stopwords: Vec<String>,
    pub strip_accents: bool,
    pub group_skin_tones: bool,
    /// Distância de Hamming máxima (em 64 bits) para duas figurinhas serem a mesma.
    #[serde(default = "default_sticker_distance")]
    pub sticker_max_distance: u32,
}

fn default_sticker_distance() -> u32 {
    4
}

impl Default for StatsSettings {
    fn default() -> Self {
        Self {
            session_gap_secs: 6 * 3600,
            min_exclusive_freq: 5,
            stopwords: text::DEFAULT_STOPWORDS.iter().map(|s| s.to_string()).collect(),
            strip_accents: false,
            group_skin_tones: false,
            sticker_max_distance: default_sticker_distance(),
        }
    }
}

/// Mensagem na forma mínima necessária para as estatísticas.
#[derive(Debug, Clone)]
pub struct Msg {
    pub id: i64,
    pub ts: i64,
    /// Índice em `authors`; `None` para sistema.
    pub author: Option<usize>,
    pub kind: MessageKind,
    pub text: Option<String>,
    pub media_file: Option<String>,
    pub session: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Count {
    pub item: String,
    pub count: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub messages: u64,
    pub words: u64,
    /// Mensagens com pelo menos uma palavra (texto ou legenda).
    pub text_messages: u64,
    pub avg_words_per_message: f64,
    pub stickers: u64,
    pub audios: u64,
    pub images: u64,
    pub videos: u64,
    pub docs: u64,
    pub media_hidden: u64,
    pub deleted: u64,
}

impl Totals {
    fn add(&mut self, m: &Msg, words: u32) {
        self.messages += 1;
        self.words += words as u64;
        if words > 0 {
            self.text_messages += 1;
        }
        match m.kind {
            MessageKind::Sticker => self.stickers += 1,
            MessageKind::Audio => self.audios += 1,
            MessageKind::Image => self.images += 1,
            MessageKind::Video => self.videos += 1,
            MessageKind::Doc => self.docs += 1,
            MessageKind::MediaHidden => self.media_hidden += 1,
            MessageKind::Deleted => self.deleted += 1,
            MessageKind::Text | MessageKind::System => {}
        }
    }

    fn finish(&mut self) {
        self.avg_words_per_message = if self.text_messages > 0 {
            self.words as f64 / self.text_messages as f64
        } else {
            0.0
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LongestMessage {
    pub id: i64,
    pub author: usize,
    pub ts: i64,
    pub words: u32,
    pub chars: u32,
    /// Início do texto, para exibir no card.
    pub preview: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaysStats {
    pub first_date: Option<NaiveDate>,
    pub last_date: Option<NaiveDate>,
    /// Dias distintos com pelo menos uma mensagem.
    pub active_days: u64,
    /// Dias entre a primeira e a última mensagem (inclusive).
    pub span_days: u64,
    pub active_pct: f64,
}

/// Série temporal preenchida com zero: `values[autor][i]` é o valor do i-ésimo período a partir de `start`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub start: String,
    pub values: Vec<Vec<u32>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekdayStats {
    /// `totals[autor][dia]`, segunda = 0.
    pub totals: Vec<[u64; 7]>,
    /// Quantas vezes cada dia da semana ocorre no período.
    pub occurrences: [u64; 7],
    /// Total (todos os autores) ÷ ocorrências daquele dia.
    pub avg_per_occurrence: [f64; 7],
    pub most_active: usize,
    pub least_active: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub version: u32,
    pub authors: Vec<String>,
    pub has_media: bool,
    pub totals: Totals,
    pub per_author: Vec<Totals>,
    pub longest_message: Option<LongestMessage>,
    pub days: DaysStats,
    pub daily: Series,
    pub monthly: Series,
    pub weekday: WeekdayStats,
    pub weekday_chart_text: String,
    /// `hourly[autor][hora]`.
    pub hourly: Vec<[u64; 24]>,
    pub top_words: Vec<Count>,
    pub top_words_by_author: Vec<Vec<Count>>,
    pub top_emojis: Vec<Count>,
    pub top_emojis_by_author: Vec<Vec<Count>>,
    pub exclusive_words: Vec<ExclusiveWords>,
    pub response_times: Vec<ResponseStats>,
    /// Autor com a menor mediana de tempo de resposta.
    pub fastest_responder: Option<usize>,
    /// Figurinhas e áudios; `None` no modo sem mídia.
    pub media: Option<MediaStats>,
}

/// Contagens de palavras e emojis por autor, calculadas em paralelo.
#[derive(Default)]
struct TextCounts {
    words: Vec<HashMap<String, u64>>,
    emojis: Vec<HashMap<String, (u64, String)>>,
}

impl TextCounts {
    fn new(n: usize) -> Self {
        Self {
            words: vec![HashMap::new(); n],
            emojis: vec![HashMap::new(); n],
        }
    }

    fn merge(mut self, other: Self) -> Self {
        for (a, b) in self.words.iter_mut().zip(other.words) {
            for (k, v) in b {
                *a.entry(k).or_default() += v;
            }
        }
        for (a, b) in self.emojis.iter_mut().zip(other.emojis) {
            for (k, (v, disp)) in b {
                a.entry(k).or_insert((0, disp)).0 += v;
            }
        }
        self
    }
}

fn date_of(ts: i64) -> NaiveDate {
    DateTime::from_timestamp(ts, 0).unwrap_or_default().naive_utc().date()
}

fn top(map: impl IntoIterator<Item = (String, u64)>, n: usize) -> Vec<Count> {
    let mut v: Vec<Count> = map.into_iter().map(|(item, count)| Count { item, count }).collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.item.cmp(&b.item)));
    v.truncate(n);
    v
}

fn sum_maps<V: Clone>(maps: &[HashMap<String, V>], get: impl Fn(&V) -> u64) -> HashMap<String, u64> {
    let mut out: HashMap<String, u64> = HashMap::new();
    for m in maps {
        for (k, v) in m {
            *out.entry(k.clone()).or_default() += get(v);
        }
    }
    out
}

fn emoji_top(map: &HashMap<String, (u64, String)>, displays: &HashMap<String, String>, n: usize) -> Vec<Count> {
    let mut t = top(map.iter().map(|(k, (c, _))| (k.clone(), *c)), n);
    for c in &mut t {
        if let Some(d) = displays.get(&c.item) {
            c.item = d.clone();
        }
    }
    t
}

/// Gráfico de barras em texto por dia da semana, para copiar e compartilhar.
pub fn weekday_text_chart(values: &[u64; 7]) -> String {
    let max = values.iter().copied().max().unwrap_or(0);
    WEEKDAY_LABELS
        .iter()
        .zip(values)
        .map(|(label, &v)| {
            let blocks = if max == 0 {
                0
            } else {
                ((v as f64 / max as f64) * CHART_WIDTH as f64)
                    .round()
                    .max(if v > 0 { 1.0 } else { 0.0 }) as usize
            };
            format!("{label}  {}", "█".repeat(blocks))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Carrega as mensagens do banco e calcula todas as estatísticas.
pub fn compute_from_db(db: &Db, settings: &StatsSettings) -> core_storage::Result<Stats> {
    let summary = db.summary()?;
    let index: HashMap<&str, usize> = summary
        .authors
        .iter()
        .enumerate()
        .map(|(i, a)| (a.as_str(), i))
        .collect();
    let mut msgs = Vec::with_capacity(summary.message_count.max(0) as usize);
    db.for_each_message(|m: MessageRow| {
        msgs.push(Msg {
            id: m.id,
            ts: m.ts,
            author: m.author.as_deref().and_then(|a| index.get(a).copied()),
            kind: m.kind,
            text: m.text,
            media_file: m.media_file,
            session: m.session_id,
        })
    })?;
    let media = if summary.has_media {
        db.media_map()?
    } else {
        HashMap::new()
    };
    Ok(compute(&summary.authors, summary.has_media, &msgs, &media, settings))
}

pub fn compute(
    authors: &[String],
    has_media: bool,
    msgs: &[Msg],
    media: &HashMap<String, MediaRow>,
    settings: &StatsSettings,
) -> Stats {
    let n = authors.len();
    // Só mensagens com autor entram nas estatísticas (sistema fica de fora).
    let counted: Vec<&Msg> = msgs
        .iter()
        .filter(|m| m.author.is_some() && m.kind != MessageKind::System)
        .collect();

    let normalizer = text::Normalizer::new(&settings.stopwords, settings.strip_accents);
    let word_counts: Vec<u32> = counted
        .par_iter()
        .map(|m| {
            m.text
                .as_deref()
                .map_or(0, |t| text::words(&text::without_urls(t)).count() as u32)
        })
        .collect();
    let text_counts = counted
        .par_iter()
        .fold(
            || TextCounts::new(n),
            |mut acc, m| {
                if let (Some(a), Some(t)) = (m.author, m.text.as_deref()) {
                    let clean = text::without_urls(t);
                    for w in normalizer.content_words(&clean) {
                        *acc.words[a].entry(w).or_default() += 1;
                    }
                    for (key, display) in emoji::emojis(t, settings.group_skin_tones) {
                        acc.emojis[a].entry(key).or_insert((0, display)).0 += 1;
                    }
                }
                acc
            },
        )
        .reduce(|| TextCounts::new(n), TextCounts::merge);

    let mut totals = Totals::default();
    let mut per_author = vec![Totals::default(); n];
    let mut hourly = vec![[0u64; 24]; n];
    let mut weekday_totals = vec![[0u64; 7]; n];
    let mut longest: Option<LongestMessage> = None;

    let first_date = counted.iter().map(|m| m.ts).min().map(date_of);
    let last_date = counted.iter().map(|m| m.ts).max().map(date_of);
    let span_days = match (first_date, last_date) {
        (Some(f), Some(l)) => (l - f).num_days() as usize + 1,
        _ => 0,
    };
    let (month_start, months) = match (first_date, last_date) {
        (Some(f), Some(l)) => {
            let idx = |d: NaiveDate| d.year() as i64 * 12 + d.month0() as i64;
            (idx(f), (idx(l) - idx(f)) as usize + 1)
        }
        _ => (0, 0),
    };
    let mut daily = vec![vec![0u32; span_days]; n];
    let mut monthly = vec![vec![0u32; months]; n];

    for (m, &words) in counted.iter().zip(&word_counts) {
        let a = m.author.unwrap();
        totals.add(m, words);
        per_author[a].add(m, words);

        let dt = DateTime::from_timestamp(m.ts, 0).unwrap_or_default().naive_utc();
        let date = dt.date();
        hourly[a][dt.hour() as usize] += 1;
        weekday_totals[a][date.weekday().num_days_from_monday() as usize] += 1;
        if let Some(f) = first_date {
            daily[a][(date - f).num_days() as usize] += 1;
            monthly[a][(date.year() as i64 * 12 + date.month0() as i64 - month_start) as usize] += 1;
        }

        if words > 0 {
            let text = m.text.as_deref().unwrap_or_default();
            let chars = text.chars().count() as u32;
            if longest.as_ref().is_none_or(|l| (words, chars) > (l.words, l.chars)) {
                longest = Some(LongestMessage {
                    id: m.id,
                    author: a,
                    ts: m.ts,
                    words,
                    chars,
                    preview: text.chars().take(280).collect(),
                });
            }
        }
    }
    totals.finish();
    per_author.iter_mut().for_each(Totals::finish);

    let active_days = (0..span_days).filter(|&d| daily.iter().any(|v| v[d] > 0)).count() as u64;
    let days = DaysStats {
        first_date,
        last_date,
        active_days,
        span_days: span_days as u64,
        active_pct: if span_days > 0 {
            active_days as f64 * 100.0 / span_days as f64
        } else {
            0.0
        },
    };

    let mut occurrences = [0u64; 7];
    if let Some(f) = first_date {
        for d in 0..span_days {
            occurrences[(f + chrono::Days::new(d as u64)).weekday().num_days_from_monday() as usize] += 1;
        }
    }
    let mut weekday_all = [0u64; 7];
    for t in &weekday_totals {
        for (acc, v) in weekday_all.iter_mut().zip(t) {
            *acc += v;
        }
    }
    let avg_per_occurrence: [f64; 7] = std::array::from_fn(|i| {
        if occurrences[i] > 0 {
            weekday_all[i] as f64 / occurrences[i] as f64
        } else {
            0.0
        }
    });
    let by_avg = |a: &usize, b: &usize| avg_per_occurrence[*a].total_cmp(&avg_per_occurrence[*b]);
    let weekday = WeekdayStats {
        totals: weekday_totals,
        occurrences,
        avg_per_occurrence,
        most_active: (0..7).max_by(by_avg).unwrap_or(0),
        least_active: (0..7).min_by(by_avg).unwrap_or(0),
    };

    let words_all = sum_maps(&text_counts.words, |v| *v);
    let emojis_all = sum_maps(&text_counts.emojis, |(c, _)| *c);
    let mut emoji_display: HashMap<String, String> = HashMap::new();
    for m in &text_counts.emojis {
        for (k, (_, d)) in m {
            // Prefere a forma com seletor de variação (renderiza como emoji colorido).
            let e = emoji_display.entry(k.clone()).or_insert_with(|| d.clone());
            if d.len() > e.len() {
                *e = d.clone();
            }
        }
    }
    let emojis_all_map: HashMap<String, (u64, String)> =
        emojis_all.into_iter().map(|(k, c)| (k, (c, String::new()))).collect();

    let response_times = response::response_times(msgs, n, settings.session_gap_secs);
    let fastest_responder = response::fastest(&response_times);
    let exclusive_words = exclusive::exclusive_words(&text_counts.words, settings.min_exclusive_freq);

    Stats {
        version: STATS_VERSION,
        authors: authors.to_vec(),
        has_media,
        totals,
        per_author,
        longest_message: longest,
        days,
        daily: Series {
            start: first_date.map(|d| d.to_string()).unwrap_or_default(),
            values: daily,
        },
        monthly: Series {
            start: first_date.map(|d| d.format("%Y-%m").to_string()).unwrap_or_default(),
            values: monthly,
        },
        weekday_chart_text: weekday_text_chart(&weekday_all),
        weekday,
        hourly,
        top_words: top(words_all, TOP_N),
        top_words_by_author: text_counts.words.iter().map(|m| top(m.clone(), TOP_N)).collect(),
        top_emojis: emoji_top(&emojis_all_map, &emoji_display, TOP_N),
        top_emojis_by_author: text_counts
            .emojis
            .iter()
            .map(|m| emoji_top(m, &emoji_display, TOP_N))
            .collect(),
        exclusive_words,
        response_times,
        fastest_responder,
        media: has_media.then(|| media::media_stats(msgs, n, media, settings.sticker_max_distance)),
    }
}

#[cfg(test)]
mod tests;
