//! Perguntas sobre o conteúdo (chat tipo 2) com os modelos de verdade, como no app: indexa a
//! conversa com o e5-small **na CPU** (ou continua a indexação) e responde com o Qwen3.5 **na GPU**
//! (o modelo de chat só carrega na primeira pergunta que precisar dele).
//!
//! ```text
//! cargo run --release -p core-llm --example conteudo -- conversa.db "o que ela disse sobre o emprego novo?"
//! ```
//!
//! Sem perguntas, só indexa. Cada pergunta vai com a resposta anterior, como no app, para testar
//! continuações ("e depois?"). Caminhos como no example `chat` (`WHATSRECAP_LLAMA_SERVER`,
//! `WHATSRECAP_MODEL`) e `WHATSRECAP_EMBEDDER` para o `.gguf` do e5.

use core_llm::{E5_SMALL, Server, ServerConfig};
use serde_json::Value;
use std::cell::OnceCell;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

struct LazyLlm {
    cfg: ServerConfig,
    server: OnceCell<Server>,
}

impl core_chat::Llm for LazyLlm {
    fn complete(&self, system: &str, user: &str, schema: &Value, max_tokens: u32) -> Result<Value, String> {
        if self.server.get().is_none() {
            let t = Instant::now();
            let server = Server::start(&self.cfg).map_err(|e| e.to_string())?;
            println!("  (modelo de chat carregado em {:.1?})", t.elapsed());
            let _ = self.server.set(server);
        }
        let client = self.server.get().unwrap().client();
        client
            .chat::<Value>(system, user, schema, max_tokens)
            .map(|out| out.value)
            .map_err(|e| e.to_string())
    }
}

struct Embedder(Server);

impl core_chat::rag::Embedder for Embedder {
    fn id(&self) -> &str {
        E5_SMALL.id
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.0.client().embed(texts).map_err(|e| e.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().ok_or("uso: conteudo <conversa.db> [pergunta]...")?;
    let mut db = core_storage::Db::open(db_path)?;
    let bin = path("WHATSRECAP_LLAMA_SERVER", LLAMA_SERVER);

    let mut ecfg = ServerConfig::new(&bin, path("WHATSRECAP_EMBEDDER", EMBEDDER));
    ecfg.args = E5_SMALL.args.iter().map(|s| s.to_string()).collect();
    ecfg.parallel = 2;
    ecfg.log = Some(std::env::temp_dir().join("whatsrecap-llama-embed.log"));
    let t = Instant::now();
    let embedder = Embedder(Server::start(&ecfg)?);
    println!("embedder carregado em {:.1?}", t.elapsed());

    let t = Instant::now();
    let mut last = Instant::now();
    let st = core_chat::rag::index(&mut db, &embedder, &AtomicBool::new(false), &mut |done, total| {
        if done == total || last.elapsed().as_secs() >= 5 {
            last = Instant::now();
            println!("  indexados {done} de {total} trechos ({:.0?})", t.elapsed());
        }
    })?;
    println!(
        "índice: {} trechos, pronto = {} ({:.1?}; {} mensagens)\n",
        st.total,
        st.ready,
        t.elapsed(),
        db.message_count()?
    );

    let mut cfg = ServerConfig::new(&bin, path("WHATSRECAP_MODEL", MODEL));
    cfg.log = Some(std::env::temp_dir().join("whatsrecap-llama-server.log"));
    let llm = LazyLlm {
        cfg,
        server: OnceCell::new(),
    };
    let today = chrono::Local::now().date_naive();
    let mut previous: Option<core_chat::PreviousTurn> = None;
    for question in args {
        let t = Instant::now();
        let conv = core_chat::Conversation::open(&db)?;
        let opt = core_chat::rag::options_for(&conv, &question, today);
        let hits = core_chat::rag::search_with(&db, &embedder, &question, &opt)?;
        let search = t.elapsed();
        let a = core_chat::ask_after(&db, &question, previous.as_ref(), Some(&llm), Some(&embedder), today)?;
        previous = Some(core_chat::PreviousTurn {
            question: question.clone(),
            route: a.route,
            answer: a.text.clone(),
            citations: a.citations.iter().map(|m| m.id).collect(),
        });
        println!("? {question}{}", if a.followup { "  (continuação)" } else { "" });
        println!(
            "  busca ({search:.0?}): {}",
            hits.iter()
                .map(|h| format!("#{}–{} ({:.2})", h.chunk.start_id, h.chunk.end_id, h.score))
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("  [{:?}] {} ({:.2?})", a.route, a.text, t.elapsed());
        for m in &a.citations {
            println!(
                "    #{} {}: {}",
                m.id,
                m.author.as_deref().unwrap_or_default(),
                m.text.as_deref().unwrap_or_default()
            );
        }
        println!();
    }
    Ok(())
}

const LLAMA_SERVER: &str = if cfg!(windows) {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server.exe"
} else {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server"
};
const MODEL: &str = "../whatsrecap-evals/models/Qwen3.5-9B-IQ4_XS.gguf";
const EMBEDDER: &str = "../whatsrecap-evals/models/multilingual-e5-small-q8_0.gguf";

/// Variável de ambiente ou caminho relativo à raiz do workspace.
fn path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(default))
}
