//! Anonimiza um export real do WhatsApp para virar fixture de teste do parser.
//!
//! ```text
//! cargo run -p synth --bin anonymize -- "Conversa do WhatsApp com Fulano.txt" -o crates/core-parser/tests/fixtures/android-real.txt
//! ```
//!
//! O que é preservado: o formato de cada linha (datas, horas, separadores, caracteres invisíveis do
//! iOS), a quebra em linhas, os marcadores de mídia, edição, mensagem apagada e sistema, emojis e
//! pontuação. O que muda: nomes dos autores viram "Pessoa A"/"Pessoa B"; cada palavra vira uma
//! palavra falsa do mesmo tamanho (a mesma palavra sempre vira a mesma, então as frequências e os
//! alongamentos como "siiiim" são mantidos); dígitos e URLs são trocados; nomes de documentos também.
//!
//! O mapeamento usa um sal aleatório a cada execução, então não dá para revertê-lo com um dicionário.
//! Mesmo assim, **revise o arquivo gerado antes de commitá-lo**.

use clap::Parser;
use core_parser::{ChatFormat, MessageKind, Source, classify_body, clean_line, detect_format, kind_from_file};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Parser)]
#[command(about = "Anonimiza um export do WhatsApp (.txt ou .zip) para uso como fixture de teste")]
struct Args {
    /// Export original (.txt ou .zip).
    input: PathBuf,
    /// Arquivo .txt anonimizado a gerar.
    #[arg(short, long)]
    out: PathBuf,
    /// Nomes que substituem os autores, na ordem em que aparecem no arquivo.
    #[arg(long, default_value = "Pessoa A,Pessoa B")]
    names: String,
}

// Trechos preservados literalmente dentro de uma mensagem.
static PROTECTED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)<(?:mensagem editada|esta mensagem foi editada|this message was edited|mensaje editado|se editó este mensaje|mídia oculta|arquivo de mídia oculto|media omitted|multimedia omitido)>|<(?:anexado|attached|adjunto): [^>]+>",
    )
    .unwrap()
});

static ANDROID_ATTACHED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?)\.([a-z0-9]{2,5})((?: • [^()]*)? \((?:arquivo anexado|file attached|archivo adjunto)\))$")
        .unwrap()
});

static IOS_ATTACHED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(<(?:anexado|attached|adjunto): )([^>]+)>$").unwrap());

static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(?:https?://|www\.)\S+").unwrap());

const CONSONANTS: &[u8] = b"bcdfglmnprstvz";
const VOWELS: &[u8] = b"aeiou";

fn strip_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        _ => c,
    }
}

struct Anonymizer {
    salt: u64,
    rng: StdRng,
    words: HashMap<String, Vec<char>>,
    used: HashSet<Vec<char>>,
    docs: HashMap<String, String>,
    authors: Vec<(String, String)>,
}

impl Anonymizer {
    /// Palavra falsa (base sem letras repetidas) para uma "chave" de palavra.
    fn base_for(&mut self, key: &str, len: usize) -> Vec<char> {
        if let Some(b) = self.words.get(key) {
            return b.clone();
        }
        let mut attempt = 0u64;
        let base = loop {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (self.salt, key, attempt).hash(&mut h);
            let mut r = StdRng::seed_from_u64(h.finish());
            let start_vowel = r.random_bool(0.3);
            let cand: Vec<char> = (0..len)
                .map(|i| {
                    let set = if (i % 2 == 0) == start_vowel {
                        VOWELS
                    } else {
                        CONSONANTS
                    };
                    set[r.random_range(0..set.len())] as char
                })
                .collect();
            // Duas palavras reais não podem virar a mesma palavra falsa.
            if !self.used.contains(&cand) || attempt > 20 {
                break cand;
            }
            attempt += 1;
        };
        self.used.insert(base.clone());
        self.words.insert(key.to_string(), base.clone());
        base
    }

    /// Mantém tamanho, maiúsculas e alongamentos: "Siiiim" → "Taaaar" (chave "sim").
    fn fake_word(&mut self, word: &str) -> String {
        let mut runs: Vec<(char, usize, bool)> = Vec::new();
        for c in word.chars() {
            let low = strip_accent(c.to_lowercase().next().unwrap_or(c));
            match runs.last_mut() {
                Some((prev, n, _)) if *prev == low => *n += 1,
                _ => runs.push((low, 1, c.is_uppercase())),
            }
        }
        let key: String = runs.iter().map(|r| r.0).collect();
        let base = self.base_for(&key, runs.len());
        runs.iter()
            .zip(base)
            .flat_map(|(&(_, n, upper), b)| {
                let c = if upper { b.to_ascii_uppercase() } else { b };
                std::iter::repeat_n(c, n)
            })
            .collect()
    }

    /// Anonimiza texto livre: palavras, dígitos e URLs. O resto (espaços, pontuação, emojis) fica.
    fn text(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut last = 0;
        for m in URL_RE.find_iter(s) {
            self.plain(&s[last..m.start()], &mut out);
            out.push_str("https://exemplo.com/link");
            last = m.end();
        }
        self.plain(&s[last..], &mut out);
        out
    }

    fn plain(&mut self, s: &str, out: &mut String) {
        let mut word = String::new();
        for c in s.chars() {
            if c.is_alphabetic() {
                word.push(c);
                continue;
            }
            if !word.is_empty() {
                out.push_str(&self.fake_word(&word));
                word.clear();
            }
            if c.is_ascii_digit() {
                out.push((b'0' + self.rng.random_range(0..10u8)) as char);
            } else {
                out.push(c);
            }
        }
        if !word.is_empty() {
            out.push_str(&self.fake_word(&word));
        }
    }

    /// Texto com trechos protegidos (marcadores de mídia e edição) preservados.
    fn with_protected(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut last = 0;
        for m in PROTECTED_RE.find_iter(s) {
            out.push_str(&self.text(&s[last..m.start()]));
            out.push_str(&self.attachment_marker(m.as_str()));
            last = m.end();
        }
        out.push_str(&self.text(&s[last..]));
        out
    }

    /// Documentos podem ter nomes pessoais ("Contrato Fulano.pdf"): troca pelo padrão do WhatsApp.
    fn doc_name(&mut self, stem: &str, ext: &str) -> String {
        let n = self.docs.len() + 1;
        self.docs
            .entry(format!("{stem}.{ext}"))
            .or_insert_with(|| format!("DOC-{n:04}.{ext}"))
            .clone()
    }

    fn attachment_marker(&mut self, marker: &str) -> String {
        let Some(c) = IOS_ATTACHED_RE.captures(marker) else {
            return marker.to_string();
        };
        let file = &c[2];
        if kind_from_file(file) != MessageKind::Doc {
            return marker.to_string();
        }
        let (stem, ext) = file.rsplit_once('.').unwrap_or((file, "bin"));
        format!("{}{}>", &c[1], self.doc_name(stem, ext))
    }

    fn author_alias(&self, raw: &str) -> String {
        let clean = clean_line(raw);
        let clean = clean.trim();
        self.authors
            .iter()
            .find(|(real, _)| real == clean)
            .map(|(_, alias)| alias.clone())
            .unwrap_or_else(|| "Pessoa X".into())
    }

    /// Mensagens de sistema ficam legíveis, mas sem nomes nem números de telefone.
    fn system(&mut self, s: &str) -> String {
        let mut s = s.to_string();
        for (real, alias) in &self.authors {
            s = s.replace(real.as_str(), alias);
        }
        s.chars()
            .map(|c| {
                if c.is_ascii_digit() {
                    (b'0' + self.rng.random_range(0..10u8)) as char
                } else {
                    c
                }
            })
            .collect()
    }

    fn body(&mut self, has_author: bool, raw: &str) -> String {
        let marked = raw.starts_with('\u{200E}');
        let c = classify_body(has_author, &clean_line(raw), marked);
        match c.kind {
            MessageKind::System => self.system(raw),
            MessageKind::Deleted => raw.to_string(),
            // Marcadores sem arquivo ("<Mídia oculta>", "imagem ocultada"…) ficam como estão.
            _ if c.kind != MessageKind::Text && c.media_file.is_none() && c.text.is_none() => raw.to_string(),
            _ => {
                if let Some(a) = ANDROID_ATTACHED_RE.captures(raw.trim_start_matches('\u{200E}')) {
                    let (stem, ext, tail) = (&a[1], &a[2], &a[3]);
                    let file = format!("{stem}.{ext}");
                    let name = if kind_from_file(&file) == MessageKind::Doc {
                        self.doc_name(stem, ext)
                    } else {
                        file
                    };
                    let lead = &raw[..raw.len() - raw.trim_start_matches('\u{200E}').len()];
                    return format!("{lead}{name}{tail}");
                }
                self.with_protected(raw)
            }
        }
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let source = Source::open(&args.input)?;
    let format: ChatFormat = source.with_reader(|r, _| detect_format(r).map_err(Box::<dyn std::error::Error>::from))?;
    let aliases: Vec<&str> = args.names.split(',').map(str::trim).collect();
    let mut anon = Anonymizer {
        salt: rand::rng().random(),
        rng: StdRng::seed_from_u64(rand::rng().random()),
        words: HashMap::new(),
        used: HashSet::new(),
        docs: HashMap::new(),
        authors: format
            .authors
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    a.clone(),
                    aliases
                        .get(i)
                        .map_or_else(|| format!("Pessoa {}", i + 1), |s| s.to_string()),
                )
            })
            .collect(),
    };
    let mut out = BufWriter::new(std::fs::File::create(&args.out)?);
    let mut in_system = false;
    let lines = source.with_reader(|r, _| {
        let mut buf = Vec::new();
        let mut n = 0u64;
        loop {
            buf.clear();
            if r.read_until(b'\n', &mut buf)? == 0 {
                break;
            }
            n += 1;
            let raw = String::from_utf8_lossy(&buf);
            let eol = if raw.ends_with("\r\n") {
                "\r\n"
            } else if raw.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            let line = &raw[..raw.len() - eol.len()];
            let anonymized = match format.split_line(line) {
                Some(p) => {
                    let body = anon.body(p.author.is_some(), p.body);
                    in_system = p.author.is_none();
                    match p.author {
                        Some(a) => format!("{}{}: {body}", p.prefix, anon.author_alias(a)),
                        None => format!("{}{body}", p.prefix),
                    }
                }
                // Continuação de mensagem (legenda ou texto multilinha).
                None if in_system => anon.system(line),
                None => anon.with_protected(line),
            };
            out.write_all(anonymized.as_bytes())?;
            out.write_all(eol.as_bytes())?;
        }
        Ok::<_, Box<dyn std::error::Error>>(n)
    })?;
    out.flush()?;
    eprintln!(
        "{lines} linhas anonimizadas → {} ({} palavras distintas). Revise o arquivo antes de commitá-lo.",
        args.out.display(),
        anon.words.len()
    );
    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(&args) {
        eprintln!("erro: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_parser::MessageReader;
    use std::io::Cursor;

    const ANDROID: &str = "\
12/01/2023 14:30 - As mensagens e ligações são protegidas com a criptografia de ponta a ponta.
12/01/2023 14:30 - Ana Souza: Oi Bruno! Siiiim, amanhã às 18h 😍
12/01/2023 14:31 - Bruno: kkkkkk sim sim
segunda linha https://maps.google.com/?q=123
13/01/2023 09:00 - Ana Souza: <Mídia oculta>
13/01/2023 09:01 - Bruno: Mensagem apagada
13/01/2023 09:02 - Ana Souza: vou sim <Mensagem editada>
13/01/2023 09:03 - Bruno: STK-20230113-WA0001.webp (arquivo anexado)
13/01/2023 09:04 - Ana Souza: Contrato Bruno Silva.pdf • 3 páginas (arquivo anexado)
13/01/2023 09:05 - Bruno: IMG-20230113-WA0003.jpg (arquivo anexado)
legenda da foto
13/01/2023 09:06 - Bruno mudou o número de telefone para +55 11 91234-5678.
";

    const IOS: &str = "\
[12/01/2023, 14:30:15] Ana: \u{200E}As mensagens e as ligações são protegidas com a criptografia de ponta a ponta.
[12/01/2023, 14:30:20] Ana: oi, tudo bem?
\u{200E}[12/01/2023, 14:31:02] Bruno: \u{200E}<anexado: 00000003-STICKER-2023-01-12-14-31-02.webp>
\u{200E}[12/01/2023, 14:31:30] Bruno: \u{200E}<anexado: 00000004-Planilha Gastos.xlsx>
[12/01/2023, 14:32:00] Bruno: \u{200E}imagem ocultada
[12/01/2023, 14:33:00] Ana: tá bom \u{200E}<Esta mensagem foi editada>
";

    fn anonymize(input: &str, name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("wr-anon-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (i, o) = (dir.join("in.txt"), dir.join("out.txt"));
        std::fs::write(&i, input).unwrap();
        run(&Args {
            input: i,
            out: o.clone(),
            names: "Pessoa A,Pessoa B".into(),
        })
        .unwrap();
        let out = std::fs::read_to_string(&o).unwrap();
        std::fs::remove_dir_all(dir).ok();
        out
    }

    fn parse(s: &str) -> Vec<core_parser::ParsedMessage> {
        let f = detect_format(Cursor::new(s)).unwrap();
        MessageReader::new(Cursor::new(s), f).map(Result::unwrap).collect()
    }

    fn check_same_structure(original: &str, name: &str) -> String {
        let anon = anonymize(original, name);
        let (a, b) = (parse(original), parse(&anon));
        assert_eq!(a.len(), b.len());
        assert_eq!(anon.lines().count(), original.lines().count());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.ts, y.ts);
            assert_eq!(x.kind, y.kind, "{x:?} → {y:?}");
            assert_eq!(x.author.is_some(), y.author.is_some());
            assert_eq!(x.media_file.is_some(), y.media_file.is_some());
            assert_eq!(x.text.as_ref().map(|t| t.chars().count()).is_some(), y.text.is_some());
        }
        anon
    }

    #[test]
    fn android_structure_and_privacy() {
        let anon = check_same_structure(ANDROID, "android");
        for leaked in [
            "Ana",
            "Souza",
            "Bruno",
            "Silva",
            "Contrato",
            "maps.google",
            "91234",
            "amanhã",
            "legenda",
        ] {
            assert!(!anon.contains(leaked), "vazou {leaked:?}:\n{anon}");
        }
        assert!(anon.contains("Pessoa A: ") && anon.contains("Pessoa B: "));
        assert!(anon.contains("STK-20230113-WA0001.webp (arquivo anexado)"));
        assert!(anon.contains("DOC-0001.pdf • 3 páginas (arquivo anexado)"));
        assert!(
            anon.contains("<Mídia oculta>") && anon.contains("Mensagem apagada") && anon.contains("<Mensagem editada>")
        );
        assert!(anon.contains("😍"));
        assert!(
            anon.contains("criptografia de ponta a ponta"),
            "sistema continua legível"
        );
        // A mesma palavra vira sempre a mesma palavra falsa, com alongamentos preservados.
        let line = anon.lines().nth(2).unwrap();
        let words: Vec<&str> = line.split_whitespace().skip(5).collect();
        assert_eq!(words[1], words[2], "\"sim sim\" → mesma palavra");
        assert!(
            words[0].chars().all(|c| c == words[0].chars().next().unwrap()),
            "kkkkkk → letra repetida"
        );
    }

    #[test]
    fn ios_structure_and_privacy() {
        let anon = check_same_structure(IOS, "ios");
        assert!(anon.contains("\u{200E}[12/01/2023, 14:31:02] Pessoa B: \u{200E}<anexado: 00000003-STICKER"));
        assert!(anon.contains("<anexado: DOC-0001.xlsx>"));
        assert!(!anon.contains("Planilha") && !anon.contains("Gastos"));
        assert!(anon.contains("\u{200E}imagem ocultada") && anon.contains("\u{200E}<Esta mensagem foi editada>"));
    }
}
