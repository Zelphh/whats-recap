//! Lista as GPUs como o app as vê para escolher entre GPU e CPU: a memória livre já desconta o uso
//! de todos os processos (no Windows). Não carrega modelo.
//!
//! ```text
//! cargo run -p core-llm --example dispositivos
//! ```
//!
//! Por padrão usa o `llama-server` do `whatsrecap-evals`; `WHATSRECAP_LLAMA_SERVER` aponta para outro.

use std::path::PathBuf;

const LLAMA_SERVER: &str = if cfg!(windows) {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server.exe"
} else {
    "../whatsrecap-evals/vendor/llama.cpp/current/llama-server"
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bin = std::env::var_os("WHATSRECAP_LLAMA_SERVER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(LLAMA_SERVER)
        });
    let devices = core_llm::list_devices(&bin)?;
    if devices.is_empty() {
        println!("nenhuma GPU");
    }
    for d in &devices {
        let fits: Vec<_> = core_llm::MODELS
            .iter()
            .filter(|m| d.free_mib >= m.min_vram_mib)
            .map(|m| m.name)
            .collect();
        println!(
            "{}: {} — {} MiB livres de {} (cabe: {})",
            d.id,
            d.name,
            d.free_mib,
            d.total_mib,
            fits.join(", ")
        );
    }
    Ok(())
}
