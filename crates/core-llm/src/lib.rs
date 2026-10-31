//! Acesso ao modelo de linguagem local (fase 5): sobe o `llama-server` (llama.cpp) como processo
//! filho e conversa com ele pela API compatível com OpenAI, sempre com saída restrita por JSON
//! schema.
//!
//! O crate não conhece o domínio do chat: recebe prompts e schemas prontos (ver
//! `core_chat::llm`) e devolve a saída desserializada no tipo pedido.
//!
//! ```no_run
//! # use core_llm::{Server, ServerConfig};
//! # #[derive(serde::Deserialize)] struct Out { r#type: String }
//! # let schema = serde_json::json!({});
//! let server = Server::start(&ServerConfig::new("llama-server", "modelo.gguf"))?;
//! let out = server.client().chat::<Out>("prompt de sistema", "pergunta", &schema, 120)?;
//! println!("{} em {:?}", out.value.r#type, out.latency);
//! # Ok::<(), core_llm::LlmError>(())
//! ```

mod client;
mod server;

pub use client::{Client, Completion, Timings};
pub use server::{RECOMMENDED_ARGS, Server, ServerConfig};

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("arquivo não encontrado: {}", .0.display())]
    NotFound(PathBuf),
    #[error("não foi possível iniciar o llama-server: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("o llama-server encerrou ao carregar o modelo ({status}){log}")]
    Exited { status: String, log: String },
    #[error("o llama-server não ficou pronto em {0:?}")]
    Timeout(Duration),
    #[error("falha na comunicação com o llama-server: {0}")]
    Http(String),
    #[error("a resposta foi cortada pelo limite de {0} tokens")]
    Truncated(u32),
    #[error("resposta fora do formato esperado ({error}): {content}")]
    InvalidOutput { error: String, content: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<ureq::Error> for LlmError {
    fn from(e: ureq::Error) -> Self {
        LlmError::Http(e.to_string())
    }
}

#[cfg(test)]
mod tests;
