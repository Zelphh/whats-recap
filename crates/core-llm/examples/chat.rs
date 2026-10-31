//! O chat completo (`core_chat::ask`) com o modelo, como no app: regras fixas primeiro e o LLM só
//! para o que elas não entendem. **Carrega o modelo na GPU** (na primeira pergunta que precisar).
//!
//! ```text
//! cargo run --release -p core-llm --example chat -- conversa.db "a gente conversa mais de semana ou no fim de semana?"
//! ```
//!
//! Usa os mesmos caminhos do example `ferramenta` (`WHATSRECAP_LLAMA_SERVER`, `WHATSRECAP_MODEL`).

use core_llm::{Server, ServerConfig};
use serde_json::Value;
use std::cell::OnceCell;
use std::path::PathBuf;
use std::time::Instant;

/// Sobe o servidor só na primeira chamada, como o `AppLlm` do `src-tauri`.
struct LazyLlm {
    cfg: ServerConfig,
    server: OnceCell<Server>,
}

impl core_chat::Llm for LazyLlm {
    fn complete(&self, system: &str, user: &str, schema: &Value, max_tokens: u32) -> Result<Value, String> {
        if self.server.get().is_none() {
            let t = Instant::now();
            let server = Server::start(&self.cfg).map_err(|e| e.to_string())?;
            println!("  (modelo carregado em {:.1?})", t.elapsed());
            let _ = self.server.set(server);
        }
        let client = self.server.get().unwrap().client();
        client
            .chat::<Value>(system, user, schema, max_tokens)
            .map(|out| out.value)
            .map_err(|e| e.to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().ok_or("uso: chat <conversa.db> <pergunta>...")?;
    let db = core_storage::Db::open(db_path)?;
    let mut cfg = ServerConfig::new(
        path("WHATSRECAP_LLAMA_SERVER", LLAMA_SERVER),
        path("WHATSRECAP_MODEL", MODEL),
    );
    cfg.log = Some(std::env::temp_dir().join("whatsrecap-llama-server.log"));
    let llm = LazyLlm {
        cfg,
        server: OnceCell::new(),
    };
    for question in args {
        let t = Instant::now();
        let a = core_chat::ask(&db, &question, Some(&llm))?;
        println!("? {question}");
        let by = a.interpreted_by.map(|b| format!("{b:?}")).unwrap_or_default();
        match &a.tool {
            Some(call) => println!("  [{:?} · {by}] {}", a.route, serde_json::to_string(call)?),
            None => println!("  [{:?}]", a.route),
        }
        println!("  {} ({:.2?})\n", a.text, t.elapsed());
    }
    Ok(())
}

const LLAMA_SERVER: &str = if cfg!(windows) {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server.exe"
} else {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server"
};
const MODEL: &str = "../whatsrecap-evals/models/Qwen3.5-9B-IQ4_XS.gguf";

/// Variável de ambiente ou caminho relativo à raiz do workspace.
fn path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(default))
}
