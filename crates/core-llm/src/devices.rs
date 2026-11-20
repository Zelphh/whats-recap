//! GPUs que o `llama-server` enxerga (`--list-devices`), com a memória livre de cada uma.
//!
//! O Vulkan só desconta a memória do próprio processo: um jogo ou outro modelo ocupando a placa
//! não aparece. No Windows, a memória livre é corrigida pelo uso de todos os processos (contador
//! "GPU Adapter Memory"); em outros sistemas, fica a do Vulkan.

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
    /// Livre para todos os processos: o menor entre o que o Vulkan informa e o total menos o uso
    /// dedicado de todos os processos (no Windows).
    pub free_mib: u64,
}

/// Uso dedicado de VRAM de um adaptador, somando todos os processos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdapterUsage {
    pub name: String,
    pub used_mib: u64,
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
    let mut devices = parse_devices(&text);
    #[cfg(windows)]
    apply_usage(&mut devices, &crate::vram_windows::adapter_usage());
    Ok(devices)
}

/// Desconta da memória livre de cada GPU o uso de todos os processos, casando pelo nome da placa.
/// Com mais de uma placa de mesmo nome, vale o maior uso entre elas (na dúvida, cabe menos).
pub(crate) fn apply_usage(devices: &mut [Device], usage: &[AdapterUsage]) {
    for d in devices {
        let used = usage
            .iter()
            .filter(|u| same_name(&u.name, &d.name))
            .map(|u| u.used_mib)
            .max();
        if let Some(used) = used {
            d.free_mib = d.free_mib.min(d.total_mib.saturating_sub(used));
        }
    }
}

fn same_name(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    norm(a) == norm(b)
}

/// Instância do contador, `luid_0x00000000_0x05F768D9_phys_0` → `0x0000000005F768D9`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_luid(instance: &str) -> Option<u64> {
    let mut parts = instance.strip_prefix("luid_")?.split('_');
    let high = u32::from_str_radix(parts.next()?.strip_prefix("0x")?, 16).ok()?;
    let low = u32::from_str_radix(parts.next()?.strip_prefix("0x")?, 16).ok()?;
    Some(((high as u64) << 32) | low as u64)
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
