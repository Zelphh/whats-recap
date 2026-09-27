use super::*;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

const CHAT: &str = "\
12/01/2023 08:00 - Ana: bom dia
12/01/2023 08:05 - Bruno: bom dia! vamos viajar pra Floripa?
12/01/2023 08:06 - Ana: bora
12/01/2023 20:00 - Bruno: comprei as passagens
13/01/2023 09:00 - Ana: <Mídia oculta>
";

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wr-storage-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn import(dir: &Path, chat: &str) -> Result<(Db, ImportSummary)> {
    let txt = dir.join("chat.txt");
    std::fs::write(&txt, chat).unwrap();
    let db_path = dir.join("chat.db");
    let mut phases = Vec::new();
    let summary = import_new(
        &db_path,
        &core_parser::Source::open(&txt)?,
        &ImportOptions::default(),
        &mut |p| phases.push(p.phase),
        &AtomicBool::new(false),
    )?;
    assert_eq!(phases.first(), Some(&ImportPhase::Detect));
    assert!(!db_path.with_extension("db.partial").exists());
    Ok((Db::open(&db_path)?, summary))
}

#[test]
fn import_and_query() {
    let dir = tmp("basic");
    let (mut db, s) = import(&dir, CHAT).unwrap();
    assert_eq!(s.message_count, 5);
    assert_eq!(s.authors, vec!["Ana", "Bruno"]);
    // 08:06 → 20:00 passa de 6h: nova sessão; 20:00 → 09:00 também.
    assert_eq!(s.session_count, 3);
    assert_eq!(db.summary().unwrap().message_count, 5);

    let page = db.messages_from(2, 2).unwrap();
    assert_eq!(page.iter().map(|m| m.id).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(page[0].session_id, 1);
    assert_eq!(db.messages_from(4, 1).unwrap()[0].session_id, 2);

    let hits = db.search("floripa", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 2);
    // Prefixo e acentos.
    assert_eq!(db.search("passag", 10).unwrap().len(), 1);
    assert!(db.search("\"", 10).is_ok());
    let any = db.search_any(&["viajar".into(), "passagens".into()], 10).unwrap();
    assert_eq!(any.iter().map(|m| m.id).collect::<Vec<_>>().len(), 2);
    assert!(db.search_any(&[], 10).unwrap().is_empty());

    let ts = chrono::NaiveDate::from_ymd_opt(2023, 1, 13)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    assert_eq!(db.first_id_at_or_after(ts.and_utc().timestamp()).unwrap(), Some(5));

    assert_eq!(db.rebuild_sessions(24 * 3600).unwrap(), 1);
    assert_eq!(db.summary().unwrap().session_count, 1);
    assert_eq!(db.messages_from(5, 1).unwrap()[0].session_id, 1);

    db.cache_set("x", &vec![1, 2, 3]).unwrap();
    assert_eq!(db.cache_get::<Vec<i32>>("x").unwrap(), Some(vec![1, 2, 3]));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn rejects_groups() {
    let dir = tmp("group");
    let chat = format!("{CHAT}13/01/2023 09:01 - Carla: oi\n");
    assert!(matches!(import(&dir, &chat), Err(StorageError::GroupNotSupported(a)) if a.len() == 3));
    assert!(!dir.join("chat.db").exists());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn cancelled_import_leaves_nothing() {
    let dir = tmp("cancel");
    let txt = dir.join("chat.txt");
    std::fs::write(&txt, CHAT).unwrap();
    let db_path = dir.join("chat.db");
    let r = import_new(
        &db_path,
        &core_parser::Source::open(&txt).unwrap(),
        &ImportOptions::default(),
        &mut |_| {},
        &AtomicBool::new(true),
    );
    assert!(matches!(r, Err(StorageError::Cancelled)));
    assert!(!db_path.exists());
    assert!(!db_path.with_extension("db.partial").exists());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn migrates_v1_database() {
    let dir = tmp("migrate");
    let (db, _) = import(&dir, CHAT).unwrap();
    // Simula um banco criado antes da fase 4.
    db.conn()
        .execute_batch("DROP TABLE media_thumbs; PRAGMA user_version = 1;")
        .unwrap();
    drop(db);
    let mut db = Db::open(dir.join("chat.db")).unwrap();
    let v: i32 = db
        .conn()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(v, schema::VERSION);
    let row = MediaRow {
        file: "STK-1.webp".into(),
        sha256: Some("ab".into()),
        phash: Some("00ff".into()),
        duration_ms: None,
    };
    db.insert_media(std::slice::from_ref(&row), &[("ab".into(), vec![1, 2, 3])])
        .unwrap();
    assert_eq!(db.sticker_thumb("STK-1.webp").unwrap(), Some(vec![1, 2, 3]));
    assert_eq!(db.media_map().unwrap()["STK-1.webp"], row);
    std::fs::remove_dir_all(dir).ok();
}
