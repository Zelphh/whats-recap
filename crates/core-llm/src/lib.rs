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
mod devices;
mod download;
mod models;
mod server;
#[cfg(windows)]
mod vram_windows;

pub use client::{Client, Completion, Timings};
pub use devices::{Device, list_devices};
pub use download::{DownloadPhase, DownloadSpec, download, partial_path};
pub use models::{
    Backend, CPU_ARGS, Choice, E5_PASSAGE, E5_QUERY, E5_SMALL, GPU_ARGS, MODELS, ModelSpec, QWEN35_4B, QWEN35_9B,
    model, plan,
};
pub use server::{CHAT_SLOTS, RECOMMENDED_ARGS, Server, ServerConfig};

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
    #[error("download cancelado")]
    Cancelled,
    #[error("o arquivo baixado não confere (SHA-256 {got}, esperado {expected}); baixe de novo")]
    Checksum { expected: String, got: String },
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
