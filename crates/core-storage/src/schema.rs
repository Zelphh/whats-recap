use crate::{Result, SessionRow};
use rusqlite::{Connection, params};
use serde::Serialize;

pub const VERSION: i32 = 2;

pub(crate) const CREATE: &str = r#"
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE messages (
    id         INTEGER PRIMARY KEY,   -- sequencial, usado como referência pelo LLM
    ts         INTEGER NOT NULL,      -- timestamp local (segundos, sem fuso)
    author     TEXT,                  -- NULL para mensagens de sistema
    kind       TEXT NOT NULL,
    text       TEXT,
    media_file TEXT,
    session_id INTEGER
);

CREATE TABLE sessions (
    id       INTEGER PRIMARY KEY,
    start_id INTEGER NOT NULL,
    end_id   INTEGER NOT NULL,
    start_ts INTEGER NOT NULL,
    end_ts   INTEGER NOT NULL
);

CREATE TABLE media (
    file        TEXT PRIMARY KEY,
    sha256      TEXT,
    phash       TEXT,
    duration_ms INTEGER
);

-- Uma miniatura por figurinha distinta (mesmo conteúdo = mesmo SHA-256).
CREATE TABLE media_thumbs (
    sha256 TEXT PRIMARY KEY,
    png    BLOB NOT NULL
);

CREATE TABLE llm_events (
    id           INTEGER PRIMARY KEY,
    type         TEXT NOT NULL,
    start_id     INTEGER,
    end_id       INTEGER,
    payload_json TEXT
);

CREATE TABLE llm_jobs (
    id          INTEGER PRIMARY KEY,
    type        TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    status      TEXT NOT NULL,
    updated_at  INTEGER
);

CREATE TABLE chunks (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER,
    start_id   INTEGER NOT NULL,
    end_id     INTEGER NOT NULL,
    text       TEXT NOT NULL,
    embedding  BLOB
);

CREATE TABLE stats_cache (
    key         TEXT PRIMARY KEY,
    json        TEXT NOT NULL,
    computed_at INTEGER
);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    text,
    content = 'messages',
    content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);
"#;

// Criados depois da inserção em lote, que fica bem mais rápida sem índices.
pub(crate) const INDEXES: &str = r#"
CREATE INDEX idx_messages_ts ON messages(ts);
CREATE INDEX idx_messages_author ON messages(author);
CREATE INDEX idx_messages_session ON messages(session_id);
CREATE INDEX idx_llm_events_type ON llm_events(type, start_id);
CREATE INDEX idx_llm_jobs_type ON llm_jobs(type, chunk_index);
INSERT INTO messages_fts(messages_fts) VALUES ('rebuild');
"#;

/// Migrações incrementais: `MIGRATIONS[i]` leva o banco da versão `i + 1` para `i + 2`.
pub(crate) const MIGRATIONS: &[&str] = &[
    // v1 → v2: miniaturas de figurinhas (fase 4).
    "CREATE TABLE media_thumbs (sha256 TEXT PRIMARY KEY, png BLOB NOT NULL);",
];

pub(crate) fn meta_set<T: Serialize + ?Sized>(conn: &Connection, key: &str, value: &T) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, serde_json::to_string(value)?],
    )?;
    Ok(())
}

pub(crate) fn insert_sessions(conn: &Connection, sessions: &[SessionRow]) -> Result<()> {
    let mut st =
        conn.prepare("INSERT INTO sessions(id, start_id, end_id, start_ts, end_ts) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for s in sessions {
        st.execute(params![s.id, s.start_id, s.end_id, s.start_ts, s.end_ts])?;
    }
    Ok(())
}
