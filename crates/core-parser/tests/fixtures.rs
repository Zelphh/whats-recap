//! Snapshot de cada fixture em `tests/fixtures/`: formato detectado, autores, contagem por tipo
//! e as primeiras mensagens. Um `.txt` sem `.expected.json` ao lado faz o teste falhar e gera o
//! snapshot; revise-o e rode de novo. Para regenerar todos: `UPDATE_FIXTURES=1 cargo test -p core-parser`.

use core_parser::{MessageReader, detect_format};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const SAMPLE: usize = 25;

fn snapshot(path: &Path) -> Value {
    let format = detect_format(BufReader::new(File::open(path).unwrap())).unwrap();
    let mut reader = MessageReader::new(BufReader::new(File::open(path).unwrap()), format.clone());
    let mut kinds: BTreeMap<&str, u64> = BTreeMap::new();
    let mut sample = Vec::new();
    let (mut first, mut last) = (None, None);
    for m in reader.by_ref() {
        let m = m.unwrap();
        *kinds.entry(m.kind.as_str()).or_default() += 1;
        first.get_or_insert(m.ts);
        last = Some(m.ts);
        if sample.len() < SAMPLE {
            sample.push(json!({
                "ts": m.ts.to_string(),
                "author": m.author,
                "kind": m.kind.as_str(),
                "text": m.text,
                "mediaFile": m.media_file,
            }));
        }
    }
    let stats = reader.stats();
    json!({
        "platform": format.platform,
        "dateOrder": format.order,
        "authors": format.authors,
        "messages": stats.messages,
        "kinds": kinds,
        "orphanLines": stats.orphan_lines,
        "invalidDates": stats.invalid_dates,
        "first": first.map(|t| t.to_string()),
        "last": last.map(|t| t.to_string()),
        "sample": sample,
    })
}

#[test]
fn fixtures_match_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let update = std::env::var_os("UPDATE_FIXTURES").is_some();
    let mut failures = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "nenhuma fixture em {}", dir.display());

    for path in paths {
        let expected_path = path.with_extension("expected.json");
        let actual = snapshot(&path);
        let pretty = serde_json::to_string_pretty(&actual).unwrap() + "\n";
        match std::fs::read_to_string(&expected_path) {
            Ok(_) if update => std::fs::write(&expected_path, pretty).unwrap(),
            Ok(expected) => {
                let expected: Value = serde_json::from_str(&expected).unwrap();
                if expected != actual {
                    failures.push(format!(
                        "{} difere do snapshot.\n--- esperado\n{}\n--- obtido\n{pretty}",
                        path.display(),
                        serde_json::to_string_pretty(&expected).unwrap()
                    ));
                }
            }
            Err(_) => {
                std::fs::write(&expected_path, pretty).unwrap();
                if !update {
                    failures.push(format!(
                        "snapshot criado para {}: revise e rode de novo",
                        path.display()
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
