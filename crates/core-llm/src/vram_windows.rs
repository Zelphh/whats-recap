//! VRAM dedicada em uso por **todos** os processos, por adaptador, no Windows.
//!
//! O Vulkan do `--list-devices` só desconta o que o próprio processo usa (com o 4B carregado por
//! outro processo, a RX 7600 continuava com 7.362 MiB livres). O contador "GPU Adapter Memory" do
//! Windows (o do Gerenciador de Tarefas) soma todos os processos; as instâncias são nomeadas pelo
//! LUID do adaptador, e o DXGI liga o LUID ao nome da placa.

use crate::devices::{AdapterUsage, parse_luid};
use std::collections::HashMap;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

/// Adaptadores físicos com o uso dedicado de todos os processos. Vazio se o contador ou o DXGI
/// falharem (quem chama fica com o número do Vulkan).
pub(crate) fn adapter_usage() -> Vec<AdapterUsage> {
    let used = dedicated_usage_by_luid();
    if used.is_empty() {
        return vec![];
    }
    adapters()
        .into_iter()
        .filter_map(|(name, luid)| {
            Some(AdapterUsage {
                name,
                used_mib: used.get(&luid)? / 1_048_576,
            })
        })
        .collect()
}

/// `(nome, LUID)` dos adaptadores de hardware.
fn adapters() -> Vec<(String, u64)> {
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return vec![];
    };
    (0..)
        .map_while(|i| unsafe { factory.EnumAdapters1(i) }.ok())
        .filter_map(|adapter| unsafe { adapter.GetDesc1() }.ok())
        .filter(|d| d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0)
        .map(|d| {
            let len = d
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(d.Description.len());
            let name = String::from_utf16_lossy(&d.Description[..len]).trim().to_string();
            let luid = ((d.AdapterLuid.HighPart as u32 as u64) << 32) | d.AdapterLuid.LowPart as u64;
            (name, luid)
        })
        .collect()
}

/// Bytes dedicados em uso por LUID, somando as instâncias (`luid_0x…_0x…_phys_N`) de cada um.
fn dedicated_usage_by_luid() -> HashMap<u64, u64> {
    let mut out = HashMap::new();
    let mut query = PDH_HQUERY::default();
    if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
        return out;
    }
    let mut counter = PDH_HCOUNTER::default();
    // O nome em inglês vale em qualquer idioma do Windows.
    let path = w!(r"\GPU Adapter Memory(*)\Dedicated Usage");
    let ok = unsafe { PdhAddEnglishCounterW(query, path, 0, &mut counter) } == 0
        && unsafe { PdhCollectQueryData(query) } == 0;
    if ok {
        for (instance, bytes) in read_array(counter) {
            if let Some(luid) = parse_luid(&instance) {
                *out.entry(luid).or_insert(0) += bytes;
            }
        }
    }
    unsafe { PdhCloseQuery(query) };
    out
}

fn read_array(counter: PDH_HCOUNTER) -> Vec<(String, u64)> {
    let (mut size, mut count) = (0u32, 0u32);
    let status = unsafe { PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut size, &mut count, None) };
    if status != PDH_MORE_DATA || size == 0 {
        return vec![];
    }
    // O buffer guarda os itens e, depois deles, as strings para as quais `szName` aponta.
    let item = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    let mut buf = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); (size as usize).div_ceil(item)];
    let status =
        unsafe { PdhGetFormattedCounterArrayW(counter, PDH_FMT_LARGE, &mut size, &mut count, Some(buf.as_mut_ptr())) };
    if status != 0 {
        return vec![];
    }
    buf[..count as usize]
        .iter()
        .filter(|it| it.FmtValue.CStatus == 0)
        .filter_map(|it| {
            let name = unsafe { it.szName.to_string() }.ok()?;
            let bytes = unsafe { it.FmtValue.Anonymous.largeValue };
            Some((name, u64::try_from(bytes).ok()?))
        })
        .collect()
}
