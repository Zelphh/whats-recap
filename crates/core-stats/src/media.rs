//! Figurinhas (17) e áudios (18), a partir da tabela `media` preenchida pelo `core-media`.

use crate::Msg;
use core_parser::MessageKind;
use core_storage::MediaRow;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const TOP_N: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StickerCount {
    /// Arquivo representativo (a variante mais usada), para buscar a miniatura.
    pub file: String,
    pub count: u64,
    /// Quantos conteúdos distintos (SHA-256) foram agrupados como a mesma figurinha.
    pub variants: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioStats {
    pub count: u64,
    /// Áudios cuja duração pôde ser lida (arquivo presente e Ogg Opus válido).
    pub with_duration: u64,
    pub total_ms: i64,
    pub avg_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaStats {
    pub top_stickers: Vec<StickerCount>,
    pub top_stickers_by_author: Vec<Vec<StickerCount>>,
    /// Figurinhas distintas depois do agrupamento perceptual.
    pub distinct_stickers: u64,
    /// Figurinhas enviadas cujo arquivo não estava no zip (ficam fora do ranking).
    pub missing_stickers: u64,
    pub audio: Vec<AudioStats>,
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Agrupa hashes cuja distância de Hamming é ≤ `max_dist` (união transitiva).
/// Retorna o índice do grupo de cada hash de entrada.
pub fn group_by_hamming(hashes: &[u64], max_dist: u32) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..hashes.len()).collect();
    for i in 0..hashes.len() {
        for j in i + 1..hashes.len() {
            if (hashes[i] ^ hashes[j]).count_ones() <= max_dist {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[b] = a;
                }
            }
        }
    }
    (0..hashes.len()).map(|i| find(&mut parent, i)).collect()
}

pub fn media_stats(msgs: &[Msg], n_authors: usize, media: &HashMap<String, MediaRow>, max_dist: u32) -> MediaStats {
    // Conteúdo distinto → grupo. Figurinhas sem dHash (falha ao decodificar) ficam num grupo próprio.
    let mut shas: Vec<&str> = media
        .values()
        .filter_map(|m| m.phash.as_ref().and(m.sha256.as_deref()))
        .collect();
    shas.sort_unstable();
    shas.dedup();
    let phash_of: HashMap<&str, u64> = media
        .values()
        .filter_map(|m| Some((m.sha256.as_deref()?, u64::from_str_radix(m.phash.as_deref()?, 16).ok()?)))
        .collect();
    let hashes: Vec<u64> = shas.iter().map(|s| phash_of[s]).collect();
    let groups = group_by_hamming(&hashes, max_dist);
    let group_of: HashMap<&str, String> = shas.iter().zip(&groups).map(|(s, g)| (*s, format!("g{g}"))).collect();

    let mut stats = MediaStats {
        audio: vec![AudioStats::default(); n_authors],
        ..Default::default()
    };
    // grupo → (total, por autor, uso de cada sha → (contagem, arquivo))
    type Group<'a> = (u64, Vec<u64>, HashMap<&'a str, (u64, &'a str)>);
    let mut by_group: HashMap<String, Group> = HashMap::new();

    for m in msgs {
        let Some(a) = m.author else { continue };
        let row = m.media_file.as_deref().and_then(|f| media.get(f));
        match m.kind {
            MessageKind::Sticker => {
                let Some(row) = row else {
                    stats.missing_stickers += 1;
                    continue;
                };
                let sha = row.sha256.as_deref().unwrap_or(&row.file);
                let key = group_of.get(sha).cloned().unwrap_or_else(|| format!("s{sha}"));
                let g = by_group
                    .entry(key)
                    .or_insert_with(|| (0, vec![0; n_authors], HashMap::new()));
                g.0 += 1;
                g.1[a] += 1;
                g.2.entry(sha).or_insert((0, &row.file)).0 += 1;
            }
            MessageKind::Audio => {
                let s = &mut stats.audio[a];
                s.count += 1;
                if let Some(d) = row.and_then(|r| r.duration_ms) {
                    s.with_duration += 1;
                    s.total_ms += d;
                }
            }
            _ => {}
        }
    }
    for s in &mut stats.audio {
        s.avg_ms = if s.with_duration > 0 {
            s.total_ms as f64 / s.with_duration as f64
        } else {
            0.0
        };
    }

    stats.distinct_stickers = by_group.len() as u64;
    let entry = |count: u64, variants: &HashMap<&str, (u64, &str)>| {
        let (_, file) = variants
            .values()
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(a.1)))
            .unwrap();
        StickerCount {
            file: file.to_string(),
            count,
            variants: variants.len() as u32,
        }
    };
    let rank = |mut v: Vec<StickerCount>| {
        v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.file.cmp(&b.file)));
        v.truncate(TOP_N);
        v
    };
    stats.top_stickers = rank(by_group.values().map(|(c, _, v)| entry(*c, v)).collect());
    stats.top_stickers_by_author = (0..n_authors)
        .map(|a| {
            rank(
                by_group
                    .values()
                    .filter(|g| g.1[a] > 0)
                    .map(|(_, per, v)| entry(per[a], v))
                    .collect(),
            )
        })
        .collect();
    stats
}
