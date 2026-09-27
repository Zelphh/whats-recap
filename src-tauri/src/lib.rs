//! App Tauri: expõe os crates `core-*` ao frontend via commands.
//!
//! Cada conversa importada vira um arquivo `<id>.db` em `<app_data>/conversations`.
//! O frontend nunca recebe a conversa inteira: pede páginas, buscas e o cache de estatísticas.

use core_stats::{STATS_VERSION, Stats, StatsSettings, compute_from_db};
use core_storage::{Db, ImportOptions, ImportProgress, ImportSummary, MessageRow, import_new};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

type CmdResult<T> = Result<T, String>;

const STATS_KEY: &str = "stats";
const SETTINGS_KEY: &str = "settings";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConversationInfo {
    id: String,
    summary: ImportSummary,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    /// `detect` | `parse` | `index` | `media` | `stats`
    phase: String,
    fraction: f32,
    messages: u64,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn conversations_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = app.path().app_data_dir().map_err(err)?.join("conversations");
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

fn db_path(app: &AppHandle, id: &str) -> CmdResult<PathBuf> {
    // O id vira nome de arquivo: aceita só caracteres seguros.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("id de conversa inválido: {id}"));
    }
    Ok(conversations_dir(app)?.join(format!("{id}.db")))
}

fn open(app: &AppHandle, id: &str) -> CmdResult<Db> {
    Db::open(db_path(app, id)?).map_err(err)
}

fn settings_of(db: &Db) -> CmdResult<StatsSettings> {
    Ok(db.meta_get(SETTINGS_KEY).map_err(err)?.unwrap_or_default())
}

fn recompute(db: &Db) -> CmdResult<Stats> {
    let stats = compute_from_db(db, &settings_of(db)?).map_err(err)?;
    db.cache_set(STATS_KEY, &stats).map_err(err)?;
    Ok(stats)
}

/// Roda trabalho pesado fora da thread principal.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[tauri::command]
async fn list_conversations(app: AppHandle) -> CmdResult<Vec<ConversationInfo>> {
    blocking(move || {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(conversations_dir(&app)?).map_err(err)? {
            let path = entry.map_err(err)?.path();
            if path.extension().is_none_or(|e| e != "db") {
                continue;
            }
            let id = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            // Bancos corrompidos ou de outra versão não impedem a listagem dos demais.
            if let Ok(summary) = Db::open(&path).and_then(|db| db.summary()) {
                out.push(ConversationInfo { id, summary });
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.summary.imported_at));
        Ok(out)
    })
    .await
}

/// Uma importação por vez; `cancel_import` sinaliza a que estiver em andamento.
#[derive(Default)]
struct ImportState {
    cancel: Arc<AtomicBool>,
    running: AtomicBool,
}

fn remove_db_files(path: &std::path::Path) -> CmdResult<()> {
    for suffix in ["", "-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{suffix}", path.display()));
        if p.exists() {
            std::fs::remove_file(p).map_err(err)?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn import_conversation(app: AppHandle, path: String) -> CmdResult<ConversationInfo> {
    let state = app.state::<ImportState>();
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("já existe uma importação em andamento".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();
    let app2 = app.clone();
    let result = blocking(move || {
        let app = app2;
        let id = format!(
            "c{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(err)?
                .as_millis()
        );
        let target = db_path(&app, &id)?;
        let source = core_parser::Source::open(&path).map_err(err)?;
        let settings = StatsSettings::default();
        let opts = ImportOptions {
            session_gap_secs: settings.session_gap_secs,
        };

        let emit = |phase: &str, fraction: f32, messages: u64| {
            let _ = app.emit(
                "import-progress",
                ProgressEvent {
                    phase: phase.into(),
                    fraction,
                    messages,
                },
            );
        };
        let mut on_progress = |p: ImportProgress| {
            let phase = serde_json::to_value(p.phase)
                .ok()
                .and_then(|v| v.as_str().map(String::from));
            emit(phase.as_deref().unwrap_or(""), p.fraction, p.messages);
        };
        let summary = import_new(&target, &source, &opts, &mut on_progress, &cancel).map_err(err)?;

        // Depois do rename, qualquer falha (ou cancelamento) remove o banco para não deixar lixo.
        let finish = || -> CmdResult<()> {
            let mut db = Db::open(&target).map_err(err)?;
            db.meta_set(SETTINGS_KEY, &settings).map_err(err)?;
            if source.has_media() {
                emit("media", 0.0, 0);
                core_media::index_media(&mut db, &source, &cancel, &mut |done, total| {
                    emit(
                        "media",
                        if total > 0 { done as f32 / total as f32 } else { 1.0 },
                        done as u64,
                    )
                })
                .map_err(err)?;
            }
            emit("stats", 0.0, summary.message_count as u64);
            recompute(&db)?;
            emit("stats", 1.0, summary.message_count as u64);
            Ok(())
        };
        if let Err(e) = finish() {
            let _ = remove_db_files(&target);
            return Err(e);
        }
        Ok(ConversationInfo { id, summary })
    })
    .await;
    app.state::<ImportState>().running.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
fn cancel_import(state: tauri::State<ImportState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Miniatura PNG de uma figurinha (bytes crus; vazio se não houver).
#[tauri::command]
async fn get_sticker_thumb(app: AppHandle, id: String, file: String) -> CmdResult<tauri::ipc::Response> {
    let png = open(&app, &id)?.sticker_thumb(&file).map_err(err)?.unwrap_or_default();
    Ok(tauri::ipc::Response::new(png))
}

#[tauri::command]
async fn delete_conversation(app: AppHandle, id: String) -> CmdResult<()> {
    remove_db_files(&db_path(&app, &id)?)
}

#[tauri::command]
async fn get_stats(app: AppHandle, id: String) -> CmdResult<Stats> {
    blocking(move || {
        let db = open(&app, &id)?;
        match db.cache_get::<Stats>(STATS_KEY) {
            Ok(Some(s)) if s.version == STATS_VERSION => Ok(s),
            // Cache ausente, de outra versão ou ilegível: recalcula.
            _ => recompute(&db),
        }
    })
    .await
}

#[tauri::command]
async fn get_settings(app: AppHandle, id: String) -> CmdResult<StatsSettings> {
    settings_of(&open(&app, &id)?)
}

#[tauri::command]
fn default_settings() -> StatsSettings {
    StatsSettings::default()
}

#[tauri::command]
async fn update_settings(app: AppHandle, id: String, settings: StatsSettings) -> CmdResult<Stats> {
    blocking(move || {
        if settings.session_gap_secs < 60 {
            return Err("o intervalo de sessão precisa ser de pelo menos 1 minuto".into());
        }
        let mut db = open(&app, &id)?;
        let old = settings_of(&db)?;
        if old.session_gap_secs != settings.session_gap_secs {
            db.rebuild_sessions(settings.session_gap_secs).map_err(err)?;
        }
        db.meta_set(SETTINGS_KEY, &settings).map_err(err)?;
        recompute(&db)
    })
    .await
}

#[tauri::command]
async fn get_summary(app: AppHandle, id: String) -> CmdResult<ImportSummary> {
    open(&app, &id)?.summary().map_err(err)
}

#[tauri::command]
async fn get_messages(app: AppHandle, id: String, start_id: i64, limit: i64) -> CmdResult<Vec<MessageRow>> {
    blocking(move || {
        open(&app, &id)?
            .messages_from(start_id, limit.clamp(1, 1000))
            .map_err(err)
    })
    .await
}

#[tauri::command]
async fn search_messages(app: AppHandle, id: String, query: String, limit: i64) -> CmdResult<Vec<MessageRow>> {
    blocking(move || open(&app, &id)?.search(&query, limit.clamp(1, 1000)).map_err(err)).await
}

/// Id da primeira mensagem na data (`AAAA-MM-DD`) ou depois dela.
#[tauri::command]
async fn locate_date(app: AppHandle, id: String, date: String) -> CmdResult<Option<i64>> {
    let d = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(err)?;
    let ts = d.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc().timestamp();
    open(&app, &id)?.first_id_at_or_after(ts).map_err(err)
}

/// Pergunta ao chat (tipo 1: contagens). A resposta traz o texto, a ferramenta usada e as citações.
#[tauri::command]
async fn chat_ask(app: AppHandle, id: String, question: String) -> CmdResult<core_chat::ChatAnswer> {
    blocking(move || core_chat::ask(&open(&app, &id)?, &question).map_err(err)).await
}

/// Perguntas de exemplo com os nomes da conversa.
#[tauri::command]
async fn chat_examples(app: AppHandle, id: String) -> CmdResult<Vec<String>> {
    Ok(core_chat::examples(&open(&app, &id)?.summary().map_err(err)?.authors))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ImportState::default())
        .invoke_handler(tauri::generate_handler![
            list_conversations,
            import_conversation,
            cancel_import,
            get_sticker_thumb,
            chat_ask,
            chat_examples,
            delete_conversation,
            get_stats,
            get_settings,
            default_settings,
            update_settings,
            get_summary,
            get_messages,
            search_messages,
            locate_date,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o app");
}
