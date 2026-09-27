//! Ferramentas fechadas do chat tipo 1. O LLM (ou a heurística) só escolhe a ferramenta e os
//! parâmetros; a contagem é sempre feita aqui, sobre o SQLite.

use crate::text::{WordQuery, tokens};
use chrono::{Datelike, Days, NaiveDate};
use core_stats::response::{ResponseStats, fastest, response_times};
use core_stats::{Msg, StatsSettings};
use core_storage::{Db, MessageRow};
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Quantas mensagens de exemplo são devolvidas como citação.
const EXAMPLES: usize = 5;
const TOP_PERIODS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    Day,
    Week,
    Month,
    Year,
    Weekday,
}

/// Chamada de ferramenta, no formato que o LLM preenche: `{"tool": "...", "args": {...}}`.
/// Datas são `AAAA-MM-DD`, inclusivas. `author` é o nome como aparece na conversa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args", rename_all = "snake_case")]
pub enum ToolCall {
    CountWord {
        word: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<NaiveDate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<NaiveDate>,
    },
    MessagesByHour {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<NaiveDate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<NaiveDate>,
    },
    MessagesByPeriod {
        granularity: Granularity,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<NaiveDate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<NaiveDate>,
    },
    FirstOccurrence {
        word: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
    },
    ResponseTime {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        author: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<NaiveDate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<NaiveDate>,
    },
}

impl ToolCall {
    pub fn name(&self) -> &'static str {
        match self {
            ToolCall::CountWord { .. } => "count_word",
            ToolCall::MessagesByHour { .. } => "messages_by_hour",
            ToolCall::MessagesByPeriod { .. } => "messages_by_period",
            ToolCall::FirstOccurrence { .. } => "first_occurrence",
            ToolCall::ResponseTime { .. } => "response_time",
        }
    }

    pub fn range(&self) -> (Option<NaiveDate>, Option<NaiveDate>) {
        match self {
            ToolCall::CountWord { from, to, .. }
            | ToolCall::MessagesByHour { from, to, .. }
            | ToolCall::MessagesByPeriod { from, to, .. }
            | ToolCall::ResponseTime { from, to, .. } => (*from, *to),
            ToolCall::FirstOccurrence { .. } => (None, None),
        }
    }

    pub fn author(&self) -> Option<&str> {
        match self {
            ToolCall::CountWord { author, .. }
            | ToolCall::MessagesByHour { author, .. }
            | ToolCall::MessagesByPeriod { author, .. }
            | ToolCall::FirstOccurrence { author, .. }
            | ToolCall::ResponseTime { author, .. } => author.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodCount {
    /// `2024-03-05` (dia), `2024-03-04` (semana, segunda-feira de início), `2024-03` (mês),
    /// `2024` (ano) ou `0`–`6` (dia da semana, segunda = 0).
    pub period: String,
    pub by_author: Vec<u64>,
    pub total: u64,
}

/// Resultado de uma ferramenta. `by_author` segue a ordem de `authors` da conversa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ToolResult {
    CountWord {
        word: String,
        total: u64,
        by_author: Vec<u64>,
        /// Mensagens com pelo menos uma ocorrência.
        messages: u64,
        /// Primeiras mensagens com ocorrência (ids), para citação.
        examples: Vec<i64>,
    },
    MessagesByHour {
        by_author: Vec<[u64; 24]>,
    },
    MessagesByPeriod {
        granularity: Granularity,
        /// Série completa em ordem cronológica, com zeros nos períodos sem mensagem.
        periods: Vec<PeriodCount>,
        /// Os períodos com mais mensagens (maior primeiro).
        top: Vec<PeriodCount>,
        /// Os períodos com menos mensagens (menor primeiro).
        bottom: Vec<PeriodCount>,
    },
    FirstOccurrence {
        word: String,
        message: Option<MessageRow>,
    },
    ResponseTime {
        by_author: Vec<ResponseStats>,
        fastest: Option<usize>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("não encontrei \"{name}\" na conversa (pessoas: {})", .options.join(", "))]
    UnknownAuthor { name: String, options: Vec<String> },
    #[error("a busca precisa de pelo menos uma palavra")]
    EmptyWord,
    #[error("a data inicial ({from}) é depois da final ({to})")]
    InvertedRange { from: NaiveDate, to: NaiveDate },
    #[error(transparent)]
    Storage(#[from] core_storage::StorageError),
    #[error("erro no banco de dados: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Contexto de uma conversa para executar ferramentas.
pub struct Conversation<'a> {
    pub db: &'a Db,
    pub authors: Vec<String>,
    pub settings: StatsSettings,
}

fn fold(s: &str) -> String {
    core_stats::text::strip_accents(&s.to_lowercase())
}

impl<'a> Conversation<'a> {
    pub fn open(db: &'a Db) -> Result<Self, ToolError> {
        Ok(Self {
            authors: db.summary()?.authors,
            settings: db.meta_get("settings")?.unwrap_or_default(),
            db,
        })
    }

    /// Resolve um nome dito pelo usuário ("bruno", "Bruno Silva", "Ana") para o índice do autor.
    pub fn resolve_author(&self, name: &str) -> Result<usize, ToolError> {
        let wanted = fold(name.trim());
        let first = |s: &str| s.split_whitespace().next().unwrap_or_default().to_string();
        let folded: Vec<String> = self.authors.iter().map(|a| fold(a)).collect();
        folded
            .iter()
            .position(|a| *a == wanted)
            .or_else(|| {
                let hits: Vec<usize> = (0..folded.len())
                    .filter(|&i| first(&folded[i]) == first(&wanted))
                    .collect();
                (hits.len() == 1).then(|| hits[0])
            })
            .ok_or_else(|| ToolError::UnknownAuthor {
                name: name.to_string(),
                options: self.authors.clone(),
            })
    }

    fn author_idx(&self, author: Option<&str>) -> Result<Option<usize>, ToolError> {
        author.map(|a| self.resolve_author(a)).transpose()
    }

    /// Cláusula `WHERE` comum: só mensagens com autor, sem sistema, no intervalo e do autor pedido.
    fn filter(
        &self,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<(String, Vec<SqlValue>), ToolError> {
        if let (Some(f), Some(t)) = (from, to)
            && f > t
        {
            return Err(ToolError::InvertedRange { from: f, to: t });
        }
        let mut sql = String::from("author IS NOT NULL AND kind != 'system'");
        let mut params = Vec::new();
        if let Some(a) = author {
            sql.push_str(" AND author = ?");
            params.push(SqlValue::Text(self.authors[a].clone()));
        }
        if let Some(f) = from {
            sql.push_str(" AND ts >= ?");
            params.push(SqlValue::Integer(day_start(f)));
        }
        if let Some(t) = to {
            sql.push_str(" AND ts < ?");
            params.push(SqlValue::Integer(day_start(t + Days::new(1))));
        }
        Ok((sql, params))
    }

    pub fn execute(&self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        match call {
            ToolCall::CountWord { word, author, from, to } => {
                self.count_word(word, self.author_idx(author.as_deref())?, *from, *to)
            }
            ToolCall::MessagesByHour { author, from, to } => {
                self.messages_by_hour(self.author_idx(author.as_deref())?, *from, *to)
            }
            ToolCall::MessagesByPeriod {
                granularity,
                author,
                from,
                to,
            } => self.messages_by_period(*granularity, self.author_idx(author.as_deref())?, *from, *to),
            ToolCall::FirstOccurrence { word, author } => {
                self.first_occurrence(word, self.author_idx(author.as_deref())?)
            }
            ToolCall::ResponseTime { author, from, to } => {
                self.response_time(self.author_idx(author.as_deref())?, *from, *to)
            }
        }
    }

    fn scan_text(
        &self,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
        mut f: impl FnMut(i64, &str, &str) -> bool,
    ) -> Result<(), ToolError> {
        let (filter, params) = self.filter(author, from, to)?;
        let mut st = self.db.conn().prepare(&format!(
            "SELECT id, author, text FROM messages WHERE text IS NOT NULL AND {filter} ORDER BY id"
        ))?;
        let mut rows = st.query(rusqlite::params_from_iter(params))?;
        while let Some(r) = rows.next()? {
            let (id, a, t): (i64, String, String) = (r.get(0)?, r.get(1)?, r.get(2)?);
            if !f(id, &a, &t) {
                break;
            }
        }
        Ok(())
    }

    fn count_word(
        &self,
        word: &str,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<ToolResult, ToolError> {
        let q = WordQuery::new(word).ok_or(ToolError::EmptyWord)?;
        let mut by_author = vec![0u64; self.authors.len()];
        let (mut total, mut messages, mut examples) = (0u64, 0u64, Vec::new());
        self.scan_text(author, from, to, |id, a, text| {
            let n = q.count_in(&tokens(text));
            if n > 0 {
                total += n;
                messages += 1;
                if let Some(i) = self.authors.iter().position(|x| x == a) {
                    by_author[i] += n;
                }
                if examples.len() < EXAMPLES {
                    examples.push(id);
                }
            }
            true
        })?;
        Ok(ToolResult::CountWord {
            word: word.to_string(),
            total,
            by_author,
            messages,
            examples,
        })
    }

    fn first_occurrence(&self, word: &str, author: Option<usize>) -> Result<ToolResult, ToolError> {
        let q = WordQuery::new(word).ok_or(ToolError::EmptyWord)?;
        let mut found = None;
        self.scan_text(author, None, None, |id, _, text| {
            if q.count_in(&tokens(text)) > 0 {
                found = Some(id);
                return false;
            }
            true
        })?;
        let message = match found {
            Some(id) => self.db.messages_from(id, 1)?.into_iter().next(),
            None => None,
        };
        Ok(ToolResult::FirstOccurrence {
            word: word.to_string(),
            message,
        })
    }

    /// Contagem por (dia, hora, autor). Como `ts` é o horário local sem fuso, o dia é `ts / 86400`
    /// e a hora `(ts % 86400) / 3600`. A agregação é feita aqui: o `GROUP BY` do SQLite montava
    /// uma B-tree temporária e era ~7× mais lento que ler `(ts, author)` e contar num `HashMap`.
    fn day_hour_counts(
        &self,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<Vec<(i64, usize, usize, u64)>, ToolError> {
        let (filter, params) = self.filter(author, from, to)?;
        let mut st = self
            .db
            .conn()
            .prepare(&format!("SELECT ts, author FROM messages WHERE {filter}"))?;
        let mut rows = st.query(rusqlite::params_from_iter(params))?;
        let mut counts: std::collections::HashMap<(i64, usize, usize), u64> = std::collections::HashMap::new();
        while let Some(r) = rows.next()? {
            let ts: i64 = r.get(0)?;
            let a = r.get_ref(1)?.as_str().map_err(rusqlite::Error::from)?;
            if let Some(i) = self.authors.iter().position(|x| x == a) {
                *counts
                    .entry((ts.div_euclid(86400), (ts.rem_euclid(86400) / 3600) as usize, i))
                    .or_default() += 1;
            }
        }
        Ok(counts.into_iter().map(|((d, h, a), n)| (d, h, a, n)).collect())
    }

    fn messages_by_hour(
        &self,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<ToolResult, ToolError> {
        let mut by_author = vec![[0u64; 24]; self.authors.len()];
        for (_, hour, a, n) in self.day_hour_counts(author, from, to)? {
            by_author[a][hour] += n;
        }
        Ok(ToolResult::MessagesByHour { by_author })
    }

    fn messages_by_period(
        &self,
        granularity: Granularity,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<ToolResult, ToolError> {
        let n = self.authors.len();
        let mut counts: BTreeMap<String, Vec<u64>> = BTreeMap::new();
        let (mut min_day, mut max_day) = (i64::MAX, i64::MIN);
        for (day, _, a, c) in self.day_hour_counts(author, from, to)? {
            min_day = min_day.min(day);
            max_day = max_day.max(day);
            counts
                .entry(period_key(granularity, date_of(day * 86400)))
                .or_insert_with(|| vec![0; n])[a] += c;
        }
        // Preenche com zero os períodos sem mensagem, dentro do intervalo pedido (ou dos dados).
        let lo = from.or_else(|| (min_day != i64::MAX).then(|| date_of(min_day * 86400)));
        let hi = to.or_else(|| (max_day != i64::MIN).then(|| date_of(max_day * 86400)));
        if let (Some(lo), Some(hi)) = (lo, hi) {
            for p in period_keys(granularity, lo, hi) {
                counts.entry(p).or_insert_with(|| vec![0; n]);
            }
        }
        let periods: Vec<PeriodCount> = counts
            .into_iter()
            .map(|(period, by_author)| PeriodCount {
                total: by_author.iter().sum(),
                period,
                by_author,
            })
            .collect();
        let mut ranked = periods.clone();
        ranked.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.period.cmp(&b.period)));
        let top = ranked.iter().take(TOP_PERIODS).cloned().collect();
        // Empates: o período mais antigo primeiro, nos dois rankings.
        ranked.sort_by(|a, b| a.total.cmp(&b.total).then_with(|| a.period.cmp(&b.period)));
        let bottom = ranked.iter().take(TOP_PERIODS).cloned().collect();
        Ok(ToolResult::MessagesByPeriod {
            granularity,
            periods,
            top,
            bottom,
        })
    }

    fn response_time(
        &self,
        author: Option<usize>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> Result<ToolResult, ToolError> {
        // O tempo de resposta depende das duas pessoas: filtra só por data e escolhe o autor no fim.
        let (filter, params) = self.filter(None, from, to)?;
        let mut st = self.db.conn().prepare(&format!(
            "SELECT id, ts, author, kind, session_id FROM messages WHERE {filter} ORDER BY id"
        ))?;
        let mut rows = st.query(rusqlite::params_from_iter(params))?;
        let mut msgs = Vec::new();
        while let Some(r) = rows.next()? {
            let (a, kind): (String, String) = (r.get(2)?, r.get(3)?);
            msgs.push(Msg {
                id: r.get(0)?,
                ts: r.get(1)?,
                author: self.authors.iter().position(|x| *x == a),
                kind: core_parser::MessageKind::parse(&kind).unwrap_or(core_parser::MessageKind::Text),
                text: None,
                media_file: None,
                session: r.get(4)?,
            });
        }
        let mut by_author = response_times(&msgs, self.authors.len(), self.settings.session_gap_secs);
        let fastest = fastest(&by_author);
        if let Some(a) = author {
            for (i, s) in by_author.iter_mut().enumerate() {
                if i != a {
                    *s = ResponseStats::default();
                }
            }
        }
        Ok(ToolResult::ResponseTime { by_author, fastest })
    }
}

pub(crate) fn day_start(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc().timestamp()
}

fn date_of(ts: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default().date_naive()
}

/// Chave do período de uma data (mesmo formato de [`PeriodCount::period`]).
fn period_key(g: Granularity, d: NaiveDate) -> String {
    match g {
        Granularity::Day => d.to_string(),
        Granularity::Week => (d - Days::new(d.weekday().num_days_from_monday() as u64)).to_string(),
        Granularity::Month => format!("{}-{:02}", d.year(), d.month()),
        Granularity::Year => d.year().to_string(),
        Granularity::Weekday => d.weekday().num_days_from_monday().to_string(),
    }
}

fn period_keys(g: Granularity, lo: NaiveDate, hi: NaiveDate) -> Vec<String> {
    let mut out = Vec::new();
    match g {
        Granularity::Weekday => out.extend((0..7).map(|d| d.to_string())),
        Granularity::Year => out.extend((lo.year()..=hi.year()).map(|y| y.to_string())),
        Granularity::Month => {
            let (mut y, mut m) = (lo.year(), lo.month());
            while (y, m) <= (hi.year(), hi.month()) {
                out.push(format!("{y}-{m:02}"));
                (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
            }
        }
        Granularity::Week | Granularity::Day => {
            let step = if g == Granularity::Week { 7 } else { 1 };
            let mut d = if g == Granularity::Week {
                lo - Days::new(lo.weekday().num_days_from_monday() as u64)
            } else {
                lo
            };
            while d <= hi {
                out.push(d.to_string());
                d = d + Days::new(step);
            }
        }
    }
    out
}
