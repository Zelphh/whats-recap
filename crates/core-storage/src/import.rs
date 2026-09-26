use crate::schema;
use crate::{Result, SessionBuilder, StorageError};
use core_parser::{CountingReader, DateOrder, MessageReader, Platform, Source, detect_format};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const BATCH: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    /// Intervalo que inicia uma nova sessão (padrão: 6h).
    pub session_gap_secs: i64,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            session_gap_secs: 6 * 3600,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Detect,
    Parse,
    Index,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: ImportPhase,
    /// 0.0–1.0 dentro da fase.
    pub fraction: f32,
    pub messages: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub source_name: String,
    pub has_media: bool,
    pub platform: Platform,
    pub date_order: DateOrder,
    pub authors: Vec<String>,
    pub message_count: i64,
    pub first_ts: i64,
    pub last_ts: i64,
    pub session_count: i64,
    pub session_gap_secs: i64,
    pub imported_at: i64,
    /// Linhas que não puderam ser associadas a nenhuma mensagem.
    pub orphan_lines: u64,
    pub invalid_dates: u64,
}

/// Importa um export para um novo banco em `db_path`.
///
/// O banco é montado em `<db_path>.partial` e só é renomeado ao final, então uma
/// importação interrompida nunca deixa um banco pela metade no lugar do definitivo.
pub fn import_new(
    db_path: &Path,
    source: &Source,
    opts: &ImportOptions,
    progress: &mut dyn FnMut(ImportProgress),
    cancel: &AtomicBool,
) -> Result<ImportSummary> {
    progress(ImportProgress {
        phase: ImportPhase::Detect,
        fraction: 0.0,
        messages: 0,
    });
    let format = source.with_reader(|r, _| detect_format(r).map_err(StorageError::from))?;
    if format.authors.len() > 2 {
        return Err(StorageError::GroupNotSupported(format.authors));
    }
    if format.authors.is_empty() {
        return Err(StorageError::NoAuthors);
    }

    let partial = db_path.with_extension("db.partial");
    if partial.exists() {
        std::fs::remove_file(&partial)?;
    }
    let mut conn = Connection::open(&partial)?;
    // Seguro porque o arquivo só vira definitivo depois do rename.
    conn.pragma_update(None, "journal_mode", "OFF")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    conn.execute_batch(schema::CREATE)?;
    conn.pragma_update(None, "user_version", schema::VERSION)?;

    let mut builder = SessionBuilder::new(opts.session_gap_secs);
    let parsed = source.with_reader(|r, total| {
        let (counting, bytes) = CountingReader::new(r);
        let mut reader = MessageReader::new(counting, format.clone());
        let (mut id, mut first_ts, mut last_ts) = (0i64, i64::MAX, i64::MIN);
        let mut done = false;
        while !done {
            if cancel.load(Ordering::Relaxed) {
                return Err(StorageError::Cancelled);
            }
            let tx = conn.transaction()?;
            {
                let mut st = tx.prepare_cached(
                    "INSERT INTO messages(id, ts, author, kind, text, media_file, session_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for _ in 0..BATCH {
                    let Some(m) = reader.next() else {
                        done = true;
                        break;
                    };
                    let m = m?;
                    id += 1;
                    let ts = m.ts.and_utc().timestamp();
                    first_ts = first_ts.min(ts);
                    last_ts = last_ts.max(ts);
                    let session = builder.push(id, ts);
                    st.execute(params![
                        id,
                        ts,
                        m.author,
                        m.kind.as_str(),
                        m.text,
                        m.media_file,
                        session
                    ])?;
                }
            }
            tx.commit()?;
            let read = bytes.load(Ordering::Relaxed);
            progress(ImportProgress {
                phase: ImportPhase::Parse,
                fraction: if total > 0 {
                    (read as f32 / total as f32).min(1.0)
                } else {
                    1.0
                },
                messages: id as u64,
            });
        }
        Ok::<_, StorageError>((id, first_ts, last_ts, reader.stats()))
    });
    let (count, first_ts, last_ts, stats) = match parsed {
        Ok(v) => v,
        Err(e) => {
            drop(conn);
            let _ = std::fs::remove_file(&partial);
            return Err(e);
        }
    };

    progress(ImportProgress {
        phase: ImportPhase::Index,
        fraction: 0.0,
        messages: count as u64,
    });
    let sessions = builder.finish();
    let tx = conn.transaction()?;
    schema::insert_sessions(&tx, &sessions)?;
    tx.execute_batch(schema::INDEXES)?;

    let summary = ImportSummary {
        source_name: source
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        has_media: source.has_media(),
        platform: format.platform,
        date_order: format.order,
        authors: format.authors,
        message_count: count,
        first_ts: if count > 0 { first_ts } else { 0 },
        last_ts: if count > 0 { last_ts } else { 0 },
        session_count: sessions.len() as i64,
        session_gap_secs: opts.session_gap_secs,
        imported_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        orphan_lines: stats.orphan_lines,
        invalid_dates: stats.invalid_dates,
    };
    schema::meta_set(&tx, "summary", &summary)?;
    tx.commit()?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    drop(conn);

    std::fs::rename(&partial, db_path)?;
    progress(ImportProgress {
        phase: ImportPhase::Index,
        fraction: 1.0,
        messages: count as u64,
    });
    Ok(summary)
}
