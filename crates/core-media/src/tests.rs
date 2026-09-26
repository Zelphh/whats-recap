use super::*;
use crate::testdata::{opus_file, sticker_webp};
use std::io::Write;

#[test]
fn opus_duration() {
    assert_eq!(opus_duration_ms(&opus_file(42_000)), Some(42_000));
    assert_eq!(opus_duration_ms(&opus_file(1_250)), Some(1_250));
    assert_eq!(opus_duration_ms(b"not ogg"), None);
    // Truncado no meio de uma página: usa a última página completa (sem granule válido → None).
    let f = opus_file(5_000);
    assert_eq!(opus_duration_ms(&f[..60]), None);
}

#[test]
fn sticker_hashes() {
    let a = sticker_webp(1, 0);
    let a2 = sticker_webp(1, 7);
    let b = sticker_webp(2, 0);
    assert_ne!(sha256_hex(&a), sha256_hex(&a2), "reencode muda o SHA-256");
    let (ha, ha2, hb) = (
        analyze_sticker(&a).unwrap().dhash,
        analyze_sticker(&a2).unwrap().dhash,
        analyze_sticker(&b).unwrap().dhash,
    );
    assert!((ha ^ ha2).count_ones() <= 2, "mesma figurinha visível → dHash próximo");
    assert!((ha ^ hb).count_ones() > 10, "figurinhas diferentes → dHash distante");
    let thumb = analyze_sticker(&a).unwrap().thumb_png;
    let t = image::load_from_memory(&thumb).unwrap();
    assert_eq!((t.width(), t.height()), (128, 128));
    assert!(analyze_sticker(b"lixo").is_none());
}

#[test]
fn index_zip() {
    let dir = std::env::temp_dir().join(format!("wr-media-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let zip_path = dir.join("chat.zip");
    let chat = "\
12/01/2023 14:30 - Ana: STK-20230112-WA0001.webp (arquivo anexado)
12/01/2023 14:31 - Bruno: STK-20230112-WA0002.webp (arquivo anexado)
12/01/2023 14:32 - Ana: PTT-20230112-WA0003.opus (arquivo anexado)
12/01/2023 14:33 - Bruno: STK-20230112-WA0004.webp (arquivo anexado)
12/01/2023 14:34 - Bruno: IMG-20230112-WA0005.jpg (arquivo anexado)
";
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        w.start_file("Conversa do WhatsApp com Bruno.txt", o).unwrap();
        w.write_all(chat.as_bytes()).unwrap();
        for (name, bytes) in [
            ("STK-20230112-WA0001.webp", sticker_webp(1, 0)),
            ("STK-20230112-WA0002.webp", sticker_webp(1, 0)), // mesmo conteúdo, outro nome
            ("PTT-20230112-WA0003.opus", opus_file(3_000)),
            ("IMG-20230112-WA0005.jpg", vec![0xFF, 0xD8]),
            // WA0004 ausente do zip
        ] {
            w.start_file(name, o).unwrap();
            w.write_all(&bytes).unwrap();
        }
        w.finish().unwrap();
    }
    let source = Source::open(&zip_path).unwrap();
    let db_path = dir.join("chat.db");
    let never = AtomicBool::new(false);
    core_storage::import_new(&db_path, &source, &Default::default(), &mut |_| {}, &never).unwrap();
    let mut db = Db::open(&db_path).unwrap();

    let mut calls = Vec::new();
    let report = index_media(&mut db, &source, &never, &mut |d, t| calls.push((d, t))).unwrap();
    assert_eq!(
        (report.stickers, report.audios, report.missing, report.failed),
        (2, 1, 1, 0)
    );
    assert_eq!(calls.last(), Some(&(4, 4)));

    let media = db.media_map().unwrap();
    assert_eq!(media["PTT-20230112-WA0003.opus"].duration_ms, Some(3_000));
    assert_eq!(
        media["STK-20230112-WA0001.webp"].sha256,
        media["STK-20230112-WA0002.webp"].sha256
    );
    assert_eq!(
        db.thumb_hashes().unwrap().len(),
        1,
        "uma miniatura por conteúdo distinto"
    );
    assert!(db.sticker_thumb("STK-20230112-WA0002.webp").unwrap().is_some());

    // Segunda execução não reprocessa nada.
    let again = index_media(&mut db, &source, &never, &mut |_, _| {}).unwrap();
    assert_eq!(again.stickers + again.audios, 0);

    // Cancelamento.
    db.conn().execute("DELETE FROM media", []).unwrap();
    let r = index_media(&mut db, &source, &AtomicBool::new(true), &mut |_, _| {});
    assert!(matches!(r, Err(MediaError::Cancelled)));
    std::fs::remove_dir_all(dir).ok();
}
