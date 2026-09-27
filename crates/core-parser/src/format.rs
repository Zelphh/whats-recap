use crate::classify::split_author;
use chrono::{NaiveDate, NaiveDateTime};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::BufRead;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// `dd/mm/aaaa hh:mm - Autor: msg`
    Android,
    /// `[dd/mm/aaaa, hh:mm:ss] Autor: msg`
    Ios,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    Dmy,
    Mdy,
    Ymd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatFormat {
    pub platform: Platform,
    pub order: DateOrder,
    /// Autores distintos encontrados na detecção (mensagens de sistema excluídas).
    pub authors: Vec<String>,
    /// Quantidade de cabeçalhos de mensagem reconhecidos.
    pub header_lines: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("erro de leitura: {0}")]
    Io(#[from] std::io::Error),
    #[error("nenhuma linha no formato de export do WhatsApp foi reconhecida")]
    Unrecognized,
}

// Espaços que aparecem entre data e hora conforme a versão/idioma (inclui NBSP e NNBSP do iOS).
const SP: &str = r"[ \x{00A0}\x{202F}]";
const INVIS: &str = r"[\x{200E}\x{200F}\x{FEFF}]*";

fn date_time_pattern() -> String {
    format!(
        r"(\d{{1,4}})[./-](\d{{1,2}})[./-](\d{{1,4}}),?{SP}+(\d{{1,2}}):(\d{{2}})(?::(\d{{2}}))?(?:{SP}*([aApP])\.?{SP}?[mM]\.?)?"
    )
}

static ANDROID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^{INVIS}{}{SP}+[-–]{SP}(.*)$", date_time_pattern())).unwrap());

static IOS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"^{INVIS}\[{}\]{SP}*(.*)$", date_time_pattern())).unwrap());

struct RawStamp {
    a: u32,
    b: u32,
    c: u32,
    a_len: usize,
    hour: u32,
    minute: u32,
    second: u32,
    ampm: Option<bool>, // Some(true) = PM
}

fn raw_stamp(c: &Captures) -> Option<RawStamp> {
    let n = |i: usize| c.get(i).map(|m| m.as_str().parse::<u32>().ok()).unwrap_or(Some(0));
    Some(RawStamp {
        a: n(1)?,
        b: n(2)?,
        c: n(3)?,
        a_len: c.get(1)?.len(),
        hour: n(4)?,
        minute: n(5)?,
        second: n(6)?,
        ampm: c.get(7).map(|m| m.as_str().eq_ignore_ascii_case("p")),
    })
}

fn full_year(y: u32) -> i32 {
    if y < 100 { 2000 + y as i32 } else { y as i32 }
}

impl RawStamp {
    fn date(&self, order: DateOrder) -> Option<NaiveDate> {
        let (y, m, d) = match order {
            DateOrder::Dmy => (self.c, self.b, self.a),
            DateOrder::Mdy => (self.c, self.a, self.b),
            DateOrder::Ymd => (self.a, self.b, self.c),
        };
        NaiveDate::from_ymd_opt(full_year(y), m, d)
    }

    fn datetime(&self, order: DateOrder) -> Option<NaiveDateTime> {
        let hour = match self.ampm {
            None => self.hour,
            Some(pm) => (self.hour % 12) + if pm { 12 } else { 0 },
        };
        self.date(order)?.and_hms_opt(hour, self.minute, self.second)
    }
}

/// Uma linha de cabeçalho quebrada em pedaços crus (sem limpeza), para ferramentas que
/// precisam reescrever o export preservando o formato (ex.: o anonimizador).
#[derive(Debug, PartialEq, Eq)]
pub struct LineParts<'a> {
    /// Data, hora e separador, exatamente como no arquivo (inclui `[`, `]`, ` - ` e invisíveis).
    pub prefix: &'a str,
    /// Nome do autor como aparece no arquivo; `None` em mensagens de sistema do Android.
    pub author: Option<&'a str>,
    /// Corpo da mensagem após `Autor: ` (ou o resto da linha, se não houver autor).
    pub body: &'a str,
}

pub(crate) struct Header {
    pub ts: NaiveDateTime,
    pub author: Option<String>,
    pub body: String,
    /// O corpo começava com `U+200E` (marca de mídia/sistema no iOS).
    pub marked: bool,
}

impl ChatFormat {
    fn regex(&self) -> &'static Regex {
        match self.platform {
            Platform::Android => &ANDROID_RE,
            Platform::Ios => &IOS_RE,
        }
    }

    /// Quebra um cabeçalho em prefixo, autor e corpo crus. `None` se a linha não for um cabeçalho.
    pub fn split_line<'a>(&self, line: &'a str) -> Option<LineParts<'a>> {
        let caps = self.regex().captures(line)?;
        let rest = caps.get(8)?;
        let prefix = &line[..rest.start()];
        let rest = rest.as_str();
        match split_author(rest) {
            (Some(_), body) => {
                let author_end = rest.len() - body.len() - 2;
                Some(LineParts {
                    prefix,
                    author: Some(&rest[..author_end]),
                    body,
                })
            }
            (None, body) => Some(LineParts {
                prefix,
                author: None,
                body,
            }),
        }
    }

    /// `None`: a linha não é um cabeçalho (continuação da mensagem anterior).
    /// `Some(Err)`: parece um cabeçalho, mas a data/hora é inválida.
    pub(crate) fn parse_header(&self, line: &str) -> Option<Result<Header, ()>> {
        let caps = self.regex().captures(line)?;
        let Some(ts) = raw_stamp(&caps).and_then(|s| s.datetime(self.order)) else {
            return Some(Err(()));
        };
        let rest = caps.get(8).map_or("", |m| m.as_str());
        let (author, body_raw) = split_author(rest);
        Some(Ok(Header {
            ts,
            author,
            marked: body_raw.starts_with('\u{200E}'),
            body: crate::clean_line(body_raw),
        }))
    }
}

#[derive(Default)]
struct Candidate {
    headers: u64,
    any_4digit_first: bool,
    invalid_dmy: u64,
    invalid_mdy: u64,
    inversions_dmy: u64,
    inversions_mdy: u64,
    prev_dmy: Option<NaiveDate>,
    prev_mdy: Option<NaiveDate>,
    authors: BTreeSet<String>,
}

impl Candidate {
    fn observe(&mut self, stamp: &RawStamp, rest: &str) {
        self.headers += 1;
        if stamp.a_len == 4 {
            self.any_4digit_first = true;
        }
        match stamp.date(DateOrder::Dmy) {
            Some(d) => {
                if self.prev_dmy.is_some_and(|p| d < p) {
                    self.inversions_dmy += 1;
                }
                self.prev_dmy = Some(d);
            }
            None => self.invalid_dmy += 1,
        }
        match stamp.date(DateOrder::Mdy) {
            Some(d) => {
                if self.prev_mdy.is_some_and(|p| d < p) {
                    self.inversions_mdy += 1;
                }
                self.prev_mdy = Some(d);
            }
            None => self.invalid_mdy += 1,
        }
        let (author, body_raw) = split_author(rest);
        if let Some(a) = author {
            let marked = body_raw.starts_with('\u{200E}');
            let kind = crate::classify_body(true, &crate::clean_line(body_raw), marked).kind;
            if kind != crate::MessageKind::System {
                self.authors.insert(a);
            }
        }
    }

    fn order(&self) -> DateOrder {
        if self.any_4digit_first {
            return DateOrder::Ymd;
        }
        // Um campo acima de 12 só pode ser o dia.
        match (self.invalid_dmy, self.invalid_mdy) {
            (0, m) if m > 0 => DateOrder::Dmy,
            (d, 0) if d > 0 => DateOrder::Mdy,
            (d, m) if d != m => {
                if d < m {
                    DateOrder::Dmy
                } else {
                    DateOrder::Mdy
                }
            }
            // Ambíguo: escolhe a interpretação que mantém as datas em ordem cronológica.
            _ if self.inversions_mdy < self.inversions_dmy => DateOrder::Mdy,
            _ => DateOrder::Dmy,
        }
    }
}

/// Percorre o export inteiro (em streaming) e determina plataforma, ordem da data e autores.
pub fn detect_format<R: BufRead>(mut reader: R) -> Result<ChatFormat, FormatError> {
    let mut android = Candidate::default();
    let mut ios = Candidate::default();
    let mut buf = Vec::with_capacity(512);
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        for (re, cand) in [(&*IOS_RE, &mut ios), (&*ANDROID_RE, &mut android)] {
            if let Some(caps) = re.captures(line) {
                if let Some(stamp) = raw_stamp(&caps) {
                    cand.observe(&stamp, caps.get(8).map_or("", |m| m.as_str()));
                }
                break;
            }
        }
    }
    let (platform, cand) = if ios.headers > android.headers {
        (Platform::Ios, ios)
    } else {
        (Platform::Android, android)
    };
    if cand.headers == 0 {
        return Err(FormatError::Unrecognized);
    }
    Ok(ChatFormat {
        platform,
        order: cand.order(),
        header_lines: cand.headers,
        authors: cand.authors.into_iter().collect(),
    })
}
