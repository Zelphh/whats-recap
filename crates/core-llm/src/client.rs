//! Cliente HTTP do `llama-server` (`/v1/chat/completions` com `response_format: json_schema`).

use crate::LlmError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use ureq::Agent;

/// Respostas determinísticas: a mesma pergunta dá a mesma interpretação.
const TEMPERATURE: f64 = 0.0;
const SEED: u64 = 42;
/// Limite de uma chamada; uma janela de 8k tokens leva ~15 s na RX 7600.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct Client {
    url: String,
    agent: Agent,
}

/// Saída de uma chamada, já desserializada.
#[derive(Debug, Clone)]
pub struct Completion<T> {
    pub value: T,
    /// Texto bruto gerado pelo modelo (JSON).
    pub content: String,
    pub latency: Duration,
    pub timings: Timings,
}

/// Tempos que o `llama-server` reporta em cada resposta.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Timings {
    #[serde(rename = "prompt_n", default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub prompt_ms: f64,
    #[serde(rename = "predicted_n", default)]
    pub generated_tokens: u64,
    #[serde(rename = "predicted_ms", default)]
    pub generated_ms: f64,
}

pub(crate) fn agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .build()
        .into()
}

impl Client {
    /// `url` é a base do servidor, ex.: `http://127.0.0.1:8089`.
    pub fn new(url: &str) -> Self {
        Client {
            url: url.trim_end_matches('/').to_string(),
            agent: agent(REQUEST_TIMEOUT),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// O servidor terminou de carregar o modelo (`/health` responde 200; durante a carga, 503).
    pub fn is_ready(&self) -> bool {
        agent(Duration::from_secs(2))
            .get(format!("{}/health", self.url))
            .call()
            .is_ok_and(|r| r.status().is_success())
    }

    /// Uma chamada com prompt de sistema, pergunta e saída restrita por `schema`, desserializada
    /// em `T`. Falha se a resposta for cortada por `max_tokens` ou não desserializar.
    pub fn chat<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        max_tokens: u32,
    ) -> Result<Completion<T>, LlmError> {
        let body = json!({
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
            "temperature": TEMPERATURE,
            "seed": SEED,
            "max_tokens": max_tokens,
            "response_format": { "type": "json_schema", "json_schema": { "name": "saida", "schema": schema } },
        });
        let t0 = Instant::now();
        let mut resp = self
            .agent
            .post(format!("{}/v1/chat/completions", self.url))
            .send_json(&body)?;
        let status = resp.status();
        let text = resp.body_mut().read_to_string()?;
        let latency = t0.elapsed();
        if !status.is_success() {
            return Err(LlmError::Http(format!("HTTP {status}: {}", shorten(&text))));
        }
        let resp: Value =
            serde_json::from_str(&text).map_err(|e| LlmError::Http(format!("resposta não é JSON: {e}")))?;
        let choice = &resp["choices"][0];
        let content = choice["message"]["content"].as_str().unwrap_or_default().to_string();
        if choice["finish_reason"] == "length" {
            return Err(LlmError::Truncated(max_tokens));
        }
        let value = serde_json::from_str(&content).map_err(|e| LlmError::InvalidOutput {
            error: e.to_string(),
            content: shorten(&content),
        })?;
        Ok(Completion {
            value,
            content,
            latency,
            timings: serde_json::from_value(resp["timings"].clone()).unwrap_or_default(),
        })
    }
}

fn shorten(s: &str) -> String {
    const MAX: usize = 500;
    match s.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}
