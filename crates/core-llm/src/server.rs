//! O `llama-server` como processo filho: sobe numa porta livre, espera o modelo carregar e é
//! encerrado quando o [`Server`] sai de escopo.

use crate::{Client, LlmError};
use std::fs::File;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Flags do modelo escolhido (Qwen3.5 9B `IQ4_XS`) na RX 7600, medidas no `whatsrecap-evals`
/// (`COMPARACAO-MODELOS.md`): todas as camadas na GPU, contexto de 16k e **sem flash attention**,
/// que no Vulkan deixa o Qwen3.5 2–2,5× mais lento. `--reasoning off`: as saídas são JSON curto.
pub const RECOMMENDED_ARGS: &[&str] = &["-ngl", "99", "-c", "16384", "-fa", "off", "--reasoning", "off"];

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Executável do `llama-server`.
    pub bin: PathBuf,
    /// Arquivo `.gguf` do modelo.
    pub model: PathBuf,
    /// Flags de hardware e do modelo (ver [`RECOMMENDED_ARGS`]).
    pub args: Vec<String>,
    /// Arquivo para a saída do servidor. Sem ele, um erro na carga não traz o fim do log.
    pub log: Option<PathBuf>,
    /// Quanto esperar o modelo carregar (de um HD lento, o 9B leva mais de um minuto).
    pub ready_timeout: Duration,
}

impl ServerConfig {
    pub fn new(bin: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        ServerConfig {
            bin: bin.into(),
            model: model.into(),
            args: RECOMMENDED_ARGS.iter().map(|s| s.to_string()).collect(),
            log: None,
            ready_timeout: Duration::from_secs(300),
        }
    }
}

pub struct Server {
    child: Child,
    client: Client,
    log: Option<PathBuf>,
}

impl Server {
    /// Sobe o servidor e só retorna quando o modelo estiver carregado.
    pub fn start(cfg: &ServerConfig) -> Result<Server, LlmError> {
        for path in [&cfg.bin, &cfg.model] {
            if !path.exists() {
                return Err(LlmError::NotFound(path.clone()));
            }
        }
        let port = free_port()?;
        let (stdout, stderr) = match &cfg.log {
            Some(path) => {
                let f = File::create(path)?;
                (Stdio::from(f.try_clone()?), Stdio::from(f))
            }
            None => (Stdio::null(), Stdio::null()),
        };
        let mut cmd = Command::new(&cfg.bin);
        cmd.arg("-m")
            .arg(&cfg.model)
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--no-webui",
                // Um slot por prompt de sistema (roteador e seleção de ferramenta): cada um mantém
                // o seu cache na VRAM. Sem o cache em RAM, que copiava o slot ocioso (~58 MiB) a
                // cada troca: com ele, ou com um slot só, cada chamada custava ~1 s a mais no Qwen3.5.
                "-np",
                "2",
                "--cache-ram",
                "0",
            ])
            .args(&cfg.args)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        hide_console(&mut cmd);
        let child = cmd.spawn().map_err(LlmError::Spawn)?;
        let mut server = Server {
            child,
            client: Client::new(&format!("http://127.0.0.1:{port}")),
            log: cfg.log.clone(),
        };
        server.wait_ready(cfg.ready_timeout)?;
        Ok(server)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    fn wait_ready(&mut self, timeout: Duration) -> Result<(), LlmError> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(status) = self.child.try_wait()? {
                return Err(LlmError::Exited {
                    status: status.to_string(),
                    log: self.log.as_deref().map(log_tail).unwrap_or_default(),
                });
            }
            if self.client.is_ready() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(LlmError::Timeout(timeout))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // O llama-server não guarda estado: encerrar à força é seguro (e é o único jeito no Windows).
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Porta livre escolhida pelo sistema. Há uma janela pequena entre soltar a porta e o servidor
/// ocupá-la; se outro processo pegar a porta, o servidor sai e [`Server::start`] devolve o erro.
pub(crate) fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// Fim do log, para mostrar o motivo de uma falha na carga (memória insuficiente, arquivo inválido).
fn log_tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().rev().take(20).collect();
    if lines.is_empty() {
        return String::new();
    }
    let tail: Vec<&str> = lines.into_iter().rev().collect();
    format!(":\n{}", tail.join("\n"))
}

/// No Windows, um processo de console aberto por um app gráfico abre uma janela preta.
#[cfg(windows)]
fn hide_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut Command) {}
