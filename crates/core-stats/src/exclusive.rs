//! Palavras exclusivas (feature 14): "só você usa" e "você usa muito mais".

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const LIST_LEN: usize = 15;
/// Soma do prior de Dirichlet informativo (distribuído conforme a frequência no corpus todo).
const PRIOR_TOTAL: f64 = 1000.0;
/// z-score mínimo (~95%) para entrar em "você usa muito mais".
const MIN_Z: f64 = 1.96;
/// Razão mínima entre as frequências relativas para entrar em "você usa muito mais".
const MIN_RATIO: f64 = 1.5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExclusiveWord {
    pub word: String,
    pub count: u64,
    pub other_count: u64,
    /// Razão entre as frequências relativas (esta pessoa ÷ a outra). `None` se a outra nunca usou.
    pub ratio: Option<f64>,
    pub z_score: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExclusiveWords {
    /// Palavras que só esta pessoa usa (com frequência mínima).
    pub only_you: Vec<ExclusiveWord>,
    /// Palavras usadas desproporcionalmente mais por esta pessoa (log-odds com prior de Dirichlet).
    pub much_more: Vec<ExclusiveWord>,
}

/// Retorna uma entrada por autor. Só faz sentido com exatamente duas pessoas.
pub fn exclusive_words(words: &[HashMap<String, u64>], min_freq: u64) -> Vec<ExclusiveWords> {
    if words.len() != 2 {
        return vec![ExclusiveWords::default(); words.len()];
    }
    let totals: [f64; 2] = [0, 1].map(|i| words[i].values().sum::<u64>() as f64);
    let corpus = totals[0] + totals[1];
    if corpus == 0.0 {
        return vec![ExclusiveWords::default(); 2];
    }
    (0..2)
        .map(|i| {
            let j = 1 - i;
            let (mine, other) = (&words[i], &words[j]);
            let mut only_you = Vec::new();
            let mut much_more = Vec::new();
            for (w, &yi) in mine {
                if yi < min_freq {
                    continue;
                }
                let yj = other.get(w).copied().unwrap_or(0);
                let alpha = PRIOR_TOTAL * (yi + yj) as f64 / corpus;
                let (yi_f, yj_f) = (yi as f64, yj as f64);
                let delta = ((yi_f + alpha) / (totals[i] + PRIOR_TOTAL - yi_f - alpha)).ln()
                    - ((yj_f + alpha) / (totals[j] + PRIOR_TOTAL - yj_f - alpha)).ln();
                let z = delta / (1.0 / (yi_f + alpha) + 1.0 / (yj_f + alpha)).sqrt();
                let ratio = (yj > 0).then(|| (yi_f / totals[i]) / (yj_f / totals[j]));
                let entry = ExclusiveWord {
                    word: w.clone(),
                    count: yi,
                    other_count: yj,
                    ratio,
                    z_score: z,
                };
                if yj == 0 {
                    only_you.push(entry);
                } else if z >= MIN_Z && ratio.is_some_and(|r| r >= MIN_RATIO) {
                    much_more.push(entry);
                }
            }
            only_you.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
            much_more.sort_by(|a, b| b.z_score.total_cmp(&a.z_score).then_with(|| a.word.cmp(&b.word)));
            only_you.truncate(LIST_LEN);
            much_more.truncate(LIST_LEN);
            ExclusiveWords { only_you, much_more }
        })
        .collect()
}
