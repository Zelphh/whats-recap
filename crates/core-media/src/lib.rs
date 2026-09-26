//! Mídia do modo "com mídia" (features 17 e 18): hash de figurinhas e duração de áudios.
//!
//! Tudo é lido de dentro do `.zip`, sem extrair arquivos para o disco.

mod ogg;
pub mod testdata;

pub use ogg::opus_duration_ms;

use core_parser::{MessageKind, Source};
use core_storage::{Db, MediaRow};
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

/// Arquivos lidos do zip por lote; cada lote é processado em paralelo.
const BATCH: usize = 128;
const THUMB_SIZE: u32 = 128;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error(transparent)]
    Storage(#[from] core_storage::StorageError),
    #[error(transparent)]
    Source(#[from] core_parser::SourceError),
    #[error("operação cancelada")]
    Cancelled,
}

#[derive(Debug, Default, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaReport {
    pub stickers: usize,
    pub audios: usize,
    /// Arquivos referenciados na conversa, mas ausentes do zip.
    pub missing: usize,
    /// Arquivos presentes que não puderam ser decodificados.
    pub failed: usize,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Compõe a transparência sobre branco: pixels transparentes das figurinhas costumam ter
/// RGB arbitrário, que mudaria o hash sem mudar a imagem visível.
fn flatten_on_white(img: &DynamicImage) -> image::GrayImage {
    let rgba = img.to_rgba8();
    image::GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let a = p[3] as f32 / 255.0;
        let mix = |c: u8| c as f32 * a + 255.0 * (1.0 - a);
        let luma = 0.299 * mix(p[0]) + 0.587 * mix(p[1]) + 0.114 * mix(p[2]);
        image::Luma([luma.round() as u8])
    })
}

/// dHash de 64 bits: reduz para 9×8 em tons de cinza e compara pixels vizinhos na horizontal.
pub fn dhash(img: &DynamicImage) -> u64 {
    let gray = flatten_on_white(img);
    let small = image::imageops::resize(&gray, 9, 8, FilterType::Triangle);
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash <<= 1;
            if small.get_pixel(x, y).0[0] < small.get_pixel(x + 1, y).0[0] {
                hash |= 1;
            }
        }
    }
    hash
}

pub struct StickerInfo {
    pub dhash: u64,
    /// Miniatura PNG (até 128×128) para o dashboard e o visualizador.
    pub thumb_png: Vec<u8>,
}

/// Decodifica uma figurinha (primeiro frame, se for animada) e calcula hash e miniatura.
pub fn analyze_sticker(bytes: &[u8]) -> Option<StickerInfo> {
    let img = image::load_from_memory_with_format(bytes, ImageFormat::WebP)
        .or_else(|_| image::load_from_memory(bytes))
        .ok()?;
    let thumb = if img.dimensions().0 > THUMB_SIZE || img.dimensions().1 > THUMB_SIZE {
        img.thumbnail(THUMB_SIZE, THUMB_SIZE)
    } else {
        img.clone()
    };
    let mut png = Vec::new();
    thumb.write_to(&mut Cursor::new(&mut png), ImageFormat::Png).ok()?;
    Some(StickerInfo {
        dhash: dhash(&img),
        thumb_png: png,
    })
}

enum Processed {
    Sticker {
        row: MediaRow,
        thumb: Option<(String, Vec<u8>)>,
        failed: bool,
    },
    Audio {
        row: MediaRow,
        failed: bool,
    },
}

/// Lê do zip as figurinhas e os áudios referenciados na conversa, grava hashes e durações
/// na tabela `media` e uma miniatura por figurinha distinta. Arquivos já indexados são pulados.
pub fn index_media(
    db: &mut Db,
    source: &Source,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<MediaReport, MediaError> {
    let mut report = MediaReport::default();
    if !source.has_media() {
        return Ok(report);
    }
    let already = db.media_map()?;
    let wanted: HashMap<String, MessageKind> = db
        .referenced_media()?
        .into_iter()
        .filter(|(f, k)| matches!(k, MessageKind::Sticker | MessageKind::Audio) && !already.contains_key(f))
        .collect();
    let total = wanted.len();
    let mut known_thumbs: HashSet<String> = db.thumb_hashes()?;
    let mut done = 0;
    let mut found = HashSet::new();
    progress(0, total);

    let mut batch: Vec<(String, MessageKind, Vec<u8>)> = Vec::with_capacity(BATCH);
    let flush = |batch: &mut Vec<(String, MessageKind, Vec<u8>)>,
                 db: &mut Db,
                 report: &mut MediaReport,
                 known: &mut HashSet<String>|
     -> Result<usize, MediaError> {
        let processed: Vec<Processed> = batch
            .par_drain(..)
            .map(|(file, kind, bytes)| {
                let sha = sha256_hex(&bytes);
                match kind {
                    MessageKind::Audio => {
                        let d = opus_duration_ms(&bytes);
                        Processed::Audio {
                            failed: d.is_none(),
                            row: MediaRow {
                                file,
                                sha256: Some(sha),
                                phash: None,
                                duration_ms: d,
                            },
                        }
                    }
                    _ => {
                        let info = analyze_sticker(&bytes);
                        Processed::Sticker {
                            failed: info.is_none(),
                            row: MediaRow {
                                file,
                                sha256: Some(sha.clone()),
                                phash: info.as_ref().map(|i| format!("{:016x}", i.dhash)),
                                duration_ms: None,
                            },
                            thumb: info.map(|i| (sha, i.thumb_png)),
                        }
                    }
                }
            })
            .collect();
        let n = processed.len();
        let mut rows = Vec::with_capacity(n);
        let mut thumbs = Vec::new();
        for p in processed {
            match p {
                Processed::Sticker { row, thumb, failed } => {
                    report.stickers += 1;
                    report.failed += failed as usize;
                    if let Some((sha, png)) = thumb {
                        // Mesma figurinha com nomes diferentes: uma miniatura só.
                        if known.insert(sha.clone()) {
                            thumbs.push((sha, png));
                        }
                    }
                    rows.push(row);
                }
                Processed::Audio { row, failed } => {
                    report.audios += 1;
                    report.failed += failed as usize;
                    rows.push(row);
                }
            }
        }
        db.insert_media(&rows, &thumbs)?;
        Ok(n)
    };

    let mut result = Ok(());
    source.read_entries(
        |name| wanted.contains_key(name),
        |name, bytes| {
            if cancel.load(Ordering::Relaxed) {
                result = Err(MediaError::Cancelled);
                return false;
            }
            found.insert(name.to_string());
            batch.push((name.to_string(), wanted[name], bytes));
            if batch.len() >= BATCH {
                match flush(&mut batch, db, &mut report, &mut known_thumbs) {
                    Ok(n) => {
                        done += n;
                        progress(done, total);
                    }
                    Err(e) => {
                        result = Err(e);
                        return false;
                    }
                }
            }
            true
        },
    )?;
    result?;
    done += flush(&mut batch, db, &mut report, &mut known_thumbs)?;
    report.missing = total - found.len();
    progress(done.max(total), total);
    Ok(report)
}

#[cfg(test)]
mod tests;
