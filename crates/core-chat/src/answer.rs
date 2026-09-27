//! Redação das respostas a partir do resultado das ferramentas, sem LLM. Todos os números
//! vêm do resultado; o texto só os organiza.

use crate::intent::{HourRange, Intent};
use crate::tools::{Granularity, PeriodCount, ToolCall, ToolResult};
use chrono::{Datelike, NaiveDate};

const MONTHS: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
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
const WEEKDAYS: [&str; 7] = [
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
    "domingo",
];

pub fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// "42s", "3min 5s", "2h 10min" (mesmo formato do dashboard).
pub fn fmt_duration(secs: f64) -> String {
    let s = secs.round() as u64;
    if s < 60 {
        return format!("{s}s");
    }
    let m = s / 60;
    if m < 60 {
        return if !s.is_multiple_of(60) {
            format!("{m}min {}s", s % 60)
        } else {
            format!("{m}min")
        };
    }
    let h = m / 60;
    if !m.is_multiple_of(60) {
        format!("{h}h {}min", m % 60)
    } else {
        format!("{h}h")
    }
}

fn fmt_date(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{} {}", fmt_int(n), if n == 1 { one } else { many })
}

/// " em 2024", " em março de 2024", " entre 01/02/2024 e 10/03/2024"…
fn range_phrase(from: Option<NaiveDate>, to: Option<NaiveDate>) -> String {
    match (from, to) {
        (None, None) => String::new(),
        (Some(f), Some(t)) if f.ordinal() == 1 && t.month() == 12 && t.day() == 31 => {
            if f.year() == t.year() {
                format!(" em {}", f.year())
            } else {
                format!(" entre {} e {}", f.year(), t.year())
            }
        }
        (Some(f), Some(t))
            if f.day() == 1
                && f.year() == t.year()
                && f.month() == t.month()
                && t.succ_opt().is_none_or(|n| n.day() == 1) =>
        {
            format!(" em {} de {}", MONTHS[f.month0() as usize], f.year())
        }
        (Some(f), Some(t)) => format!(" entre {} e {}", fmt_date(f), fmt_date(t)),
        (Some(f), None) => format!(" desde {}", fmt_date(f)),
        (None, Some(t)) => format!(" até {}", fmt_date(t)),
    }
}

fn period_label(g: Granularity, p: &str) -> String {
    match g {
        Granularity::Weekday => WEEKDAYS
            .get(p.parse::<usize>().unwrap_or(0))
            .copied()
            .unwrap_or(p)
            .to_string(),
        Granularity::Year => p.to_string(),
        Granularity::Month => {
            let (y, m) = p.split_once('-').unwrap_or((p, "1"));
            format!("{} de {y}", MONTHS[m.parse::<usize>().unwrap_or(1).clamp(1, 12) - 1])
        }
        Granularity::Week => NaiveDate::parse_from_str(p, "%Y-%m-%d")
            .map(|d| format!("a semana de {}", fmt_date(d)))
            .unwrap_or_else(|_| p.to_string()),
        Granularity::Day => NaiveDate::parse_from_str(p, "%Y-%m-%d")
            .map(|d| {
                format!(
                    "{} ({})",
                    fmt_date(d),
                    WEEKDAYS[d.weekday().num_days_from_monday() as usize]
                )
            })
            .unwrap_or_else(|_| p.to_string()),
    }
}

fn period_noun(g: Granularity) -> &'static str {
    match g {
        Granularity::Day => "O dia",
        Granularity::Week => "A semana",
        Granularity::Month => "O mês",
        Granularity::Year => "O ano",
        Granularity::Weekday => "O dia da semana",
    }
}

fn period_plural(g: Granularity) -> &'static str {
    match g {
        Granularity::Day => "dias",
        Granularity::Week => "semanas",
        Granularity::Month => "meses",
        Granularity::Year => "anos",
        Granularity::Weekday => "dias da semana",
    }
}

fn hours_phrase(h: Option<HourRange>) -> String {
    match h {
        Some((a, b, "manhã")) => format!("De manhã ({a}h–{b}h), "),
        Some((a, b, name)) => format!("De {name} ({a}h–{b}h), "),
        None => String::new(),
    }
}

fn sum_hours(by_author: &[[u64; 24]], hours: Option<HourRange>) -> Vec<u64> {
    let (a, b) = hours.map_or((0, 23), |(a, b, _)| (a as usize, b as usize));
    by_author.iter().map(|h| h[a..=b].iter().sum()).collect()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Texto da resposta e ids das mensagens citadas.
pub fn compose(call: &ToolCall, result: &ToolResult, intent: &Intent, authors: &[String]) -> (String, Vec<i64>) {
    let (from, to) = call.range();
    let range = range_phrase(from, to);
    let who = call.author();
    match (result, intent) {
        (
            ToolResult::CountWord {
                word,
                total,
                by_author,
                messages,
                examples,
            },
            _,
        ) => {
            let text = if *total == 0 {
                format!(
                    "Não encontrei “{word}”{}{range}.",
                    who.map(|a| format!(" nas mensagens de {a}")).unwrap_or_default()
                )
            } else if let Some(a) = who {
                format!(
                    "{a} escreveu “{word}” {}{range}, em {}.",
                    plural(*total, "vez", "vezes"),
                    plural(*messages, "mensagem", "mensagens")
                )
            } else {
                let parts: Vec<String> = authors
                    .iter()
                    .zip(by_author)
                    .map(|(a, n)| format!("{a}: {}", fmt_int(*n)))
                    .collect();
                let lead = if authors.len() == 2 && by_author[0] != by_author[1] {
                    let top = if by_author[0] > by_author[1] { 0 } else { 1 };
                    format!(" {} usa mais.", authors[top])
                } else {
                    String::new()
                };
                format!(
                    "“{word}” aparece {}{range} ({}).{lead}",
                    plural(*total, "vez", "vezes"),
                    parts.join(" · ")
                )
            };
            (text, examples.iter().take(3).copied().collect())
        }
        (ToolResult::FirstOccurrence { word, message }, _) => match message {
            None => (
                format!(
                    "Não encontrei “{word}”{}.",
                    who.map(|a| format!(" nas mensagens de {a}")).unwrap_or_default()
                ),
                vec![],
            ),
            Some(m) => {
                let dt = chrono::DateTime::from_timestamp(m.ts, 0)
                    .unwrap_or_default()
                    .naive_utc();
                let snippet: String = m.text.as_deref().unwrap_or_default().chars().take(160).collect();
                (
                    format!(
                        "A primeira vez foi em {} às {}, por {}: “{snippet}”",
                        fmt_date(dt.date()),
                        dt.format("%H:%M"),
                        m.author.as_deref().unwrap_or("?")
                    ),
                    vec![m.id],
                )
            }
        },
        (ToolResult::MessagesByHour { by_author }, Intent::WhoMost { most, hours }) => {
            let totals = sum_hours(by_author, *hours);
            let prefix = hours_phrase(*hours);
            let text = if authors.len() != 2 {
                format!(
                    "{prefix}{} enviou {}{range}.",
                    authors[0],
                    plural(totals[0], "mensagem", "mensagens")
                )
            } else if totals[0] == totals[1] {
                format!("{prefix}empate{range}: {} mensagens para cada um.", fmt_int(totals[0]))
            } else {
                let top = if (totals[0] > totals[1]) == *most { 0 } else { 1 };
                format!(
                    "{prefix}{} manda {} mensagens{range}: {} contra {} de {}.",
                    authors[top],
                    if *most { "mais" } else { "menos" },
                    fmt_int(totals[top]),
                    fmt_int(totals[1 - top]),
                    authors[1 - top]
                )
            };
            (capitalize(&text), vec![])
        }
        (ToolResult::MessagesByHour { by_author }, Intent::TotalMessages { hours }) => {
            let totals = sum_hours(by_author, *hours);
            let sum: u64 = totals.iter().sum();
            let prefix = hours_phrase(*hours);
            let text = match who {
                Some(a) => format!("{prefix}{a} enviou {}{range}.", plural(sum, "mensagem", "mensagens")),
                None => {
                    let parts: Vec<String> = authors
                        .iter()
                        .zip(&totals)
                        .map(|(a, n)| format!("{a}: {}", fmt_int(*n)))
                        .collect();
                    format!(
                        "{prefix}foram {}{range} ({}).",
                        plural(sum, "mensagem", "mensagens"),
                        parts.join(" · ")
                    )
                }
            };
            (capitalize(&text), vec![])
        }
        (ToolResult::MessagesByHour { by_author }, _) => {
            let all: Vec<u64> = (0..24).map(|h| by_author.iter().map(|a| a[h]).sum()).collect();
            let peak = (0..24).max_by_key(|&h| (all[h], std::cmp::Reverse(h))).unwrap_or(0);
            let text = if all[peak] == 0 {
                format!(
                    "Não há mensagens{}{range}.",
                    who.map(|a| format!(" de {a}")).unwrap_or_default()
                )
            } else {
                format!(
                    "O horário com mais mensagens{}{range} é das {peak}h às {}h, com {}.",
                    who.map(|a| format!(" de {a}")).unwrap_or_default(),
                    (peak + 1) % 24,
                    plural(all[peak], "mensagem", "mensagens")
                )
            };
            (text, vec![])
        }
        (
            ToolResult::MessagesByPeriod {
                granularity,
                periods,
                top,
                bottom,
            },
            intent,
        ) => {
            let most = !matches!(intent, Intent::TopPeriod { most: false });
            let pick: Option<&PeriodCount> = if most { top.first() } else { bottom.first() };
            let of = who.map(|a| format!(" de {a}")).unwrap_or_default();
            let empty = periods.iter().filter(|p| p.total == 0).count();
            let text = match pick {
                None => format!("Não há mensagens{of}{range}."),
                Some(p) if !most && empty > 1 => format!(
                    "{} {}{range} não tiveram nenhuma mensagem{of}; o primeiro foi {}.",
                    fmt_int(empty as u64),
                    period_plural(*granularity),
                    period_label(*granularity, &p.period)
                ),
                Some(p) => format!(
                    "{} com {} mensagens{of}{range} foi {}, com {}.",
                    period_noun(*granularity),
                    if most { "mais" } else { "menos" },
                    period_label(*granularity, &p.period),
                    if p.total == 0 {
                        "nenhuma mensagem".to_string()
                    } else {
                        plural(p.total, "mensagem", "mensagens")
                    }
                ),
            };
            (text, vec![])
        }
        (ToolResult::ResponseTime { by_author, .. }, Intent::ResponseTime { compare: false }) if who.is_some() => {
            let a = who.unwrap_or_default();
            let s = authors
                .iter()
                .position(|x| x == a)
                .and_then(|i| by_author.get(i))
                .cloned()
                .unwrap_or_default();
            let text = if s.count == 0 {
                format!("Não há respostas de {a}{range} para calcular.")
            } else {
                format!(
                    "{a} leva em média {} para responder{range} (mediana de {}, em {}).",
                    fmt_duration(s.mean_secs),
                    fmt_duration(s.median_secs),
                    plural(s.count, "resposta", "respostas")
                )
            };
            (text, vec![])
        }
        (ToolResult::ResponseTime { by_author, fastest }, _) => {
            let text = match fastest {
                Some(f) if authors.len() == 2 => {
                    let (fs, os) = (&by_author[*f], &by_author[1 - f]);
                    format!(
                        "{} responde mais rápido{range}: mediana de {}, contra {} de {}. (Médias: {} e {}.)",
                        authors[*f],
                        fmt_duration(fs.median_secs),
                        fmt_duration(os.median_secs),
                        authors[1 - f],
                        fmt_duration(fs.mean_secs),
                        fmt_duration(os.mean_secs)
                    )
                }
                _ => format!("Não há respostas suficientes das duas pessoas{range} para comparar."),
            };
            (text, vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(fmt_int(0), "0");
        assert_eq!(fmt_int(1234567), "1.234.567");
        assert_eq!(fmt_duration(76.0), "1min 16s");
        assert_eq!(fmt_duration(3600.0 * 18.0 + 37.0 * 60.0), "18h 37min");
        let d = |y, m, dd| NaiveDate::from_ymd_opt(y, m, dd);
        assert_eq!(range_phrase(d(2024, 1, 1), d(2024, 12, 31)), " em 2024");
        assert_eq!(range_phrase(d(2024, 2, 1), d(2024, 2, 29)), " em fevereiro de 2024");
        assert_eq!(range_phrase(d(2022, 1, 1), d(2023, 12, 31)), " entre 2022 e 2023");
        assert_eq!(
            range_phrase(d(2024, 2, 3), d(2024, 2, 10)),
            " entre 03/02/2024 e 10/02/2024"
        );
        assert_eq!(period_label(Granularity::Month, "2024-03"), "março de 2024");
        assert_eq!(period_label(Granularity::Weekday, "6"), "domingo");
    }
}
