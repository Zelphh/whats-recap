//! Interpretação heurística de perguntas quantitativas comuns, sem LLM: escolhe a ferramenta e
//! os parâmetros para os formatos óbvios ("quantas vezes o Bruno falou 'amor'?"). O que não se
//! encaixa retorna `None` e, com o LLM disponível, é interpretado por ele.

use crate::router::fold;
use crate::tools::{Granularity, ListFilter, ToolCall};
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
    ListMessages,
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

// Aspas fora de palavras: o apóstrofo de "l'amour" ou "d'água" não abre nem fecha citação.
static QUOTED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[^\p{L}\p{N}])["“”'‘’«»]([^"“”'‘’«»]{1,60})["“”'‘’«»](?:$|[^\p{L}\p{N}])"#).unwrap()
});

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
static DAY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").unwrap());
static DAY_MONTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,2}) de (janeiro|fevereiro|marco|abril|maio|junho|julho|agosto|setembro|outubro|novembro|dezembro)(?: de)? (\d{4})\b")
        .unwrap()
});
// "das 7h às 8h", "entre 22h e 2h", "de 7 a 9 da noite", "das 7:30 às 8:15".
static HOUR_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:das|dos|entre(?: as)?|de) (\d{1,2})(?:h|:00| horas?)?(?:(?:h|:)\d{2})?(?: (?:da|de) (manha|tarde|noite|madrugada))? (?:as|a|e|ate)(?: as)? (\d{1,2})(?:(?:h|:)(\d{2})|h| horas?)?(?: (?:da|de) (manha|tarde|noite|madrugada))?\b",
    )
    .unwrap()
});
static MONTH_AFTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ de (janeiro|fevereiro|marco|abril|maio|junho|julho|agosto|setembro|outubro|novembro|dezembro)\b")
        .unwrap()
});
// "às 7h", "a partir das 22h", "antes das 7 da manhã": a hora precisa de "h", "horas" ou parte do dia.
static ONE_HOUR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(a partir das|depois das|antes das|as) (\d{1,2})(?:h\b|:\d{2}| horas?\b|(?: (?:da|de) (manha|tarde|noite|madrugada))\b)(?: (?:da|de) (manha|tarde|noite|madrugada)\b)?",
    )
    .unwrap()
});
// "quais foram as mensagens", "que mensagens", "mostra as mensagens", "liste todas as msgs".
static LIST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:quais|que|mostr\w*|list\w*|exib\w*|ver|veja)\b (?:foram |sao |eram )?(?:todas )?(?:as )?(?:mensagens|msgs?)\b")
        .unwrap()
});
static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9]+").unwrap());

static RESPONSE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(respond\w*|resposta\w*|demora\w*|mais rapido|mais devagar|mais lento)\b").unwrap()
});
static PEAK_HOUR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(que horas?|qual (o |a )?(horario|hora)|horario|hora do dia)\b").unwrap());
static PERIOD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bqual (?:o |a |foi o |foi a )?(dia da semana|mes|ano|semana|dia)\b").unwrap());
// Períodos relativos a hoje ou do calendário, que `date_range` não converte: ficam para o LLM, que
// recebe as datas calculadas. Responder sobre a conversa inteira seria errado.
static RELATIVE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(passad[oa]|ultim[oa]s? (\d+ )?(ano|anos|mes|meses|semana|semanas|dias)|(este|esse|nesse|neste) (ano|mes)|(esta|essa|nessa|nesta) semana|ontem|anteontem|verao|inverno|outono|primavera|ferias|natal|carnaval|ano novo|semestre|trimestre)\b",
    )
    .unwrap()
});
static WEEKEND_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(fim de semana|fins de semana|dias? (util|uteis))\b").unwrap());
static WHO_MOST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bquem\b.*\b(manda|envia|escreve|fala|conversa|digita|puxa|mensag)\w*").unwrap());

fn date(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

fn last_day(y: i32, m: u32) -> Option<NaiveDate> {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    date(ny, nm, 1)?.pred_opt()
}

/// Intervalo de datas mencionado: "em 15/03/2024", "15 de março de 2024", "entre 2022 e 2024",
/// "em março de 2024", "em 2023".
pub fn date_range(q: &str) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let day = DAY_RE
        .captures(q)
        .and_then(|c| date(c[3].parse().ok()?, c[2].parse().ok()?, c[1].parse().ok()?))
        .or_else(|| {
            let c = DAY_MONTH_RE.captures(q)?;
            let m = MONTHS.iter().position(|x| *x == &c[2])? as u32 + 1;
            date(c[3].parse().ok()?, m, c[1].parse().ok()?)
        });
    if let Some(d) = day {
        return (Some(d), Some(d));
    }
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

/// Hora com a parte do dia: "9 da noite" → 21, "2 da tarde" → 14 (meio-dia e antes ficam iguais).
fn with_part(h: u8, part: Option<&str>) -> u8 {
    match part {
        Some("tarde" | "noite") if (1..12).contains(&h) => h + 12,
        _ => h,
    }
}

/// Faixa de horas da pergunta, no formato de [`ListFilter`](crate::ListFilter): início inclusivo,
/// fim exclusivo ("das 7h às 8h" → 7 e 8; "das 7:00 às 8:15" → 7 e 9).
pub fn hour_range(q: &str) -> Option<(Option<u8>, Option<u8>)> {
    if let Some(c) = HOUR_RANGE_RE.captures(q) {
        // "de 15 a 20 de março" são dias, não horas.
        if MONTH_AFTER_RE.is_match(&q[c.get(0)?.end()..]) {
            return None;
        }
        let (a, b): (u8, u8) = (c[1].parse().ok()?, c[3].parse().ok()?);
        if a > 24 || b > 24 {
            return None;
        }
        let end_part = c.get(5).map(|m| m.as_str());
        // "das 7 às 9 da noite": a parte do dia do fim vale para o começo se o começo não tiver a sua.
        let start_part = c.get(2).map(|m| m.as_str()).or(end_part.filter(|_| a <= b));
        let minutes = c.get(4).is_some_and(|m| m.as_str() != "00");
        let end = with_part(b, end_part) + u8::from(minutes);
        return Some((Some(with_part(a, start_part) % 24), Some(end.min(24))));
    }
    let c = ONE_HOUR_RE.captures(q)?;
    let h: u8 = c[2].parse().ok()?;
    if h > 23 {
        return None;
    }
    let h = with_part(h, c.get(3).or(c.get(4)).map(|m| m.as_str()));
    Some(match &c[1] {
        "antes das" => (None, Some(h)),
        "as" => (Some(h), Some(h)),
        _ => (Some(h), None),
    })
}

/// A palavra ou expressão procurada: entre aspas, ou logo depois do verbo (até 3 palavras).
pub fn target_word(original: &str, q: &str) -> Option<String> {
    if QUOTED_RE.is_match(original) {
        return quoted_word(original);
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

/// Primeiro nome do autor: a primeira parte com letras ou números ("💚 Ana" → "Ana").
pub fn first_name(author: &str) -> &str {
    author
        .split_whitespace()
        .find(|w| w.chars().any(char::is_alphanumeric))
        .unwrap_or_default()
}

/// Nome que o WhatsApp dá ao dono do celular em alguns exports.
const SELF_NAMES: &[&str] = &["voce", "you"];

/// Primeira pessoa na pergunta ("eu disse", "falei"): aponta para quem pergunta.
const FIRST_PERSON: &[&str] = &["eu", "falei", "escrevi", "mandei", "usei", "digitei", "chamei"];

/// Terceira pessoa: sabendo quem pergunta, só pode ser o outro.
const THIRD_PERSON: &[&str] = &["ela", "ele", "dela", "dele"];

/// Quem pergunta, quando o export já diz: o autor chamado "Você" (ou "You").
pub fn default_me(authors: &[String]) -> Option<&str> {
    authors
        .iter()
        .find(|a| SELF_NAMES.contains(&fold(a).trim()))
        .map(String::as_str)
}

/// Autor mencionado pelo primeiro nome ou, sabendo quem pergunta (`me`), por "eu" (quem pergunta)
/// e "ela"/"ele" (o outro), se só um for citado. A palavra procurada não conta: "eu" em "eu disse
/// 'eu te amo'" só vale uma vez.
fn mentioned_author(q: &str, authors: &[String], me: Option<&str>, exclude: Option<&str>) -> Option<String> {
    let q = match exclude {
        Some(w) => q.replacen(&fold(w), " ", 1),
        None => q.to_string(),
    };
    let words: Vec<&str> = WORD_RE.find_iter(&q).map(|m| m.as_str()).collect();
    let said = |list: &[&str]| words.iter().any(|w| list.contains(w));
    let hits: Vec<&String> = authors
        .iter()
        .filter(|a| {
            // O nome pode ter mais de uma palavra para o `WORD_RE` ("l'amour" → "l", "amour").
            let name = fold(first_name(a));
            let parts: Vec<&str> = WORD_RE.find_iter(&name).map(|m| m.as_str()).collect();
            // Escrito sem o apóstrofo ("lamour") também vale.
            let named = !parts.is_empty()
                && (words.windows(parts.len()).any(|w| w == parts.as_slice())
                    || words.contains(&parts.concat().as_str()));
            let is_me = me.is_some_and(|m| m == a.as_str());
            named || (is_me && said(FIRST_PERSON)) || (me.is_some() && !is_me && said(THIRD_PERSON))
        })
        .collect();
    (hits.len() == 1).then(|| hits[0].clone())
}

/// "o que ela mandou das 7h às 8h?", "o que a gente falou de madrugada?": com horário, "o que" pede
/// as mensagens daquele horário, não uma resposta sobre o conteúdo.
pub fn is_timed_what(q: &str) -> bool {
    q.contains("o que") && (hour_range(q).is_some() || part_of_day(q).is_some())
}

/// Palavra entre aspas, se houver (na lista de mensagens, só ela filtra por palavra).
fn quoted_word(original: &str) -> Option<String> {
    let c = QUOTED_RE.captures(original)?;
    let w = c[1].trim();
    (!w.is_empty()).then(|| w.to_string())
}

/// Interpreta a pergunta, sabendo quem pergunta só pelo autor "Você" (ver [`interpret_as`]).
pub fn interpret(question: &str, authors: &[String]) -> Option<(ToolCall, Intent)> {
    interpret_as(question, authors, default_me(authors))
}

/// Interpreta a pergunta. `authors` são os nomes da conversa (para reconhecer "o Bruno"); `me` é
/// quem pergunta, se conhecido (para "eu", "ela" e "ele").
pub fn interpret_as(question: &str, authors: &[String], me: Option<&str>) -> Option<(ToolCall, Intent)> {
    let q = fold(question);
    let (from, to) = date_range(&q);
    if from.is_none() && RELATIVE_RE.is_match(&q) {
        return None;
    }
    let has = |s: &str| WORD_RE.find_iter(&q).any(|m| m.as_str() == s);
    let most = !has("menos");
    // "quais foram as mensagens dela das 7h às 8h?" e "o que ela mandou de madrugada?"; com faixa de
    // horas, "quantas mensagens…" também (`messages_by_hour` só soma partes do dia inteiras).
    let hours = hour_range(&q);
    if LIST_RE.is_match(&q) || (hours.is_some() && q.contains("quantas mensagens")) || is_timed_what(&q) {
        let word = quoted_word(question);
        let (hour_from, hour_to) = hours
            .or_else(|| part_of_day(&q).map(|(a, b, _)| (Some(a), Some(b + 1))))
            .unwrap_or_default();
        let author = if has("quem") {
            None
        } else {
            mentioned_author(&q, authors, me, word.as_deref())
        };
        return Some((
            ToolCall::ListMessages(ListFilter {
                author,
                from,
                to,
                hour_from,
                hour_to,
                word,
            }),
            Intent::ListMessages,
        ));
    }
    let word = target_word(question, &q);
    let author = mentioned_author(&q, authors, me, word.as_deref());

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
    // "de semana ou no fim de semana?" é sobre dias da semana, não semanas do calendário.
    let weekend = WEEKEND_RE.is_match(&q).then_some("dia da semana");
    if let Some(period) = weekend.or_else(|| PERIOD_RE.captures(&q).map(|c| c.get(1).unwrap().as_str()))
        && (has("mais") || has("menos"))
    {
        let granularity = match period {
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

/// O que a resposta deve destacar quando a ferramenta foi escolhida pelo LLM: os mesmos sinais
/// que a heurística usa ("menos", parte do dia, "que horas"), lidos da pergunta original.
pub fn intent_for(call: &ToolCall, question: &str) -> Intent {
    let q = fold(question);
    let most = !WORD_RE.find_iter(&q).any(|m| m.as_str() == "menos");
    match call {
        ToolCall::CountWord { .. } => Intent::CountWord,
        ToolCall::FirstOccurrence { .. } => Intent::FirstOccurrence,
        ToolCall::MessagesByPeriod { .. } => Intent::TopPeriod { most },
        ToolCall::ResponseTime { author, .. } => Intent::ResponseTime {
            compare: author.is_none(),
        },
        ToolCall::ListMessages(_) => Intent::ListMessages,
        ToolCall::MessagesByHour { author, .. } => {
            let hours = part_of_day(&q);
            if PEAK_HOUR_RE.is_match(&q) && hours.is_none() {
                Intent::PeakHour
            } else if author.is_none() && WHO_MOST_RE.is_match(&q) {
                Intent::WhoMost { most, hours }
            } else {
                Intent::TotalMessages { hours }
            }
        }
    }
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
    fn names_with_apostrophe_and_emoji() {
        let authors = vec!["Você".to_string(), "l'amour vrai 💚 🌼 💜".to_string()];
        let (c, _) = interpret("Quantas vezes l'amour falou “amor”?", &authors).unwrap();
        assert_eq!(
            c,
            ToolCall::CountWord {
                word: "amor".into(),
                author: Some(authors[1].clone()),
                from: None,
                to: None
            }
        );
        assert_eq!(first_name("💚 Ana Souza"), "Ana");
        let (c, _) = interpret("a gente conversa mais de semana ou no fim de semana?", &authors).unwrap();
        assert!(matches!(
            c,
            ToolCall::MessagesByPeriod {
                granularity: Granularity::Weekday,
                ..
            }
        ));
        let (c, _) = interpret("Quantas vezes eu disse 'eu te amo'", &authors).unwrap();
        assert_eq!(c.author(), Some("Você"));
        assert!(matches!(&c, ToolCall::CountWord { word, .. } if word == "eu te amo"));
        // Com "Você" na conversa, "ela" só pode ser a outra pessoa.
        let (c, _) = interpret("Quantas vezes ela falou 'eu te amo'", &authors).unwrap();
        assert_eq!(c.author(), Some(authors[1].as_str()));
        let (c, _) = interpret("quem fala mais 'oi', eu ou ela?", &authors).unwrap();
        assert_eq!(c.author(), None);
        // Sem "Você" no export, quem pergunta vem das configurações.
        let names = vec!["Matheus M.T.".to_string(), "l'amour vrai 💚".to_string()];
        let who = |q: &str, me| interpret_as(q, &names, me).unwrap().0.author().map(str::to_string);
        assert_eq!(who("quantas vezes ela falou 'amor'?", None), None);
        assert_eq!(
            who("quantas vezes ela falou 'amor'?", Some("Matheus M.T.")),
            Some(names[1].clone())
        );
        assert_eq!(
            who("quantas vezes ele falou 'amor'?", Some("l'amour vrai 💚")),
            Some(names[0].clone())
        );
        assert_eq!(
            who("quantas vezes eu falei 'amor'?", Some("Matheus M.T.")),
            Some(names[0].clone())
        );
        assert_eq!(
            who("quantas vezes ela me chamou de 'chato'?", Some("Matheus M.T.")),
            Some(names[1].clone())
        );
        assert_eq!(target_word("e o copo d'água?", "e o copo d'agua?"), None);
    }

    #[test]
    fn hour_ranges() {
        let r = |q: &str| hour_range(&fold(q));
        assert_eq!(r("das 7h às 8h"), Some((Some(7), Some(8))));
        assert_eq!(r("das 7 às 8 da manhã"), Some((Some(7), Some(8))));
        assert_eq!(r("entre 22h e 2h"), Some((Some(22), Some(2))));
        assert_eq!(r("entre as 7 da manhã e as 8 da manha"), Some((Some(7), Some(8))));
        assert_eq!(r("das 10 até as 11"), Some((Some(10), Some(11))));
        assert_eq!(r("de 7 a 9 da noite"), Some((Some(19), Some(21))));
        assert_eq!(r("das 11 às 2 da tarde"), Some((Some(11), Some(14))));
        assert_eq!(r("das 7:30 às 8:15"), Some((Some(7), Some(9))));
        assert_eq!(r("das 7h30 às 8h"), Some((Some(7), Some(8))));
        assert_eq!(r("às 7h"), Some((Some(7), Some(7))));
        assert_eq!(r("a partir das 22h"), Some((Some(22), None)));
        assert_eq!(r("antes das 7 da manhã"), Some((None, Some(7))));
        assert_eq!(r("de 15 a 20 de março"), None);
        assert_eq!(r("entre 2022 e 2023"), None);
        assert_eq!(r("as 3 mensagens"), None);
    }

    #[test]
    fn list_messages() {
        let list = |q: &str, authors: &[String]| match interpret(q, authors) {
            Some((ToolCall::ListMessages(f), Intent::ListMessages)) => f,
            other => panic!("{q}: {other:?}"),
        };
        assert_eq!(
            list("quais foram as mensagens do Bruno das 7h às 8h?", &authors()),
            ListFilter {
                author: Some("Bruno".into()),
                hour_from: Some(7),
                hour_to: Some(8),
                ..Default::default()
            }
        );
        let voce = vec!["Você".to_string(), "l'amour vrai 💚".to_string()];
        let f = list(
            "Quais mensagens lamour vrai mandou entre as 7 da manhã e as 8 da manha?",
            &voce,
        );
        assert_eq!(
            (f.author.as_deref(), f.hour_from, f.hour_to),
            (Some(voce[1].as_str()), Some(7), Some(8))
        );
        let f = list("Quais foram as mensagens dela das 7 às 8 da manhã?", &voce);
        assert_eq!(
            (f.author.as_deref(), f.hour_from, f.hour_to),
            (Some(voce[1].as_str()), Some(7), Some(8))
        );
        assert_eq!(
            list("me mostra as mensagens com 'amor' de madrugada em 2024", &authors()),
            ListFilter {
                from: d(2024, 1, 1),
                to: d(2024, 12, 31),
                hour_from: Some(0),
                hour_to: Some(6),
                word: Some("amor".into()),
                ..Default::default()
            }
        );
        let f = list("me mostra o que ela escreveu de madrugada", &voce);
        assert_eq!(
            (f.author.as_deref(), f.hour_from, f.hour_to),
            (Some(voce[1].as_str()), Some(0), Some(6))
        );
        let f = list("o que o Bruno mandou às 3h da manhã?", &authors());
        assert_eq!(
            (f.author.as_deref(), f.hour_from, f.hour_to),
            (Some("Bruno"), Some(3), Some(3))
        );
        let f = list("quantas mensagens a Ana mandou entre 22h e 2h?", &authors());
        assert_eq!(
            (f.author.as_deref(), f.hour_from, f.hour_to),
            (Some("Ana Souza"), Some(22), Some(2))
        );
        let f = list("quais foram as mensagens do dia 15/03/2024?", &authors());
        assert_eq!((f.from, f.to), (d(2024, 3, 15), d(2024, 3, 15)));
        let f = list("que mensagens ele mandou em 2 de fevereiro de 2025?", &voce);
        assert_eq!((f.from, f.author.as_deref()), (d(2025, 2, 2), Some(voce[1].as_str())));
        // Sem horário, "quantas mensagens" continua com `messages_by_hour`.
        assert!(matches!(
            call("quantas mensagens o Bruno mandou de manhã?"),
            ToolCall::MessagesByHour { .. }
        ));
    }

    #[test]
    fn relative_periods_go_to_the_llm() {
        for q in [
            "quem mandou mais mensagem no ano passado?",
            "quantas vezes ela falou amor esse mês?",
            "quem respondeu mais rápido nos últimos 3 meses?",
            "qual dia a gente mais conversou nas férias?",
        ] {
            assert!(interpret(q, &authors()).is_none(), "{q}");
        }
        // Com data explícita, as regras continuam respondendo.
        assert!(interpret("quantas vezes ela falou amor no verão de 2024?", &authors()).is_some());
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
