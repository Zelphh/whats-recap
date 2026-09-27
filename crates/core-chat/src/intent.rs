//! Interpretação heurística de perguntas quantitativas comuns, sem LLM: escolhe a ferramenta e
//! os parâmetros para os formatos óbvios ("quantas vezes o Bruno falou 'amor'?"). O que não se
//! encaixa retorna `None` e, com o LLM disponível, é interpretado por ele.

use crate::router::fold;
use crate::tools::{Granularity, ToolCall};
use chrono::NaiveDate;
use regex::Regex;
use std::sync::LazyLock;

/// Faixa de horas (inclusiva) e o nome da parte do dia.
pub type HourRange = (u8, u8, &'static str);

/// O que a resposta deve destacar a partir do resultado da ferramenta.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    CountWord,
    FirstOccurrence,
    /// Quem manda mais (ou menos) mensagens, opcionalmente numa parte do dia.
    WhoMost {
        most: bool,
        hours: Option<HourRange>,
    },
    TotalMessages {
        hours: Option<HourRange>,
    },
    PeakHour,
    TopPeriod {
        most: bool,
    },
    /// `compare`: "quem responde mais rápido?"; senão, o tempo de uma pessoa.
    ResponseTime {
        compare: bool,
    },
}

pub const PARTS_OF_DAY: [HourRange; 4] = [
    (0, 5, "madrugada"),
    (6, 11, "manhã"),
    (12, 17, "tarde"),
    (18, 23, "noite"),
];

const MONTHS: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "marco",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

static QUOTED_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"["“”'‘’«»]([^"“”'‘’«»]{1,60})["“”'‘’«»]"#).unwrap());

// Palavra depois do verbo: "falou amor", "disse a palavra saudade", "falou em viagem".
static AFTER_VERB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:falou|falei|falamos|falaram|disse|disseram|dissemos|escreveu|escrevi|escrevemos|mandou|mandei|usou|usei|usamos|digitou|digitei|chamou|chamei|palavra|termo|expressao)\s+(?:a palavra\s+|o termo\s+|a expressao\s+|em\s+|sobre\s+|de\s+)?(.+?)\s*[?!.]*$",
    )
    .unwrap()
});

static TRAILING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\s+(?:(?:pra|para|pro|com|a|ao)\s+(?:mim|ele|ela|voce|vc|a gente|nos|o \w+|a \w+)|na conversa|no chat|no whatsapp|nessa conversa|nesta conversa|ate hoje|no total|comigo|contigo|(?:em|no ano de|durante|de) \d{4}|em \w+ de \d{4}|entre \d{4} e \d{4}|de (?:madrugada|manha|tarde|noite)|(?:na|pela|a) (?:madrugada|manha|tarde|noite))$",
    )
    .unwrap()
});

static YEAR_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bentre (\d{4}) e (\d{4})\b").unwrap());
static MONTH_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(janeiro|fevereiro|marco|abril|maio|junho|julho|agosto|setembro|outubro|novembro|dezembro)(?: de)? (\d{4})\b")
        .unwrap()
});
static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b((?:19|20)\d{2})\b").unwrap());
static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9]+").unwrap());

static RESPONSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(respond\w*|resposta\w*|demora\w*|mais rapido|mais devagar|mais lento)\b").unwrap()
});
static PEAK_HOUR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(que horas?|qual (o |a )?(horario|hora)|horario|hora do dia)\b").unwrap());
static PERIOD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bqual (?:o |a |foi o |foi a )?(dia da semana|mes|ano|semana|dia)\b").unwrap());
static WHO_MOST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bquem\b.*\b(manda|envia|escreve|fala|conversa|digita|puxa|mensag)\w*").unwrap());

fn date(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

fn last_day(y: i32, m: u32) -> Option<NaiveDate> {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    date(ny, nm, 1)?.pred_opt()
}

/// Intervalo de datas mencionado: "entre 2022 e 2024", "em março de 2024", "em 2023".
pub fn date_range(q: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    if let Some(c) = YEAR_RANGE_RE.captures(q) {
        let (a, b): (i32, i32) = (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0));
        return (date(a.min(b), 1, 1), date(a.max(b), 12, 31));
    }
    if let Some(c) = MONTH_YEAR_RE.captures(q) {
        let m = MONTHS.iter().position(|x| *x == &c[1]).unwrap_or(0) as u32 + 1;
        let y: i32 = c[2].parse().unwrap_or(0);
        return (date(y, m, 1), last_day(y, m));
    }
    if let Some(c) = YEAR_RE.captures(q) {
        let y: i32 = c[1].parse().unwrap_or(0);
        return (date(y, 1, 1), date(y, 12, 31));
    }
    (None, None)
}

fn part_of_day(q: &str) -> Option<HourRange> {
    let words: Vec<&str> = WORD_RE.find_iter(q).map(|m| m.as_str()).collect();
    PARTS_OF_DAY.iter().copied().find(|(_, _, name)| {
        let folded = fold(name);
        words.iter().any(|w| *w == folded)
    })
}

/// A palavra ou expressão procurada: entre aspas, ou logo depois do verbo (até 3 palavras).
pub fn target_word(original: &str, q: &str) -> Option<String> {
    if let Some(c) = QUOTED_RE.captures(original) {
        let w = c[1].trim();
        return (!w.is_empty()).then(|| w.to_string());
    }
    let mut w = AFTER_VERB_RE.captures(q)?[1].trim().to_string();
    loop {
        let next = TRAILING_RE.replace(&w, "").trim().to_string();
        if next == w {
            break;
        }
        w = next;
    }
    let n = WORD_RE.find_iter(&w).count();
    (1..=3).contains(&n).then_some(w)
}

/// Autor mencionado pelo primeiro nome, se só um for citado (e fora da palavra procurada).
fn mentioned_author(q: &str, authors: &[String], exclude: Option<&str>) -> Option<String> {
    let excluded: Vec<String> = exclude
        .map(|w| WORD_RE.find_iter(&fold(w)).map(|m| m.as_str().to_string()).collect())
        .unwrap_or_default();
    let words: Vec<&str> = WORD_RE
        .find_iter(q)
        .map(|m| m.as_str())
        .filter(|w| !excluded.iter().any(|e| e == w))
        .collect();
    let hits: Vec<&String> = authors
        .iter()
        .filter(|a| {
            let first = fold(a.split_whitespace().next().unwrap_or_default());
            !first.is_empty() && words.contains(&first.as_str())
        })
        .collect();
    (hits.len() == 1).then(|| hits[0].clone())
}

/// Interpreta a pergunta. `authors` são os nomes da conversa (para reconhecer "o Bruno").
pub fn interpret(question: &str, authors: &[String]) -> Option<(ToolCall, Intent)> {
    let q = fold(question);
    let (from, to) = date_range(&q);
    let has = |s: &str| WORD_RE.find_iter(&q).any(|m| m.as_str() == s);
    let most = !has("menos");
    let word = target_word(question, &q);
    let author = mentioned_author(&q, authors, word.as_deref());

    if q.contains("primeira vez") {
        let word = word?;
        return Some((ToolCall::FirstOccurrence { word, author }, Intent::FirstOccurrence));
    }
    if q.contains("quantas vezes") || (has("quem") && word.is_some() && !RESPONSE_RE.is_match(&q)) {
        // "quem fala mais 'amor'?" compara as duas pessoas: sem filtro de autor.
        let author = if has("quem") { None } else { author };
        return Some((
            ToolCall::CountWord {
                word: word?,
                author,
                from,
                to,
            },
            Intent::CountWord,
        ));
    }
    if RESPONSE_RE.is_match(&q) {
        let compare = has("quem") || author.is_none();
        let author = if compare { None } else { author };
        return Some((
            ToolCall::ResponseTime { author, from, to },
            Intent::ResponseTime { compare },
        ));
    }
    if let Some(c) = PERIOD_RE.captures(&q)
        && (has("mais") || has("menos"))
    {
        let granularity = match &c[1] {
            "dia da semana" => Granularity::Weekday,
            "mes" => Granularity::Month,
            "ano" => Granularity::Year,
            "semana" => Granularity::Week,
            _ => Granularity::Day,
        };
        let author = if has("quem") { None } else { author };
        return Some((
            ToolCall::MessagesByPeriod {
                granularity,
                author,
                from,
                to,
            },
            Intent::TopPeriod { most },
        ));
    }
    let hours = part_of_day(&q);
    if PEAK_HOUR_RE.is_match(&q) && hours.is_none() {
        return Some((ToolCall::MessagesByHour { author, from, to }, Intent::PeakHour));
    }
    if WHO_MOST_RE.is_match(&q) && (has("mais") || has("menos")) {
        return Some((
            ToolCall::MessagesByHour { author: None, from, to },
            Intent::WhoMost { most, hours },
        ));
    }
    if q.contains("quantas mensagens") {
        return Some((
            ToolCall::MessagesByHour { author, from, to },
            Intent::TotalMessages { hours },
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authors() -> Vec<String> {
        vec!["Ana Souza".into(), "Bruno".into()]
    }

    fn call(q: &str) -> ToolCall {
        interpret(q, &authors())
            .unwrap_or_else(|| panic!("não interpretou: {q}"))
            .0
    }

    fn d(y: i32, m: u32, dd: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, dd)
    }

    #[test]
    fn count_word() {
        assert_eq!(
            call("Quantas vezes o Bruno falou 'amor'?"),
            ToolCall::CountWord {
                word: "amor".into(),
                author: Some("Bruno".into()),
                from: None,
                to: None
            }
        );
        assert_eq!(
            call("quantas vezes a ana disse te amo pra ele em 2024?"),
            ToolCall::CountWord {
                word: "te amo".into(),
                author: Some("Ana Souza".into()),
                from: d(2024, 1, 1),
                to: d(2024, 12, 31)
            }
        );
        assert_eq!(
            call("Quantas vezes falamos “saudade” em março de 2023"),
            ToolCall::CountWord {
                word: "saudade".into(),
                author: None,
                from: d(2023, 3, 1),
                to: d(2023, 3, 31)
            }
        );
        // O nome dentro da palavra procurada não vira filtro de autor.
        assert_eq!(
            call("quantas vezes eu falei 'bruno'?"),
            ToolCall::CountWord {
                word: "bruno".into(),
                author: None,
                from: None,
                to: None
            }
        );
        assert_eq!(
            call("quem fala mais 'kkk'?"),
            ToolCall::CountWord {
                word: "kkk".into(),
                author: None,
                from: None,
                to: None
            }
        );
    }

    #[test]
    fn first_occurrence() {
        assert_eq!(
            call("Quando foi a primeira vez que o Bruno disse te amo?"),
            ToolCall::FirstOccurrence {
                word: "te amo".into(),
                author: Some("Bruno".into())
            }
        );
        assert!(
            interpret(
                "quando foi a primeira vez que a gente brigou feio por causa de dinheiro",
                &authors()
            )
            .is_none()
        );
    }

    #[test]
    fn messages() {
        let (c, i) = interpret("quem manda mais mensagem de madrugada?", &authors()).unwrap();
        assert_eq!(
            c,
            ToolCall::MessagesByHour {
                author: None,
                from: None,
                to: None
            }
        );
        assert_eq!(
            i,
            Intent::WhoMost {
                most: true,
                hours: Some((0, 5, "madrugada"))
            }
        );

        let (c, i) = interpret("Qual mês teve mais mensagens em 2024?", &authors()).unwrap();
        assert_eq!(
            c,
            ToolCall::MessagesByPeriod {
                granularity: Granularity::Month,
                author: None,
                from: d(2024, 1, 1),
                to: d(2024, 12, 31)
            }
        );
        assert_eq!(i, Intent::TopPeriod { most: true });

        let (c, i) = interpret("qual o dia da semana que a gente menos conversa", &authors()).unwrap();
        assert!(matches!(
            c,
            ToolCall::MessagesByPeriod {
                granularity: Granularity::Weekday,
                ..
            }
        ));
        assert_eq!(i, Intent::TopPeriod { most: false });

        assert_eq!(
            interpret("que horas a Ana mais manda mensagem?", &authors()).unwrap().1,
            Intent::PeakHour
        );
        let (c, i) = interpret("quantas mensagens o Bruno mandou entre 2022 e 2023?", &authors()).unwrap();
        assert_eq!(
            c,
            ToolCall::MessagesByHour {
                author: Some("Bruno".into()),
                from: d(2022, 1, 1),
                to: d(2023, 12, 31)
            }
        );
        assert_eq!(i, Intent::TotalMessages { hours: None });
    }

    #[test]
    fn response_time() {
        let (c, i) = interpret("Quem responde mais rápido?", &authors()).unwrap();
        assert_eq!(
            c,
            ToolCall::ResponseTime {
                author: None,
                from: None,
                to: None
            }
        );
        assert_eq!(i, Intent::ResponseTime { compare: true });
        let (c, i) = interpret("quanto tempo o Bruno demora pra responder?", &authors()).unwrap();
        assert_eq!(
            c,
            ToolCall::ResponseTime {
                author: Some("Bruno".into()),
                from: None,
                to: None
            }
        );
        assert_eq!(i, Intent::ResponseTime { compare: false });
    }

    #[test]
    fn not_understood() {
        for q in [
            "oi",
            "o que ela disse sobre o emprego novo?",
            "quantas vezes a gente brigou?",
        ] {
            assert!(interpret(q, &authors()).is_none(), "{q}");
        }
    }
}
