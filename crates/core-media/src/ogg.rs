//! Duração de áudios Opus lida do container Ogg, sem decodificar o áudio.

const PAGE_HEADER: usize = 27;
/// O Opus sempre usa relógio de 48 kHz no granule position.
const OPUS_RATE: u64 = 48_000;

/// Duração em ms: granule position da última página, menos o *pre-skip* do cabeçalho
/// `OpusHead`, dividido por 48.000. Retorna `None` se o arquivo não for Ogg Opus válido.
pub fn opus_duration_ms(bytes: &[u8]) -> Option<i64> {
    let mut pos = 0;
    let mut pre_skip: Option<u64> = None;
    let mut last_granule: Option<u64> = None;
    while pos + PAGE_HEADER <= bytes.len() && &bytes[pos..pos + 4] == b"OggS" {
        let granule = u64::from_le_bytes(bytes[pos + 6..pos + 14].try_into().ok()?);
        let n_segs = bytes[pos + 26] as usize;
        let seg_table = bytes.get(pos + PAGE_HEADER..pos + PAGE_HEADER + n_segs)?;
        let data_start = pos + PAGE_HEADER + n_segs;
        let data_len: usize = seg_table.iter().map(|&s| s as usize).sum();
        if pre_skip.is_none() {
            let head = bytes.get(data_start..data_start + 12)?;
            if &head[..8] != b"OpusHead" {
                return None;
            }
            pre_skip = Some(u16::from_le_bytes([head[10], head[11]]) as u64);
        }
        // -1 (todos os bits ligados) indica página sem pacote terminando nela.
        if granule != u64::MAX {
            last_granule = Some(granule);
        }
        pos = data_start + data_len;
    }
    let samples = last_granule?.checked_sub(pre_skip?)?;
    Some((samples * 1000 / OPUS_RATE) as i64)
}
