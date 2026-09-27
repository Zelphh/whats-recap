use super::*;
use chrono::NaiveDate;
use std::io::Cursor;

fn parse(input: &str) -> (ChatFormat, Vec<ParsedMessage>) {
    let format = detect_format(Cursor::new(input)).expect("formato");
    let msgs = MessageReader::new(Cursor::new(input), format.clone())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    (format, msgs)
}

fn dt(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> chrono::NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .and_hms_opt(h, min, s)
        .unwrap()
}

const ANDROID: &str = "\
12/01/2023 14:30 - As mensagens e ligações são protegidas com a criptografia de ponta a ponta e ficam somente entre você e os participantes desta conversa.
12/01/2023 14:30 - Ana: oi!
12/01/2023 14:31 - Bruno: oi, tudo bem?
segunda linha
terceira linha
13/01/2023 09:00 - Ana: <Mídia oculta>
13/01/2023 09:01 - Bruno: Mensagem apagada
13/01/2023 09:02 - Ana: vou sim <Mensagem editada>
13/01/2023 09:03 - Bruno: STK-20230113-WA0001.webp (arquivo anexado)
13/01/2023 09:04 - Ana: PTT-20230113-WA0002.opus (arquivo anexado)
13/01/2023 09:05 - Bruno: IMG-20230113-WA0003.jpg (arquivo anexado)
olha isso
13/01/2023 09:06 - Ana: horário: 10h
";

#[test]
fn android_basic() {
    let (fmt, m) = parse(ANDROID);
    assert_eq!(fmt.platform, Platform::Android);
    assert_eq!(fmt.order, DateOrder::Dmy);
    assert_eq!(fmt.authors, vec!["Ana", "Bruno"]);
    assert_eq!(m.len(), 10);

    assert_eq!(m[0].kind, MessageKind::System);
    assert_eq!(m[0].author, None);

    assert_eq!(m[1].ts, dt(2023, 1, 12, 14, 30, 0));
    assert_eq!(m[1].author.as_deref(), Some("Ana"));
    assert_eq!(m[1].text.as_deref(), Some("oi!"));

    assert_eq!(
        m[2].text.as_deref(),
        Some("oi, tudo bem?\nsegunda linha\nterceira linha")
    );
    assert_eq!(m[3].kind, MessageKind::MediaHidden);
    assert_eq!(m[4].kind, MessageKind::Deleted);
    assert_eq!(m[5].text.as_deref(), Some("vou sim"));

    assert_eq!(m[6].kind, MessageKind::Sticker);
    assert_eq!(m[6].media_file.as_deref(), Some("STK-20230113-WA0001.webp"));
    assert_eq!(m[7].kind, MessageKind::Audio);
    assert_eq!(m[8].kind, MessageKind::Image);
    assert_eq!(m[8].text.as_deref(), Some("olha isso"));
    // ": " dentro do texto não confunde a separação do autor.
    assert_eq!(m[9].author.as_deref(), Some("Ana"));
    assert_eq!(m[9].text.as_deref(), Some("horário: 10h"));
}

const IOS: &str = "\u{FEFF}[12/01/2023, 14:30:15] Ana: \u{200E}As mensagens e as ligações são protegidas com a criptografia de ponta a ponta.
[12/01/2023, 14:30:20] Ana: oi
\u{200E}[12/01/2023, 14:31:02] Bruno: \u{200E}<anexado: 00000003-STICKER-2023-01-12-14-31-02.webp>
[12/01/2023, 14:32:00] Bruno: \u{200E}imagem ocultada
[12/01/2023, 14:33:00] Ana: \u{200E}Esta mensagem foi apagada.
[12/01/2023, 14:34:00] Bruno: \u{200E}Chamada de voz perdida
[12/01/2023, 14:35:00] Ana: tá bom \u{200E}<Esta mensagem foi editada>
";

#[test]
fn ios_basic() {
    let (fmt, m) = parse(IOS);
    assert_eq!(fmt.platform, Platform::Ios);
    assert_eq!(fmt.authors, vec!["Ana", "Bruno"]);
    assert_eq!(m.len(), 7);
    assert_eq!(m[0].kind, MessageKind::System);
    assert_eq!(m[0].author, None);
    assert_eq!(m[1].ts, dt(2023, 1, 12, 14, 30, 20));
    assert_eq!(m[2].kind, MessageKind::Sticker);
    assert_eq!(
        m[2].media_file.as_deref(),
        Some("00000003-STICKER-2023-01-12-14-31-02.webp")
    );
    assert_eq!(m[3].kind, MessageKind::Image);
    assert_eq!(m[3].media_file, None);
    assert_eq!(m[4].kind, MessageKind::Deleted);
    assert_eq!(m[5].kind, MessageKind::System);
    assert_eq!(m[6].text.as_deref(), Some("tá bom"));
}

#[test]
fn date_order_by_field_above_12() {
    let mdy = "01/13/2023 10:00 - Ana: a\n01/14/2023 10:00 - Bruno: b\n";
    assert_eq!(parse(mdy).0.order, DateOrder::Mdy);
    let dmy = "13/01/2023 10:00 - Ana: a\n14/01/2023 10:00 - Bruno: b\n";
    assert_eq!(parse(dmy).0.order, DateOrder::Dmy);
}

#[test]
fn date_order_ambiguous_uses_chronology() {
    // Em M/D, as datas ficam em ordem (1/2, 2/2, 3/2); em D/M, voltariam no tempo.
    let s = "02/01/2023 10:00 - Ana: a\n02/02/2023 10:00 - Bruno: b\n02/03/2023 10:00 - Ana: c\n01/04/2023 10:00 - Ana: d\n";
    // D/M: 2/jan, 2/fev, 2/mar, 1/abr (em ordem). M/D: 1/fev, 2/fev, 3/fev, 4/jan (inversão).
    assert_eq!(parse(s).0.order, DateOrder::Dmy);
    let s = "01/02/2023 10:00 - Ana: a\n02/02/2023 10:00 - Bruno: b\n03/02/2023 10:00 - Ana: c\n04/01/2023 10:00 - Ana: d\n";
    assert_eq!(parse(s).0.order, DateOrder::Mdy);
}

#[test]
fn twelve_hour_clock_and_short_year() {
    let s = "1/5/23, 2:30\u{202F}PM - Ana: tarde\n1/5/23, 12:05 AM - Bruno: madrugada\n";
    let (_, m) = parse(s);
    assert_eq!(m[0].ts.time(), chrono::NaiveTime::from_hms_opt(14, 30, 0).unwrap());
    assert_eq!(m[1].ts.time(), chrono::NaiveTime::from_hms_opt(0, 5, 0).unwrap());
    assert_eq!(chrono::Datelike::year(&m[0].ts), 2023);
}

#[test]
fn unrecognized_file() {
    assert!(matches!(
        detect_format(Cursor::new("olá\nmundo\n")),
        Err(FormatError::Unrecognized)
    ));
}

#[test]
fn group_is_visible_in_authors() {
    let s = "12/01/2023 14:30 - Ana: a\n12/01/2023 14:31 - Bruno: b\n12/01/2023 14:32 - Carla: c\n";
    assert_eq!(parse(s).0.authors.len(), 3);
}

#[test]
fn crlf_and_orphan_lines() {
    let s = "cabeçalho solto\r\n12/01/2023 14:30 - Ana: oi\r\ncontinua\r\n";
    let format = detect_format(Cursor::new(s)).unwrap();
    let mut r = MessageReader::new(Cursor::new(s), format);
    let m: Vec<_> = r.by_ref().map(Result::unwrap).collect();
    assert_eq!(m[0].text.as_deref(), Some("oi\ncontinua"));
    assert_eq!(r.stats().orphan_lines, 1);
}

#[test]
fn zip_source() {
    use std::io::Write;
    let dir = std::env::temp_dir().join(format!("wr-parser-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("chat.zip");
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        w.start_file("STK-20230113-WA0001.webp", opts).unwrap();
        w.write_all(b"RIFF").unwrap();
        w.start_file("Conversa do WhatsApp com Bruno.txt", opts).unwrap();
        w.write_all(ANDROID.as_bytes()).unwrap();
        w.finish().unwrap();
    }
    let src = Source::open(&path).unwrap();
    assert!(src.has_media());
    let fmt = src
        .with_reader(|r, _| detect_format(r).map_err(Box::<dyn std::error::Error>::from))
        .unwrap();
    assert_eq!(fmt.authors.len(), 2);
    let n = src
        .with_reader(|r, _| Ok::<_, SourceError>(MessageReader::new(r, fmt.clone()).count()))
        .unwrap();
    assert_eq!(n, 10);
    assert_eq!(src.media_files().unwrap(), vec!["STK-20230113-WA0001.webp"]);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn split_line_keeps_raw_pieces() {
    let (fmt, _) = parse(IOS);
    let line = "\u{200E}[12/01/2023, 14:31:02] Bruno: \u{200E}<anexado: x.webp>";
    let p = fmt.split_line(line).unwrap();
    assert_eq!(p.prefix, "\u{200E}[12/01/2023, 14:31:02] ");
    assert_eq!(p.author, Some("Bruno"));
    assert_eq!(p.body, "\u{200E}<anexado: x.webp>");
    assert_eq!(format!("{}{}: {}", p.prefix, p.author.unwrap(), p.body), line);
    assert_eq!(fmt.split_line("continuação"), None);

    let (fmt, _) = parse(ANDROID);
    let p = fmt
        .split_line("12/01/2023 14:30 - As mensagens são protegidas")
        .unwrap();
    assert_eq!(
        (p.prefix, p.author, p.body),
        ("12/01/2023 14:30 - ", None, "As mensagens são protegidas")
    );
}
