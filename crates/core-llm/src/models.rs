//! Modelos que o app sabe baixar e rodar, com as flags de cada um, e a escolha entre GPU e CPU.
//!
//! Números do `whatsrecap-evals` (`COMPARACAO-MODELOS.md`) na RX 7600 8 GB (Vulkan, Windows).

use serde::Serialize;

/// Um modelo GGUF do catálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: &'static str,
    /// Nome para a interface.
    pub name: &'static str,
    /// Nome do arquivo em disco.
    pub file: &'static str,
    #[serde(skip)]
    pub url: &'static str,
    /// Tamanho exato em bytes (o download confere).
    pub size: u64,
    /// SHA-256 do arquivo (o LFS do Hugging Face).
    #[serde(skip)]
    pub sha256: &'static str,
    /// Flags do modelo, na GPU e na CPU. O flash attention depende do modelo: no Vulkan do
    /// Windows, `-fa on` deixa o Qwen3.5 2–2,5× mais lento no `llama-server`, enquanto o Gemma 4
    /// fica mais rápido com ele (geração de 19 para 32 tok/s).
    #[serde(skip)]
    pub args: &'static [&'static str],
    /// VRAM livre (MiB) para rodar com todas as camadas na GPU e o contexto de [`GPU_ARGS`]: o pico
    /// medido sem o desktop, com uma margem.
    pub min_vram_mib: u64,
}

/// O modelo escolhido (2026-10-29): Qwen3.5 9B `IQ4_XS`, sem flash attention. Pico de ~5,7 GB de
/// VRAM com 16k de contexto.
pub const QWEN35_9B: ModelSpec = ModelSpec {
    id: "qwen3.5-9b-iq4xs",
    name: "Qwen3.5 9B",
    file: "Qwen3.5-9B-IQ4_XS.gguf",
    url: "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-IQ4_XS.gguf",
    size: 5_168_653_536,
    sha256: "7e918aeca06c52bcb528ea6b04b4ec957e75ee8c0a73138854c0dfcf371ea429",
    args: &["-fa", "off", "--reasoning", "off"],
    min_vram_mib: 6000,
};

/// Alternativa mais leve: Qwen3.5 4B `Q4_K_M` (~3 GB de VRAM), para GPUs com menos memória livre.
pub const QWEN35_4B: ModelSpec = ModelSpec {
    id: "qwen3.5-4b-q4km",
    name: "Qwen3.5 4B",
    file: "Qwen3.5-4B-Q4_K_M.gguf",
    url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
    size: 2_740_937_888,
    sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
    args: &["-fa", "off", "--reasoning", "off"],
    min_vram_mib: 3500,
};

/// Em ordem de preferência.
pub const MODELS: &[ModelSpec] = &[QWEN35_9B, QWEN35_4B];

/// Embedder das perguntas sobre o conteúdo (chat tipo 2): multilingual-e5-small `Q8_0`. Roda na
/// CPU (a GPU fica com o modelo de chat): ~2.400 tokens/s no Ryzen 7 5700X. No evals, a busca só
/// vetorial com ele achou o trecho certo em primeiro lugar em 79% das perguntas (MRR 0,84).
/// Pede os prefixos [`E5_QUERY`] e [`E5_PASSAGE`].
pub const E5_SMALL: ModelSpec = ModelSpec {
    id: "e5-small-q8",
    name: "Busca por significado (e5-small)",
    file: "multilingual-e5-small-q8_0.gguf",
    url: "https://huggingface.co/TwinSunsLLC/multilingual-e5-small-gguf/resolve/main/multilingual-e5-small-q8_0.gguf",
    size: 132_439_008,
    sha256: "e011debc1208e31bf7b6aebee2d9fc8bd2ca11694a77ed66ac9d0c9d0a877c93",
    // Contexto e lotes grandes: o e5 é bidirecional e cada texto precisa caber inteiro num lote.
    args: &[
        "--embedding",
        "--pooling",
        "mean",
        "-c",
        "8192",
        "-b",
        "8192",
        "-ub",
        "8192",
        "-ngl",
        "0",
        "-dev",
        "none",
    ],
    min_vram_mib: 0,
};

/// Prefixos do e5: a pergunta e os trechos são codificados de jeitos diferentes.
pub const E5_QUERY: &str = "query: ";
pub const E5_PASSAGE: &str = "passage: ";

/// Modelos de chat ou o embedder.
pub fn model(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().chain([&E5_SMALL]).find(|m| m.id == id)
}

/// Todas as camadas na GPU, 16k de contexto.
pub const GPU_ARGS: &[&str] = &["-ngl", "99", "-c", "16384"];
/// Só CPU: contexto menor (o prompt mais longo do chat tem ~2k tokens) e as threads padrão do
/// llama.cpp (os núcleos físicos).
pub const CPU_ARGS: &[&str] = &["-ngl", "0", "-c", "8192"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Gpu,
    Cpu,
}

/// Um modelo num backend: o que o servidor vai rodar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub model: &'static ModelSpec,
    pub backend: Backend,
}

impl Choice {
    /// Flags do `llama-server` para esta escolha. `device` (ex.: `Vulkan0`, de
    /// [`crate::list_devices`]) prende o modelo a uma GPU: sem ele, o llama.cpp divide as camadas
    /// entre todas as que achar, inclusive a integrada. Na CPU, nenhuma GPU é usada.
    pub fn args(&self, device: Option<&str>) -> Vec<String> {
        let (hw, dev) = match self.backend {
            Backend::Gpu => (GPU_ARGS, device),
            Backend::Cpu => (CPU_ARGS, Some("none")),
        };
        let dev = dev.map(|d| ["-dev", d]);
        hw.iter()
            .copied()
            .chain(dev.iter().flatten().copied())
            .chain(self.model.args.iter().copied())
            .map(String::from)
            .collect()
    }
}

/// Ordem de tentativa com os modelos baixados (`available`) e a VRAM livre da GPU (`None`: sem
/// GPU). Na GPU, os modelos que cabem, do maior para o menor; por último, o menor modelo na CPU
/// (lento, mas responde). Se um falhar ao carregar, quem chama tenta o próximo.
pub fn plan(available: &[&'static ModelSpec], gpu_free_mib: Option<u64>) -> Vec<Choice> {
    let mut out: Vec<Choice> = MODELS
        .iter()
        .filter(|m| available.contains(m))
        .filter(|m| gpu_free_mib.is_some_and(|free| free >= m.min_vram_mib))
        .map(|model| Choice {
            model,
            backend: Backend::Gpu,
        })
        .collect();
    if let Some(model) = MODELS.iter().rev().find(|m| available.contains(m)) {
        out.push(Choice {
            model,
            backend: Backend::Cpu,
        });
    }
    out
}
