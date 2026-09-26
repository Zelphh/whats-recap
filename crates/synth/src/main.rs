//! Gerador de conversas sintéticas no formato de export do WhatsApp.
//!
//! Permite demos, screenshots e testes de volume sem expor conversas reais.
//!
//! ```text
//! cargo run -p synth -- --days 1095 --platform ios -o /tmp/conversa.txt
//! cargo run -p synth -- --days 365 --media -o /tmp/conversa.zip   # com figurinhas e áudios
//! ```

use chrono::{Duration, NaiveDate, NaiveDateTime};
use clap::{Parser, ValueEnum};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
enum Platform {
    Android,
    Ios,
}

#[derive(Parser)]
#[command(about = "Gera uma conversa sintética de WhatsApp entre duas pessoas")]
struct Args {
    /// Formato do export.
    #[arg(long, value_enum, default_value = "android")]
    platform: Platform,
    /// Quantidade de dias de conversa.
    #[arg(long, default_value_t = 365)]
    days: u32,
    /// Data da primeira mensagem (AAAA-MM-DD).
    #[arg(long, default_value = "2023-01-01")]
    start: NaiveDate,
    /// Nomes das duas pessoas, separados por vírgula.
    #[arg(long, default_value = "Ana,Bruno")]
    names: String,
    /// Média de mensagens por dia ativo.
    #[arg(long, default_value_t = 60)]
    per_day: u32,
    /// Semente do gerador (a mesma semente gera a mesma conversa).
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Gera um export com mídia (.zip com figurinhas e áudios). Exige `--out`.
    #[arg(long)]
    media: bool,
    /// Arquivo de saída (padrão: stdout).
    #[arg(short, long)]
    out: Option<PathBuf>,
}

const COMMON: &[&str] = &[
    "bom dia",
    "boa noite",
    "tudo bem?",
    "tudo e vc?",
    "saudades",
    "chegou em casa?",
    "cheguei",
    "já almoçou?",
    "to indo",
    "kkkkkk",
    "kkkkkkkkk",
    "sério?",
    "que isso",
    "verdade",
    "sim",
    "não sei",
    "vamos ver um filme hoje?",
    "bora",
    "pode ser",
    "to com fome",
    "vou tomar banho",
    "voltei",
    "hoje foi corrido",
    "trabalho tá puxado",
    "amanhã a gente vê isso",
    "te amo",
    "também te amo",
    "que horas vc sai?",
    "saio às 18h",
    "me liga quando puder",
    "olha isso",
    "acabei de ver",
    "quer pedir pizza?",
    "quero!",
    "vamos na casa da sua mãe domingo?",
    "combinado",
    "comprei as passagens pra Floripa",
    "a viagem vai ser incrível",
    "não esquece do aniversário do Pedro",
    "o que acha de ir no show?",
    "nossa, que dia",
    "dormi mal",
    "vou dormir",
    "sonhei com vc",
];

const PERSONAL: [&[&str]; 2] = [
    &[
        "amiga, você não sabe",
        "nossa senhora",
        "aff",
        "maravilhoso",
        "lindoo",
        "socorro",
        "gente",
    ],
    &["mano", "suave", "tranquilo", "firmeza", "véi", "top demais", "de boa"],
];

const EMOJIS: &[&str] = &[
    "😂",
    "❤️",
    "😍",
    "🥰",
    "😘",
    "👍",
    "👍🏽",
    "🙏",
    "😅",
    "😭",
    "🔥",
    "🇧🇷",
    "👨‍👩‍👧",
];

const CONFLICT: &[&str] = &[
    "você nunca me escuta",
    "lá vem",
    "SÉRIO QUE VOCÊ FEZ ISSO??",
    "tanto faz",
    "ok.",
    "blz",
    "não quero falar sobre isso agora",
    "você sempre faz isso!!",
    "esquece",
];

const APOLOGY: &[&str] = &[
    "desculpa, exagerei",
    "foi mal",
    "me perdoa?",
    "desculpa a demora",
    "perdão amor",
];

/// Quantidade de figurinhas distintas no "acervo" das duas pessoas.
const STICKER_DESIGNS: u32 = 40;

/// Arquivos de mídia acumulados para o .zip (modo `--media`).
#[derive(Default)]
struct MediaSink {
    files: Vec<(String, Vec<u8>)>,
    stickers: HashMap<(u32, u32), Vec<u8>>,
    counter: u32,
}

struct Gen {
    rng: StdRng,
    names: [String; 2],
    platform: Platform,
    media: Option<MediaSink>,
}

impl Gen {
    fn pick<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[self.rng.random_range(0..list.len())]
    }

    fn text(&mut self, who: usize) -> String {
        let mut t = if self.rng.random_bool(0.2) {
            self.pick(PERSONAL[who]).to_string()
        } else {
            self.pick(COMMON).to_string()
        };
        if self.rng.random_bool(0.15) {
            t = format!("{t} {}", self.pick(PERSONAL[who]));
        }
        if self.rng.random_bool(0.2) {
            let n = self.rng.random_range(1..=3);
            let e = self.pick(EMOJIS);
            t = format!("{t} {}", e.repeat(n));
        }
        if self.rng.random_bool(0.03) {
            t = format!("{t}\n{}", self.pick(COMMON));
        }
        t
    }

    /// Nome de arquivo no padrão do WhatsApp e texto da mensagem que o referencia.
    fn attachment(&mut self, ts: NaiveDateTime, android_prefix: &str, ios_kind: &str, ext: &str) -> (String, String) {
        let sink = self.media.as_mut().unwrap();
        sink.counter += 1;
        match self.platform {
            Platform::Android => {
                let name = format!(
                    "{android_prefix}-{}-WA{:04}.{ext}",
                    ts.format("%Y%m%d"),
                    sink.counter % 10_000
                );
                (name.clone(), format!("{name} (arquivo anexado)"))
            }
            Platform::Ios => {
                let name = format!(
                    "{:08}-{ios_kind}-{}.{ext}",
                    sink.counter,
                    ts.format("%Y-%m-%d-%H-%M-%S")
                );
                (name.clone(), format!("\u{200E}<anexado: {name}>"))
            }
        }
    }

    fn sticker(&mut self, who: usize, ts: NaiveDateTime) -> String {
        // Popularidade enviesada (poucas figurinhas muito usadas) e um "gosto" diferente por pessoa.
        let u: f64 = self.rng.random();
        let mut design = (u * u * STICKER_DESIGNS as f64) as u32;
        if who == 1 && self.rng.random_bool(0.5) {
            design = (design + STICKER_DESIGNS / 2) % STICKER_DESIGNS;
        }
        // Às vezes a mesma figurinha chega reencodada (outro SHA-256, mesma imagem).
        let variant = if self.rng.random_bool(0.15) {
            self.rng.random_range(1..=3)
        } else {
            0
        };
        let (name, text) = self.attachment(ts, "STK", "STICKER", "webp");
        let sink = self.media.as_mut().unwrap();
        let bytes = sink
            .stickers
            .entry((design, variant))
            .or_insert_with(|| core_media::testdata::sticker_webp(design, variant))
            .clone();
        sink.files.push((name, bytes));
        text
    }

    fn audio(&mut self, who: usize, ts: NaiveDateTime) -> String {
        let secs = if who == 0 {
            self.rng.random_range(5..90)
        } else {
            self.rng.random_range(3..40)
        };
        let ms = secs * 1000 + self.rng.random_range(0..1000);
        let (name, text) = self.attachment(ts, "PTT", "AUDIO", "opus");
        self.media
            .as_mut()
            .unwrap()
            .files
            .push((name, core_media::testdata::opus_file(ms)));
        text
    }

    fn body(&mut self, who: usize, idx: usize, ts: NaiveDateTime) -> String {
        let ios = matches!(self.platform, Platform::Ios);
        let r: f64 = self.rng.random();
        let stamp = ts.format("%Y%m%d");
        if self.media.is_some() && r < 0.06 {
            return if r < 0.03 {
                self.attachment(ts, "IMG", "PHOTO", "jpg").1
            } else if r < 0.045 {
                self.sticker(who, ts)
            } else {
                self.audio(who, ts)
            };
        }
        if r < 0.03 {
            if ios {
                "\u{200E}imagem ocultada".into()
            } else {
                "<Mídia oculta>".into()
            }
        } else if r < 0.045 {
            if ios {
                "\u{200E}figurinha omitida".into()
            } else {
                "<Mídia oculta>".into()
            }
        } else if r < 0.06 {
            if ios {
                "\u{200E}áudio ocultado".into()
            } else {
                "<Mídia oculta>".into()
            }
        } else if r < 0.065 {
            if ios {
                "\u{200E}Esta mensagem foi apagada.".into()
            } else {
                "Mensagem apagada".into()
            }
        } else if r < 0.07 {
            let t = self.text(who);
            if ios {
                format!("{t} \u{200E}<Esta mensagem foi editada>")
            } else {
                format!("{t} <Mensagem editada>")
            }
        } else if r < 0.072 {
            format!("olha esse link https://exemplo.com/{stamp}/{idx}")
        } else {
            self.text(who)
        }
    }

    fn line(&self, ts: NaiveDateTime, who: Option<usize>, body: &str) -> String {
        let prefix = match self.platform {
            Platform::Android => ts.format("%d/%m/%Y %H:%M - ").to_string(),
            Platform::Ios => ts.format("[%d/%m/%Y, %H:%M:%S] ").to_string(),
        };
        match who {
            Some(w) => format!("{prefix}{}: {body}", self.names[w]),
            None => format!("{prefix}{body}"),
        }
    }

    /// Atraso até a próxima mensagem: curto no mesmo bloco, variado nas respostas.
    fn delay(&mut self, same_author: bool, who: usize) -> i64 {
        if same_author {
            return self.rng.random_range(3..60);
        }
        // A segunda pessoa responde um pouco mais devagar, para o dashboard ter contraste.
        let slow = if who == 1 { 1.6 } else { 1.0 };
        let r: f64 = self.rng.random();
        let secs = if r < 0.8 {
            self.rng.random_range(5..120)
        } else if r < 0.97 {
            self.rng.random_range(120..900)
        } else {
            self.rng.random_range(900..5400)
        };
        (secs as f64 * slow) as i64
    }
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();
    let names: Vec<String> = args.names.split(',').map(|s| s.trim().to_string()).collect();
    assert!(names.len() == 2, "--names precisa de exatamente dois nomes");
    let mut g = Gen {
        rng: StdRng::seed_from_u64(args.seed),
        names: [names[0].clone(), names[1].clone()],
        platform: args.platform,
        media: args.media.then(MediaSink::default),
    };
    if args.media && args.out.is_none() {
        eprintln!("--media exige --out <arquivo.zip>");
        std::process::exit(2);
    }
    let mut chat = Vec::new();
    let mut out: Box<dyn Write> = match (&args.out, args.media) {
        (_, true) => Box::new(&mut chat),
        (Some(p), false) => Box::new(BufWriter::new(std::fs::File::create(p)?)),
        (None, false) => Box::new(BufWriter::new(std::io::stdout().lock())),
    };

    let start = args.start.and_hms_opt(9, 0, 0).unwrap();
    let encryption = "As mensagens e ligações são protegidas com a criptografia de ponta a ponta e ficam somente entre você e os participantes desta conversa. Nem mesmo o WhatsApp pode ler ou ouvi-las.";
    let first = match args.platform {
        Platform::Android => g.line(start, None, encryption),
        Platform::Ios => g.line(start, Some(0), &format!("\u{200E}{encryption}")),
    };
    writeln!(out, "{first}")?;

    let mut idx = 0usize;
    let mut last = start;
    for day in 0..args.days {
        let date = args.start + Duration::days(day as i64);
        // Fins de semana um pouco mais ativos; alguns dias sem conversa.
        let weekend = matches!(
            chrono::Datelike::weekday(&date),
            chrono::Weekday::Sat | chrono::Weekday::Sun
        );
        if !g.rng.random_bool(if weekend { 0.95 } else { 0.85 }) {
            continue;
        }
        let sessions = g.rng.random_range(1..=3);
        let mut remaining = (args.per_day as f64 * g.rng.random_range(0.3..1.7)) as usize + 1;
        let mut hours: Vec<u32> = (0..sessions).map(|_| g.rng.random_range(7..24)).collect();
        hours.sort_unstable();
        let conflict_day = g.rng.random_bool(0.03);
        for (s, &hour) in hours.iter().enumerate() {
            let mut ts = date
                .and_hms_opt(hour, g.rng.random_range(0..60), g.rng.random_range(0..60))
                .unwrap();
            if ts <= last {
                ts = last + Duration::seconds(30);
            }
            let n = if s + 1 == sessions {
                remaining
            } else {
                remaining / (sessions - s)
            };
            remaining -= n;
            let mut who = g.rng.random_range(0..2);
            let conflict_at = conflict_day.then(|| g.rng.random_range(0..n.max(1)));
            let mut i = 0;
            while i < n {
                let body = if Some(i) == conflict_at {
                    // Pequena briga seguida de um pedido de desculpas.
                    for k in 0..6 {
                        let c = g.pick(CONFLICT).to_string();
                        writeln!(out, "{}", g.line(ts, Some((who + k) % 2), &c))?;
                        ts += Duration::seconds(g.rng.random_range(5..40));
                    }
                    g.pick(APOLOGY).to_string()
                } else {
                    g.body(who, idx, ts)
                };
                writeln!(out, "{}", g.line(ts, Some(who), &body))?;
                idx += 1;
                i += 1;
                let next = if g.rng.random_bool(0.55) { 1 - who } else { who };
                ts += Duration::seconds(g.delay(next == who, next));
                who = next;
            }
            last = ts;
        }
    }
    out.flush()?;
    drop(out);

    if let (Some(sink), Some(path)) = (g.media, &args.out) {
        let chat_name = match args.platform {
            Platform::Android => format!("Conversa do WhatsApp com {}.txt", g.names[1]),
            Platform::Ios => "_chat.txt".to_string(),
        };
        let mut zip = zip::ZipWriter::new(BufWriter::new(std::fs::File::create(path)?));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file(chat_name, opts)?;
        zip.write_all(&chat)?;
        // Mídia já comprimida: armazenada sem compressão, como o WhatsApp faz.
        let stored = opts.compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in &sink.files {
            zip.start_file(name.as_str(), stored)?;
            zip.write_all(bytes)?;
        }
        zip.finish()?;
    }
    Ok(())
}
