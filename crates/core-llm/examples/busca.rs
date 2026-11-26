//! Mede a busca do chat tipo 2 (só a busca, sem o modelo de chat): recall@k e MRR das perguntas
//! do `rag.json` do evals numa conversa em que as janelas dele foram injetadas, com o e5-small
//! **na CPU** e os trechos do app. Compara busca só vetorial, com filtro de datas e híbrida (FTS),
//! e formas de trecho.
//!
//! ```text
//! cargo run --release -p core-llm --example busca -- conversa-rag.db ../whatsrecap-evals/datasets/rag.json [12,2,1000 8,2,700 ...]
//! ```
//!
//! Cada forma de trecho diferente da do banco é indexada numa cópia (`<banco>.t<m>-<o>-<c>.db`, ao
//! lado do original), reaproveitada nas próximas execuções. `WHATSRECAP_LLAMA_SERVER` e
//! `WHATSRECAP_EMBEDDER` como no example `conteudo`.

use core_chat::rag::{self, ChunkParams, SearchOptions};
use core_llm::{E5_SMALL, Server, ServerConfig};
use core_storage::Db;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

struct Embedder(Server);

impl rag::Embedder for Embedder {
    fn id(&self) -> &str {
        E5_SMALL.id
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.0.client().embed(texts).map_err(|e| e.to_string())
    }
}

/// Uma pergunta respondível com os ids (no banco) das mensagens que a sustentam.
struct Question {
    id: String,
    q: String,
    gold: Vec<i64>,
}

const KS: [usize; 5] = [1, 3, 5, 8, 10];
/// Até onde procurar o primeiro trecho certo.
const DEPTH: usize = 50;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [db_path, rag_path, shapes @ ..] = args.as_slice() else {
        return Err("uso: busca <conversa.db> <rag.json> [mensagens,sobreposição,caracteres]...".into());
    };
    let shapes: Vec<ChunkParams> = if shapes.is_empty() {
        vec![rag::CHUNKS]
    } else {
        shapes.iter().map(|s| parse_shape(s)).collect::<Result<_, _>>()?
    };
    let rag_json: Value = serde_json::from_str(&std::fs::read_to_string(rag_path)?)?;
    let questions = gold_questions(&Db::open(db_path)?, &rag_json)?;
    println!("{} perguntas respondíveis\n", questions.len());

    let bin = path("WHATSRECAP_LLAMA_SERVER", LLAMA_SERVER);
    let mut cfg = ServerConfig::new(&bin, path("WHATSRECAP_EMBEDDER", EMBEDDER));
    cfg.args = E5_SMALL.args.iter().map(|s| s.to_string()).collect();
    cfg.parallel = 2;
    cfg.log = Some(std::env::temp_dir().join("whatsrecap-llama-embed.log"));
    let embedder = Embedder(Server::start(&cfg)?);
    let today = chrono::Local::now().date_naive();

    for shape in shapes {
        let file = db_for(Path::new(db_path), shape)?;
        let mut db = Db::open(&file)?;
        let t = Instant::now();
        let mut last = Instant::now();
        let st = rag::index_with(
            &mut db,
            &embedder,
            shape,
            &AtomicBool::new(false),
            &mut |done, total| {
                if done == total || last.elapsed().as_secs() >= 15 {
                    last = Instant::now();
                    eprintln!("  indexados {done} de {total} trechos ({:.0?})", t.elapsed());
                }
            },
        )?;
        let conv = core_chat::Conversation::open(&db)?;
        println!(
            "## trechos de até {} mensagens / {} caracteres, sobreposição {} ({} trechos)",
            shape.messages, shape.chars, shape.overlap, st.total
        );
        let variants: [(&str, bool, bool); 4] = [
            ("vetorial", false, false),
            ("+ datas", true, false),
            ("híbrida", false, true),
            ("híbrida + datas", true, true),
        ];
        let mut ranks: HashMap<&str, Vec<Option<usize>>> = HashMap::new();
        for q in &questions {
            for (name, dates, hybrid) in variants {
                let opt = SearchOptions {
                    k: DEPTH,
                    range: if dates { rag::question_range(&q.q, today) } else { None },
                    keywords: if hybrid { rag::keywords(&conv, &q.q) } else { Vec::new() },
                };
                let hits = rag::search_with(&db, &embedder, &q.q, &opt)?;
                let rank = hits
                    .iter()
                    .position(|h| q.gold.iter().any(|id| (h.chunk.start_id..=h.chunk.end_id).contains(id)));
                ranks.entry(name).or_default().push(rank);
            }
        }
        println!("| busca | {} | MRR |", KS.map(|k| format!("@{k}")).join(" | "));
        println!("|---|{}---|", "---|".repeat(KS.len()));
        for (name, _, _) in variants {
            let r = &ranks[name];
            let recall = KS.map(|k| {
                let n = r.iter().filter(|x| x.is_some_and(|p| p < k)).count();
                format!("{n}/{}", r.len())
            });
            let mrr = r.iter().map(|x| x.map_or(0.0, |p| 1.0 / (p as f64 + 1.0))).sum::<f64>() / r.len() as f64;
            println!("| {name} | {} | {mrr:.2} |", recall.join(" | "));
        }
        println!("\nposição do primeiro trecho certo (– = fora dos {DEPTH}):");
        for (i, q) in questions.iter().enumerate() {
            let cols: Vec<String> = variants
                .iter()
                .map(|(name, _, _)| ranks[name][i].map_or("–".into(), |p| (p + 1).to_string()))
                .collect();
            println!(
                "  {} {:>3} {:>3} {:>3} {:>3}  {}",
                q.id, cols[0], cols[1], cols[2], cols[3], q.q
            );
        }
        println!();
    }
    Ok(())
}

fn parse_shape(s: &str) -> Result<ChunkParams, String> {
    let n: Vec<usize> = s
        .split(',')
        .map(|x| x.trim().parse().map_err(|_| format!("forma de trecho inválida: {s}")))
        .collect::<Result<_, _>>()?;
    match n.as_slice() {
        [messages, overlap, chars] => Ok(ChunkParams {
            messages: *messages,
            overlap: *overlap,
            chars: *chars,
        }),
        _ => Err(format!(
            "forma de trecho inválida (mensagens,sobreposição,caracteres): {s}"
        )),
    }
}

/// O banco original para a forma do app; uma cópia para as outras.
fn db_for(original: &Path, shape: ChunkParams) -> std::io::Result<PathBuf> {
    if shape == rag::CHUNKS {
        return Ok(original.to_path_buf());
    }
    let stem = original.file_stem().unwrap_or_default().to_string_lossy();
    let copy = original.with_file_name(format!(
        "{stem}.t{}-{}-{}.db",
        shape.messages, shape.overlap, shape.chars
    ));
    if !copy.exists() {
        std::fs::copy(original, &copy)?;
    }
    Ok(copy)
}

/// Perguntas respondíveis com os `gold_ids` do evals traduzidos para os ids do banco (mesma data e
/// mesmo texto).
fn gold_questions(db: &Db, rag_json: &Value) -> Result<Vec<Question>, Box<dyn std::error::Error>> {
    let mut ids: HashMap<i64, i64> = HashMap::new();
    for w in rag_json["windows"].as_array().ok_or("rag.json sem windows")? {
        let date = chrono::NaiveDate::parse_from_str(w["date"].as_str().unwrap_or_default(), "%Y-%m-%d")?;
        let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let first = db.first_id_at_or_after(start)?.ok_or("data fora da conversa")?;
        let day: Vec<_> = db
            .messages_from(first, 5000)?
            .into_iter()
            .take_while(|m| m.ts < start + 86_400)
            .collect();
        for m in w["messages"].as_array().into_iter().flatten() {
            let (rid, text) = (m[0].as_i64().unwrap_or_default(), m[2].as_str().unwrap_or_default());
            let found = day
                .iter()
                .find(|r| r.text.as_deref() == Some(text))
                .ok_or_else(|| format!("mensagem {rid} ({text}) não está no banco"))?;
            ids.insert(rid, found.id);
        }
    }
    let mut out = Vec::new();
    for q in rag_json["questions"].as_array().ok_or("rag.json sem questions")? {
        if q["answerable"] != true {
            continue;
        }
        let gold = q["gold_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| ids.get(&v.as_i64()?).copied())
            .collect();
        out.push(Question {
            id: q["id"].as_str().unwrap_or_default().to_string(),
            q: q["q"].as_str().unwrap_or_default().to_string(),
            gold,
        });
    }
    Ok(out)
}

const LLAMA_SERVER: &str = if cfg!(windows) {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server.exe"
} else {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server"
};
const EMBEDDER: &str = "../whatsrecap-evals/models/multilingual-e5-small-q8_0.gguf";

/// Variável de ambiente ou caminho relativo à raiz do workspace.
fn path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(default))
}
