use super::*;
use chrono::NaiveDate;
use core_storage::MediaRow;

fn ts(d: u32, h: u32, m: u32, s: u32) -> i64 {
    // Janeiro de 2024: dia 1 é uma segunda-feira.
    NaiveDate::from_ymd_opt(2024, 1, d)
        .unwrap()
        .and_hms_opt(h, m, s)
        .unwrap()
        .and_utc()
        .timestamp()
}

struct Builder {
    msgs: Vec<Msg>,
    gap: i64,
    last_ts: i64,
    session: i64,
}

impl Builder {
    fn new() -> Self {
        Self {
            msgs: Vec::new(),
            gap: 6 * 3600,
            last_ts: i64::MIN,
            session: 0,
        }
    }
    fn push(&mut self, ts: i64, author: Option<usize>, kind: MessageKind, text: Option<&str>) -> &mut Self {
        if self.last_ts == i64::MIN || ts - self.last_ts > self.gap {
            self.session += 1;
        }
        self.last_ts = ts;
        self.msgs.push(Msg {
            id: self.msgs.len() as i64 + 1,
            ts,
            author,
            kind,
            text: text.map(String::from),
            media_file: None,
            session: self.session,
        });
        self
    }
    fn text(&mut self, ts: i64, a: usize, t: &str) -> &mut Self {
        self.push(ts, Some(a), MessageKind::Text, Some(t))
    }
}

fn authors() -> Vec<String> {
    vec!["Ana".into(), "Bruno".into()]
}

#[test]
fn simple_counts() {
    let mut b = Builder::new();
    b.push(ts(1, 8, 0, 0), None, MessageKind::System, Some("criptografia"))
        .text(ts(1, 8, 0, 0), 0, "bom dia amor 😍😍")
        .text(ts(1, 8, 2, 0), 1, "bom dia! dormiu bem? https://exemplo.com/a/b")
        .push(ts(1, 8, 3, 0), Some(0), MessageKind::MediaHidden, None)
        .text(ts(3, 22, 0, 0), 1, "siiiim kkkkkk amor amor")
        .push(ts(3, 22, 1, 0), Some(0), MessageKind::Deleted, None);
    let s = compute(&authors(), false, &b.msgs, &HashMap::new(), &StatsSettings::default());

    assert_eq!(s.totals.messages, 5);
    assert_eq!(s.per_author[0].messages, 3);
    assert_eq!(s.per_author[0].media_hidden, 1);
    assert_eq!(s.per_author[0].deleted, 1);
    // "bom dia amor" (3) + "bom dia dormiu bem" (4, URL ignorada) + "siiiim kkkkkk amor amor" (4)
    assert_eq!(s.totals.words, 11);
    assert_eq!(s.totals.text_messages, 3);
    assert!((s.totals.avg_words_per_message - 11.0 / 3.0).abs() < 1e-9);

    let longest = s.longest_message.unwrap();
    assert_eq!(longest.id, 3);
    assert_eq!(longest.words, 4);

    assert_eq!(s.days.active_days, 2);
    assert_eq!(s.days.span_days, 3);
    assert_eq!(s.daily.start, "2024-01-01");
    assert_eq!(s.daily.values[1], vec![1, 0, 1]); // dia vazio preenchido com zero
    assert_eq!(s.monthly.values[0], vec![3]);

    // Seg (dia 1): 3 mensagens; Qua (dia 3): 2.
    assert_eq!(s.weekday.totals[0][0] + s.weekday.totals[1][0], 3);
    assert_eq!(s.weekday.occurrences, [1, 1, 1, 0, 0, 0, 0]);
    assert_eq!(s.weekday.most_active, 0);
    assert_eq!(s.hourly[1][22], 1);

    assert_eq!(
        s.top_words[0],
        Count {
            item: "amor".into(),
            count: 3
        }
    );
    assert!(s.top_words.iter().any(|c| c.item == "siim"));
    assert!(s.top_words.iter().any(|c| c.item == "kk"));
    assert!(!s.top_words.iter().any(|c| c.item.contains("exemplo")));
    assert_eq!(
        s.top_emojis[0],
        Count {
            item: "😍".into(),
            count: 2
        }
    );
    assert!(s.top_emojis_by_author[1].is_empty());
}

#[test]
fn response_times_rules() {
    let mut b = Builder::new();
    b.text(ts(1, 10, 0, 0), 0, "a")
        .text(ts(1, 10, 0, 30), 0, "a2") // bloco da Ana: conta a partir da última
        .text(ts(1, 10, 1, 30), 1, "b") // Bruno: 60s
        .push(ts(1, 10, 2, 0), Some(0), MessageKind::Deleted, None) // ignorada
        .text(ts(1, 10, 11, 30), 0, "a3") // Ana: 600s
        .text(ts(1, 20, 0, 0), 1, "b2"); // nova sessão (> 6h): descartada
    let s = compute(&authors(), false, &b.msgs, &HashMap::new(), &StatsSettings::default());
    assert_eq!(s.response_times[1].count, 1);
    assert_eq!(s.response_times[1].median_secs, 60.0);
    assert_eq!(s.response_times[1].buckets, [0, 1, 0, 0]);
    assert_eq!(s.response_times[0].count, 1);
    assert_eq!(s.response_times[0].median_secs, 600.0);
    assert_eq!(s.fastest_responder, Some(1));
}

#[test]
fn exclusive_words() {
    let mut b = Builder::new();
    let mut t = ts(1, 0, 0, 0);
    for _ in 0..30 {
        b.text(t, 0, "mano mano cara legal");
        t += 10;
        b.text(t, 1, "cara legal demais");
        t += 10;
    }
    b.text(t, 1, "mano");
    let s = compute(&authors(), false, &b.msgs, &HashMap::new(), &StatsSettings::default());
    let ana = &s.exclusive_words[0];
    assert!(ana.only_you.is_empty());
    let mano = ana.much_more.iter().find(|w| w.word == "mano").expect("mano");
    assert_eq!((mano.count, mano.other_count), (60, 1));
    assert!(mano.ratio.unwrap() > 30.0);
    let bruno = &s.exclusive_words[1];
    assert_eq!(bruno.only_you[0].word, "demais");
    // "cara"/"legal" são usadas por ambos com a mesma frequência absoluta: não é "muito mais".
    assert!(!bruno.much_more.iter().any(|w| w.word == "cara"));
}

#[test]
fn text_chart() {
    let chart = weekday_text_chart(&[10, 5, 0, 20, 1, 0, 0]);
    let lines: Vec<&str> = chart.lines().collect();
    assert_eq!(lines[3], format!("Qui  {}", "█".repeat(25)));
    assert_eq!(lines[1].chars().filter(|&c| c == '█').count(), 6);
    assert_eq!(lines[2], "Qua  ");
    assert_eq!(lines[4].chars().filter(|&c| c == '█').count(), 1);
}

#[test]
fn normalization() {
    use text::*;
    assert_eq!(collapse_repeats("kkkkkk"), "kk");
    assert_eq!(collapse_repeats("carro"), "carro");
    assert_eq!(normalize("Siiiiim", false).as_deref(), Some("siim"));
    assert_eq!(normalize("2024", false), None);
    assert_eq!(normalize("É", true), None); // 1 letra
    assert_eq!(normalize("Você", true).as_deref(), Some("voce"));
}

#[test]
fn stickers_and_audio() {
    let mut b = Builder::new();
    let t = ts(1, 10, 0, 0);
    let mut media = HashMap::new();
    let mut add = |file: &str, sha: &str, phash: Option<u64>, dur: Option<i64>| {
        media.insert(
            file.to_string(),
            MediaRow {
                file: file.into(),
                sha256: Some(sha.into()),
                phash: phash.map(|h| format!("{h:016x}")),
                duration_ms: dur,
            },
        );
    };
    // "a" e "a2" são a mesma figurinha reencodada (dHash a 1 bit); "b" é outra.
    add("STK-1.webp", "a", Some(0xF0F0_F0F0_F0F0_F0F0), None);
    add("STK-2.webp", "a", Some(0xF0F0_F0F0_F0F0_F0F0), None);
    add("STK-3.webp", "a2", Some(0xF0F0_F0F0_F0F0_F0F1), None);
    add("STK-4.webp", "b", Some(0x0F0F_0F0F_0F0F_0F0F), None);
    add("PTT-1.opus", "x", None, Some(30_000));
    add("PTT-2.opus", "y", None, Some(90_000));
    for (i, (who, kind, file)) in [
        (0, MessageKind::Sticker, "STK-1.webp"),
        (0, MessageKind::Sticker, "STK-2.webp"),
        (1, MessageKind::Sticker, "STK-3.webp"),
        (1, MessageKind::Sticker, "STK-4.webp"),
        (1, MessageKind::Sticker, "STK-ausente.webp"),
        (0, MessageKind::Audio, "PTT-1.opus"),
        (0, MessageKind::Audio, "PTT-2.opus"),
        (1, MessageKind::Audio, "PTT-ausente.opus"),
    ]
    .into_iter()
    .enumerate()
    {
        b.push(t + i as i64 * 10, Some(who), kind, None);
        b.msgs.last_mut().unwrap().media_file = Some(file.into());
    }
    let s = compute(&authors(), true, &b.msgs, &media, &StatsSettings::default());
    let m = s.media.unwrap();
    assert_eq!(m.distinct_stickers, 2);
    assert_eq!(m.missing_stickers, 1);
    assert_eq!(m.top_stickers[0].count, 3);
    assert_eq!(m.top_stickers[0].variants, 2);
    // Representante: a variante mais usada ("a", arquivo de menor nome em empate).
    assert_eq!(m.top_stickers[0].file, "STK-1.webp");
    assert_eq!(m.top_stickers_by_author[1].len(), 2);
    assert_eq!(m.top_stickers_by_author[0][0].count, 2);
    assert_eq!(
        m.audio[0],
        AudioStats {
            count: 2,
            with_duration: 2,
            total_ms: 120_000,
            avg_ms: 60_000.0
        }
    );
    assert_eq!(m.audio[1].count, 1);
    assert_eq!(m.audio[1].with_duration, 0);

    // Com distância 0, as variantes deixam de ser agrupadas.
    let strict = StatsSettings {
        sticker_max_distance: 0,
        ..Default::default()
    };
    assert_eq!(
        compute(&authors(), true, &b.msgs, &media, &strict)
            .media
            .unwrap()
            .distinct_stickers,
        3
    );
    // Sem mídia: não há seção de mídia.
    assert!(
        compute(&authors(), false, &b.msgs, &HashMap::new(), &StatsSettings::default())
            .media
            .is_none()
    );
}

#[test]
fn settings_without_new_fields_still_load() {
    let old =
        r#"{"sessionGapSecs":21600,"minExclusiveFreq":5,"stopwords":[],"stripAccents":false,"groupSkinTones":false}"#;
    let s: StatsSettings = serde_json::from_str(old).unwrap();
    assert_eq!(s.sticker_max_distance, 4);
}
