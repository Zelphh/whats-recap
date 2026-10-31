//! Sobe o `llama-server` com o modelo e escolhe a ferramenta para cada pergunta, como o chat fará
//! quando a heurística não entender a pergunta. **Carrega o modelo na GPU.**
//!
//! ```text
//! cargo run --release -p core-llm --example ferramenta -- "quantas vezes ela me chamou de chato ano passado?"
//! ```
//!
//! Por padrão usa o `llama-server` e o Qwen3.5 9B do `whatsrecap-evals` (ao lado deste repositório);
//! `WHATSRECAP_LLAMA_SERVER` e `WHATSRECAP_MODEL` apontam para outros arquivos.

use core_chat::{ToolCall, llm};
use core_llm::{Server, ServerConfig};
use std::path::PathBuf;
use std::time::Instant;

// Conversa fictícia (a mesma do dataset do evals).
const AUTHORS: [&str; 2] = ["Ana Souza", "Bruno"];
const FIRST: &str = "2021-03-14";
const LAST: &str = "2026-02-27";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let questions: Vec<String> = std::env::args().skip(1).collect();
    if questions.is_empty() {
        return Err("uso: ferramenta <pergunta>...".into());
    }
    let mut cfg = ServerConfig::new(
        path("WHATSRECAP_LLAMA_SERVER", LLAMA_SERVER),
        path("WHATSRECAP_MODEL", MODEL),
    );
    cfg.log = Some(std::env::temp_dir().join("whatsrecap-llama-server.log"));
    println!("carregando {} …", cfg.model.display());
    let t = Instant::now();
    let server = Server::start(&cfg)?;
    println!(
        "pronto em {:.1?} (log: {})\n",
        t.elapsed(),
        cfg.log.as_ref().unwrap().display()
    );

    let authors: Vec<String> = AUTHORS.iter().map(|a| a.to_string()).collect();
    // "Hoje" é o fim da conversa fictícia, para "ano passado" dar sempre o mesmo intervalo.
    let system = llm::tool_selection_prompt(&authors, FIRST, LAST, LAST);
    let schema = llm::tool_schema(&authors);
    let date = |s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    for q in questions {
        let out = server.client().chat::<serde_json::Value>(&system, &q, &schema, 120)?;
        let shown = if out.value["tool"] == llm::NO_TOOL {
            "nenhuma ferramenta responde".to_string()
        } else {
            let call: ToolCall = serde_json::from_value(out.value)?;
            serde_json::to_string(&call.without_redundant_range(date(FIRST), date(LAST)))?
        };
        println!("? {q}\n  {shown}");
        println!(
            "  {:.2?} · {} tokens de prompt, {} gerados\n",
            out.latency, out.timings.prompt_tokens, out.timings.generated_tokens
        );
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
