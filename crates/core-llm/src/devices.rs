//! GPUs que o `llama-server` enxerga (`--list-devices`), com a memória livre de cada uma.

use crate::LlmError;
use crate::server::hide_console;
use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// Identificador do backend, ex.: `Vulkan0`.
    pub id: String,
    /// Nome da placa, ex.: `AMD Radeon RX 7600`.
    pub name: String,
    pub total_mib: u64,
    pub free_mib: u64,
}

/// Roda `llama-server --list-devices` (inicializa o Vulkan, sem carregar modelo; ~1 s).
pub fn list_devices(bin: &Path) -> Result<Vec<Device>, LlmError> {
    if !bin.exists() {
        return Err(LlmError::NotFound(bin.to_path_buf()));
    }
    let mut cmd = Command::new(bin);
    cmd.arg("--list-devices").stdin(Stdio::null());
    hide_console(&mut cmd);
    let out = cmd.output().map_err(LlmError::Spawn)?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(parse_devices(&text))
}

/// Linhas como `  Vulkan0: AMD Radeon RX 7600 (8176 MiB, 7362 MiB free)`.
pub(crate) fn parse_devices(text: &str) -> Vec<Device> {
    text.lines()
        .filter_map(|line| {
            let (id, rest) = line.trim().split_once(": ")?;
            if id.contains(' ') {
                return None;
            }
            let (name, mem) = rest.rsplit_once(" (")?;
            let (total, free) = mem.strip_suffix(" MiB free)")?.split_once(" MiB, ")?;
            Some(Device {
                id: id.to_string(),
                name: name.trim().to_string(),
                total_mib: total.trim().parse().ok()?,
                free_mib: free.trim().parse().ok()?,
            })
        })
        .collect()
}
