//! Download do modelo com retomada e checagem de tamanho e SHA-256.
//!
//! Baixa para `<destino>.partial` e só renomeia depois de conferir o hash: um arquivo com o nome
//! final está sempre completo. Interrompido (cancelado, sem rede, app fechado), o `.partial` fica e
//! a próxima chamada continua de onde parou com `Range`.

use crate::LlmError;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const CHUNK: usize = 1 << 20;

/// O que baixar.
#[derive(Debug, Clone)]
pub struct DownloadSpec<'a> {
    pub url: &'a str,
    pub size: u64,
    /// SHA-256 em hexadecimal minúsculo.
    pub sha256: &'a str,
}

/// Fase do download, para a interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadPhase {
    /// Relendo o `.partial` de uma tentativa anterior (o hash é calculado em streaming).
    Resume,
    Download,
}

pub fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".partial");
    PathBuf::from(name)
}

/// Baixa `spec` para `dest`. `on_progress(fase, bytes, total)` é chamado a cada pedaço (1 MiB);
/// `cancel` interrompe entre pedaços e deixa o `.partial` para retomar.
pub fn download(
    spec: &DownloadSpec,
    dest: &Path,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(DownloadPhase, u64, u64),
) -> Result<(), LlmError> {
    if dest.metadata().is_ok_and(|m| m.len() == spec.size) {
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = partial_path(dest);
    let mut hasher = Sha256::new();
    let mut done = rehash_partial(&partial, spec.size, &mut hasher, cancel, on_progress)?;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        // Sem limite global (5 GB levam minutos), mas uma conexão parada não trava para sempre.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into();

    if done < spec.size {
        let mut req = agent.get(spec.url);
        if done > 0 {
            req = req.header("Range", format!("bytes={done}-"));
        }
        let resp = req.call()?;
        let status = resp.status().as_u16();
        let mut file = match status {
            206 => OpenOptions::new().append(true).open(&partial)?,
            // O servidor ignorou o Range: recomeça.
            200 => {
                hasher = Sha256::new();
                done = 0;
                File::create(&partial)?
            }
            _ => return Err(LlmError::Http(format!("HTTP {status} ao baixar {}", spec.url))),
        };
        let mut body = resp.into_body().into_reader();
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(LlmError::Cancelled);
            }
            let n = read_full(&mut body, &mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            done += n as u64;
            if done > spec.size {
                break;
            }
            on_progress(DownloadPhase::Download, done, spec.size);
        }
        file.sync_all()?;
    }

    if done != spec.size {
        if done > spec.size {
            std::fs::remove_file(&partial)?;
        }
        return Err(LlmError::Http(format!(
            "download incompleto: {done} de {} bytes",
            spec.size
        )));
    }
    let got = hex(&hasher.finalize());
    if !got.eq_ignore_ascii_case(spec.sha256) {
        std::fs::remove_file(&partial)?;
        return Err(LlmError::Checksum {
            expected: spec.sha256.to_string(),
            got,
        });
    }
    std::fs::rename(&partial, dest)?;
    Ok(())
}

/// Hash do que já foi baixado (de uma tentativa anterior). Devolve quantos bytes há.
fn rehash_partial(
    partial: &Path,
    size: u64,
    hasher: &mut Sha256,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(DownloadPhase, u64, u64),
) -> Result<u64, LlmError> {
    let Ok(mut file) = File::open(partial) else {
        return Ok(0);
    };
    if file.metadata()?.len() > size {
        drop(file);
        std::fs::remove_file(partial)?;
        return Ok(0);
    }
    let mut buf = vec![0u8; CHUNK];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(LlmError::Cancelled);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(done);
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        on_progress(DownloadPhase::Resume, done, size);
    }
}

/// Lê até encher `buf` ou o corpo acabar (o leitor HTTP devolve pedaços pequenos).
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match r.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
