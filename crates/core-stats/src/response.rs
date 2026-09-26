//! Tempo de resposta (features 15 e 16).

use crate::Msg;
use core_parser::MessageKind;
use serde::{Deserialize, Serialize};

/// Limites superiores das faixas de distribuição: < 1 min, 1–5 min, 5–30 min, 30 min até o gap de sessão.
pub const BUCKET_LIMITS_SECS: [i64; 3] = [60, 5 * 60, 30 * 60];

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResponseStats {
    pub count: u64,
    pub mean_secs: f64,
    pub median_secs: f64,
    pub buckets: [u64; 4],
}

/// Uma resposta acontece quando o autor muda entre duas mensagens consecutivas; o tempo vai
/// da última mensagem da outra pessoa até a primeira de quem responde. Respostas que cruzam
/// uma sessão (gap maior que o limite) são descartadas. Sistema e apagadas são ignoradas.
pub fn response_times(msgs: &[Msg], n_authors: usize, gap_secs: i64) -> Vec<ResponseStats> {
    let mut samples: Vec<Vec<i64>> = vec![Vec::new(); n_authors];
    let mut prev: Option<(usize, i64, i64)> = None;
    for m in msgs {
        let Some(a) = m.author else { continue };
        if matches!(m.kind, MessageKind::System | MessageKind::Deleted) {
            continue;
        }
        if let Some((pa, pts, psession)) = prev {
            let dt = (m.ts - pts).max(0);
            if pa != a && psession == m.session && dt <= gap_secs {
                samples[a].push(dt);
            }
        }
        prev = Some((a, m.ts, m.session));
    }
    samples.into_iter().map(summarize).collect()
}

fn summarize(mut s: Vec<i64>) -> ResponseStats {
    if s.is_empty() {
        return ResponseStats::default();
    }
    s.sort_unstable();
    let len = s.len();
    let median = if len % 2 == 1 {
        s[len / 2] as f64
    } else {
        (s[len / 2 - 1] + s[len / 2]) as f64 / 2.0
    };
    let mut buckets = [0u64; 4];
    for &v in &s {
        let i = BUCKET_LIMITS_SECS.iter().position(|&lim| v < lim).unwrap_or(3);
        buckets[i] += 1;
    }
    ResponseStats {
        count: len as u64,
        mean_secs: s.iter().sum::<i64>() as f64 / len as f64,
        median_secs: median,
        buckets,
    }
}

/// Quem responde mais rápido, pela mediana (exige respostas das duas pessoas).
pub fn fastest(stats: &[ResponseStats]) -> Option<usize> {
    if stats.len() < 2 || stats.iter().any(|s| s.count == 0) {
        return None;
    }
    (0..stats.len()).min_by(|&a, &b| stats[a].median_secs.total_cmp(&stats[b].median_secs))
}
