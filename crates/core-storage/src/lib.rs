//! Armazenamento SQLite: um arquivo `.db` por conversa importada.

mod import;
mod schema;

pub use import::{ImportOptions, ImportPhase, ImportProgress, ImportSummary, import_new};

use core_parser::{MessageKind, SourceError};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("erro no banco de dados: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Format(#[from] core_parser::FormatError),
    #[error("erro de leitura: {0}")]
    Io(#[from] std::io::Error),
    #[error("erro de serialização: {0}")]
    Json(#[from] serde_json::Error),
    #[error("conversas em grupo não são suportadas (autores encontrados: {})", .0.join(", "))]
    GroupNotSupported(Vec<String>),
    #[error("nenhuma mensagem com autor foi encontrada no arquivo")]
    NoAuthors,
    #[error("operação cancelada")]
    Cancelled,
    #[error("banco de dados em versão incompatível ({0})")]
    SchemaVersion(i32),
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRow {
    pub id: i64,
    pub ts: i64,
    pub author: Option<String>,
    pub kind: MessageKind,
    pub text: Option<String>,
    pub media_file: Option<String>,
    pub session_id: i64,
}

impl MessageRow {
    fn from_row(r: &rusqlite::Row) -> rusqlite::Result<Self> {
        let kind: String = r.get(3)?;
        Ok(Self {
            id: r.get(0)?,
            ts: r.get(1)?,
            author: r.get(2)?,
            kind: MessageKind::parse(&kind).unwrap_or(MessageKind::Text),
            text: r.get(4)?,
            media_file: r.get(5)?,
            session_id: r.get(6)?,
        })
    }
}

/// Linha da tabela `media`: resultado da indexação de uma figurinha ou áudio do zip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRow {
    pub file: String,
    pub sha256: Option<String>,
    /// dHash de 64 bits em hexadecimal (só figurinhas).
    pub phash: Option<String>,
    pub duration_ms: Option<i64>,
}

const MESSAGE_COLS: &str = "id, ts, author, kind, text, media_file, session_id";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: i64,
    pub start_id: i64,
    pub end_id: i64,
    pub start_ts: i64,
    pub end_ts: i64,
}

/// Divide a conversa em sessões: uma nova sessão começa quando o intervalo entre
/// duas mensagens consecutivas passa de `gap_secs`.
pub struct SessionBuilder {
    gap_secs: i64,
    sessions: Vec<SessionRow>,
}

impl SessionBuilder {
    pub fn new(gap_secs: i64) -> Self {
        Self {
            gap_secs,
            sessions: Vec::new(),
        }
    }

    /// Registra a próxima mensagem (em ordem de `id`) e devolve o id da sessão dela.
    pub fn push(&mut self, id: i64, ts: i64) -> i64 {
        match self.sessions.last_mut() {
            Some(s) if ts - s.end_ts <= self.gap_secs => {
                s.end_id = id;
                s.end_ts = s.end_ts.max(ts);
                s.id
            }
            _ => {
                let sid = self.sessions.len() as i64 + 1;
                self.sessions.push(SessionRow {
                    id: sid,
                    start_id: id,
                    end_id: id,
                    start_ts: ts,
                    end_ts: ts,
                });
                sid
            }
        }
    }

    pub fn finish(self) -> Vec<SessionRow> {
        self.sessions
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let mut v: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if !(1..=schema::VERSION).contains(&v) {
            return Err(StorageError::SchemaVersion(v));
        }
        while v < schema::VERSION {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(schema::MIGRATIONS[(v - 1) as usize])?;
            v += 1;
            tx.pragma_update(None, "user_version", v)?;
            tx.commit()?;
        }
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    pub fn meta_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        v.map(|s| serde_json::from_str(&s)).transpose().map_err(Into::into)
    }

    pub fn meta_set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        schema::meta_set(&self.conn, key, value)
    }

    pub fn summary(&self) -> Result<ImportSummary> {
        self.meta_get("summary")?.ok_or(StorageError::SchemaVersion(-1))
    }

    pub fn cache_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT json FROM stats_cache WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        v.map(|s| serde_json::from_str(&s)).transpose().map_err(Into::into)
    }

    pub fn cache_set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute(
            "INSERT INTO stats_cache(key, json, computed_at) VALUES (?1, ?2, strftime('%s','now'))
             ON CONFLICT(key) DO UPDATE SET json = excluded.json, computed_at = excluded.computed_at",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn message_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?)
    }

    /// Página de mensagens a partir de `start_id` (os ids são sequenciais, começando em 1).
    pub fn messages_from(&self, start_id: i64, limit: i64) -> Result<Vec<MessageRow>> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {MESSAGE_COLS} FROM messages WHERE id >= ?1 ORDER BY id LIMIT ?2"
        ))?;
        let rows = st.query_map(params![start_id, limit], MessageRow::from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Itera todas as mensagens em ordem, sem carregá-las de uma vez.
    pub fn for_each_message(&self, mut f: impl FnMut(MessageRow)) -> Result<()> {
        let mut st = self
            .conn
            .prepare(&format!("SELECT {MESSAGE_COLS} FROM messages ORDER BY id"))?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            f(MessageRow::from_row(r)?);
        }
        Ok(())
    }

    /// Busca full-text (FTS5). Cada termo vira um termo literal; o último aceita prefixo.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<MessageRow>> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let fts = format!("{}*", terms.join(" "));
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {MESSAGE_COLS} FROM messages WHERE id IN
               (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1)
             ORDER BY id LIMIT ?2"
        ))?;
        let rows = st.query_map(params![fts, limit], MessageRow::from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Primeira mensagem no instante `ts` ou depois dele (para "ir para data").
    pub fn first_id_at_or_after(&self, ts: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row("SELECT MIN(id) FROM messages WHERE ts >= ?1", [ts], |r| r.get(0))?)
    }

    pub fn sessions(&self) -> Result<Vec<SessionRow>> {
        let mut st = self
            .conn
            .prepare("SELECT id, start_id, end_id, start_ts, end_ts FROM sessions ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                start_id: r.get(1)?,
                end_id: r.get(2)?,
                start_ts: r.get(3)?,
                end_ts: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Arquivos de mídia referenciados nas mensagens, com o tipo inferido pelo nome.
    pub fn referenced_media(&self) -> Result<Vec<(String, MessageKind)>> {
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT media_file, kind FROM messages WHERE media_file IS NOT NULL")?;
        let rows = st.query_map([], |r| {
            let kind: String = r.get(1)?;
            Ok((r.get(0)?, MessageKind::parse(&kind).unwrap_or(MessageKind::Doc)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn media_map(&self) -> Result<HashMap<String, MediaRow>> {
        let mut st = self
            .conn
            .prepare("SELECT file, sha256, phash, duration_ms FROM media")?;
        let rows = st.query_map([], |r| {
            Ok(MediaRow {
                file: r.get(0)?,
                sha256: r.get(1)?,
                phash: r.get(2)?,
                duration_ms: r.get(3)?,
            })
        })?;
        rows.map(|r| r.map(|m| (m.file.clone(), m)))
            .collect::<rusqlite::Result<_>>()
            .map_err(Into::into)
    }

    pub fn thumb_hashes(&self) -> Result<HashSet<String>> {
        let mut st = self.conn.prepare("SELECT sha256 FROM media_thumbs")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Grava linhas de `media` e miniaturas `(sha256, png)` numa transação.
    pub fn insert_media(&mut self, rows: &[MediaRow], thumbs: &[(String, Vec<u8>)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut st = tx.prepare_cached(
                "INSERT OR REPLACE INTO media(file, sha256, phash, duration_ms) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for m in rows {
                st.execute(params![m.file, m.sha256, m.phash, m.duration_ms])?;
            }
            let mut st = tx.prepare_cached("INSERT OR IGNORE INTO media_thumbs(sha256, png) VALUES (?1, ?2)")?;
            for (sha, png) in thumbs {
                st.execute(params![sha, png])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Miniatura PNG da figurinha de um arquivo (via SHA-256).
    pub fn sticker_thumb(&self, file: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT t.png FROM media m JOIN media_thumbs t ON t.sha256 = m.sha256 WHERE m.file = ?1",
                [file],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Recalcula as sessões com um novo limite de intervalo.
    pub fn rebuild_sessions(&mut self, gap_secs: i64) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut builder = SessionBuilder::new(gap_secs);
        {
            let mut read = tx.prepare("SELECT id, ts FROM messages ORDER BY id")?;
            let mut upd = tx.prepare("UPDATE messages SET session_id = ?1 WHERE id = ?2 AND session_id IS NOT ?1")?;
            let pairs: Vec<(i64, i64)> = read
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (id, ts) in pairs {
                upd.execute(params![builder.push(id, ts), id])?;
            }
        }
        let sessions = builder.finish();
        tx.execute("DELETE FROM sessions", [])?;
        schema::insert_sessions(&tx, &sessions)?;
        tx.commit()?;
        let mut summary = self.summary()?;
        summary.session_count = sessions.len() as i64;
        summary.session_gap_secs = gap_secs;
        self.meta_set("summary", &summary)?;
        Ok(sessions.len())
    }
}

#[cfg(test)]
mod tests;
