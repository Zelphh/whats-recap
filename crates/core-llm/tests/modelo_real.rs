//! Teste com o modelo de verdade. **Carrega o modelo na GPU**, por isso fica fora do `cargo test`:
//!
//! ```text
//! cargo test --release -p core-llm -- --ignored
//! ```
//!
//! Usa o `llama-server` e o Qwen3.5 9B do `whatsrecap-evals` (ao lado deste repositório), ou
//! `WHATSRECAP_LLAMA_SERVER` e `WHATSRECAP_MODEL`.

use core_chat::{ToolCall, llm};
use core_llm::{Server, ServerConfig};
use std::path::PathBuf;

fn path(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(default))
}

#[test]
#[ignore = "carrega o modelo na GPU"]
fn chooses_the_tool_with_the_real_model() {
    let bin = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let cfg = ServerConfig::new(
        path(
            "WHATSRECAP_LLAMA_SERVER",
            &format!("../whatsrecap-evals/vendor/llama.cpp/current/{bin}"),
        ),
        path("WHATSRECAP_MODEL", "../whatsrecap-evals/models/Qwen3.5-9B-IQ4_XS.gguf"),
    );
    let server = Server::start(&cfg).unwrap();

    let authors = vec!["Ana Souza".to_string(), "Bruno".to_string()];
    let system = llm::tool_selection_prompt(&authors, None, "2021-03-14", "2026-02-27", "2026-02-27");
    let out = server
        .client()
        .chat::<ToolCall>(
            &system,
            "Quantas vezes o Bruno falou 'amor'?",
            &llm::tool_schema(&authors),
            120,
        )
        .unwrap();

    assert_eq!(out.value.name(), "count_word");
    assert_eq!(out.value.author(), Some("Bruno"));
    let (tool, args) = (out.content.find(r#""tool""#), out.content.find(r#""args""#));
    assert!(tool < args, "\"tool\" deve vir antes de \"args\": {}", out.content);
}
