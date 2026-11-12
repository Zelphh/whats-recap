//! Modelo de linguagem do chat no app: onde ficam o `llama-server` e os modelos, qual modelo roda
//! (na GPU, se couber; senão na CPU), o aquecimento dos prompts e o estado para a interface.
//!
//! O `llama-server` vem no instalador, na pasta `llama/` dos recursos (`scripts/setup-llama.ps1`
//! prepara a pasta). Os modelos não: são baixados pelo app para `<app_data>/models`.

use core_llm::{Backend, Choice, Client, Device, ModelSpec, Server, ServerConfig};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// O servidor no ar e o que ele está rodando.
struct Running {
    server: Server,
    active: Active,
    /// Conversa cujos prompts já estão no cache dos slots.
    warmed_for: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Active {
    model_id: &'static str,
    model_name: &'static str,
    backend: Backend,
    /// Placa usada (só na GPU).
    device: Option<String>,
}

/// O servidor só sobe quando o chat abre (ou na primeira pergunta livre) e fica de pé até o app
/// fechar ou o usuário descarregar o modelo.
#[derive(Default)]
pub struct LlmState {
    server: Mutex<Option<Running>>,
    /// Subindo o servidor ou aquecendo os prompts.
    loading: AtomicBool,
    /// Cópia do que está rodando, legível enquanto `server` está travado.
    active: Mutex<Option<Active>>,
    /// Por que a última tentativa de subir o servidor falhou.
    last_error: Mutex<Option<String>>,
    /// Download em andamento: modelo e sinal de cancelamento.
    download: Mutex<Option<(&'static str, Arc<AtomicBool>)>>,
}

const SERVER_EXE: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};

/// Pasta do `whatsrecap-evals` ao lado deste repositório (só em desenvolvimento).
fn evals_dir() -> Option<PathBuf> {
    cfg!(debug_assertions).then(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../whatsrecap-evals"))
}

/// O `llama-server`: `WHATSRECAP_LLAMA_SERVER`, o que vem no app (`<recursos>/llama/`) ou, em
/// desenvolvimento sem a pasta preparada, o do `whatsrecap-evals`.
pub fn llama_bin(app: &AppHandle) -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("WHATSRECAP_LLAMA_SERVER").map(PathBuf::from),
        app.path().resource_dir().ok().map(|d| d.join("llama").join(SERVER_EXE)),
        evals_dir().map(|d| d.join("vendor/llama.cpp/current").join(SERVER_EXE)),
    ];
    candidates.into_iter().flatten().find(|p| p.is_file())
}

/// Onde os modelos são baixados: `WHATSRECAP_MODELS_DIR` ou `<app_data>/models`.
pub fn download_dir(app: &AppHandle) -> Result<PathBuf, String> {
    match std::env::var_os("WHATSRECAP_MODELS_DIR") {
        Some(d) => Ok(PathBuf::from(d)),
        None => Ok(app.path().app_data_dir().map_err(err)?.join("models")),
    }
}

/// Arquivo do modelo, se já estiver baixado (tamanho exato; um `.partial` não conta). Em
/// desenvolvimento, também vale o do `whatsrecap-evals`.
pub fn model_path(app: &AppHandle, spec: &ModelSpec) -> Option<PathBuf> {
    let dirs = [download_dir(app).ok(), evals_dir().map(|d| d.join("models"))];
    dirs.into_iter()
        .flatten()
        .map(|d| d.join(spec.file))
        .find(|p| p.metadata().is_ok_and(|m| m.len() == spec.size))
}

/// A GPU com mais memória livre.
fn best_gpu(bin: &std::path::Path) -> Option<Device> {
    core_llm::list_devices(bin).ok()?.into_iter().max_by_key(|d| d.free_mib)
}

impl LlmState {
    /// Há o executável e pelo menos um modelo: o chat pode usar o LLM.
    pub fn available(app: &AppHandle) -> bool {
        llama_bin(app).is_some() && core_llm::MODELS.iter().any(|m| model_path(app, m).is_some())
    }

    /// `"off"` (sem executável ou modelo: só regras fixas), `"idle"` (ainda não carregado),
    /// `"loading"` (subindo ou aquecendo) ou `"ready"`.
    pub fn status(&self, app: &AppHandle) -> &'static str {
        if self.loading.load(Ordering::SeqCst) {
            "loading"
        } else if self.active.lock().is_ok_and(|a| a.is_some()) {
            "ready"
        } else if Self::available(app) {
            "idle"
        } else {
            "off"
        }
    }

    /// Cliente do servidor, subindo-o se preciso. Tenta os modelos baixados na ordem de
    /// [`core_llm::plan`]: o maior que cabe na VRAM livre e, se a carga falhar, o próximo; por
    /// último, a CPU.
    fn ensure(&self, app: &AppHandle) -> Result<Client, String> {
        let mut guard = self.server.lock().map_err(err)?;
        if let Some(r) = guard.as_ref() {
            return Ok(r.server.client().clone());
        }
        self.loading.store(true, Ordering::SeqCst);
        let result = self.start(app);
        self.loading.store(false, Ordering::SeqCst);
        if let Ok(mut last) = self.last_error.lock() {
            *last = result.as_ref().err().cloned();
        }
        let running = result?;
        *self.active.lock().map_err(err)? = Some(running.active.clone());
        let client = running.server.client().clone();
        *guard = Some(running);
        Ok(client)
    }

    fn start(&self, app: &AppHandle) -> Result<Running, String> {
        let bin = llama_bin(app).ok_or("o llama-server não foi encontrado na instalação")?;
        let models: Vec<(&'static ModelSpec, PathBuf)> = core_llm::MODELS
            .iter()
            .filter_map(|m| model_path(app, m).map(|p| (m, p)))
            .collect();
        if models.is_empty() {
            return Err("nenhum modelo baixado (veja Configurações › Modelo de linguagem)".into());
        }
        let gpu = best_gpu(&bin);
        let available: Vec<&ModelSpec> = models.iter().map(|(m, _)| *m).collect();
        let log = app.path().app_data_dir().ok().map(|d| d.join("llama-server.log"));
        let mut failures = Vec::new();
        for choice in core_llm::plan(&available, gpu.as_ref().map(|d| d.free_mib)) {
            let model_file = &models.iter().find(|(m, _)| *m == choice.model).unwrap().1;
            let device = gpu.as_ref().filter(|_| choice.backend == Backend::Gpu);
            let mut cfg = ServerConfig::new(&bin, model_file);
            cfg.args = choice.args(device.map(|d| d.id.as_str()));
            cfg.log = log.clone();
            match Server::start(&cfg) {
                Ok(server) => {
                    return Ok(Running {
                        server,
                        active: active(&choice, device),
                        warmed_for: None,
                    });
                }
                Err(e) => failures.push(format!(
                    "{} ({}): {e}",
                    choice.model.name,
                    backend_label(choice.backend)
                )),
            }
        }
        Err(format!("o modelo não carregou. {}", failures.join("; ")))
    }

    /// Sobe o servidor e processa os prompts da conversa `id`, para que a primeira pergunta livre
    /// responda no tempo normal (~1–2 s). Não faz nada sem modelo; se já aqueceu para esta
    /// conversa, só confere que o servidor está no ar.
    pub fn prepare(
        &self,
        app: &AppHandle,
        id: &str,
        prompts: impl FnOnce() -> Result<Vec<(String, Value)>, String>,
    ) -> Result<(), String> {
        if !Self::available(app) {
            return Ok(());
        }
        self.ensure(app)?;
        let mut guard = self.server.lock().map_err(err)?;
        let Some(running) = guard.as_mut() else {
            return Ok(());
        };
        if running.warmed_for.as_deref() == Some(id) {
            return Ok(());
        }
        self.loading.store(true, Ordering::SeqCst);
        let prompts = prompts();
        if let Ok(p) = &prompts {
            running.server.client().warm(p);
            running.warmed_for = Some(id.to_string());
        }
        self.loading.store(false, Ordering::SeqCst);
        prompts.map(|_| ())
    }

    /// Encerra o servidor (libera a VRAM). A próxima pergunta livre sobe de novo.
    pub fn stop(&self) {
        if let Ok(mut s) = self.server.lock() {
            s.take();
        }
        if let Ok(mut a) = self.active.lock() {
            a.take();
        }
    }

    /// Começa um download; erro se já houver um.
    pub fn begin_download(&self, model: &'static str) -> Result<Arc<AtomicBool>, String> {
        let mut d = self.download.lock().map_err(err)?;
        if d.is_some() {
            return Err("já existe um download em andamento".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *d = Some((model, cancel.clone()));
        Ok(cancel)
    }

    pub fn end_download(&self) {
        if let Ok(mut d) = self.download.lock() {
            d.take();
        }
    }

    pub fn cancel_download(&self) {
        if let Ok(d) = self.download.lock()
            && let Some((_, cancel)) = d.as_ref()
        {
            cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Modelo em uso (se houver), para não apagar o arquivo aberto.
    pub fn active_model(&self) -> Option<&'static str> {
        self.active.lock().ok()?.as_ref().map(|a| a.model_id)
    }

    pub fn info(&self, app: &AppHandle) -> LlmInfo {
        let bin = llama_bin(app);
        let gpu = bin.as_deref().and_then(best_gpu);
        let available: Vec<&ModelSpec> = core_llm::MODELS
            .iter()
            .filter(|m| model_path(app, m).is_some())
            .collect();
        let next = core_llm::plan(&available, gpu.as_ref().map(|d| d.free_mib))
            .first()
            .map(|c| active(c, gpu.as_ref().filter(|_| c.backend == Backend::Gpu)));
        let downloading = self.download.lock().ok().and_then(|d| d.as_ref().map(|(m, _)| *m));
        let download_dir = download_dir(app).ok();
        LlmInfo {
            status: self.status(app),
            runtime: bin.is_some(),
            gpu,
            models: core_llm::MODELS
                .iter()
                .map(|m| {
                    let path = model_path(app, m);
                    let partial = download_dir
                        .as_ref()
                        .and_then(|d| core_llm::partial_path(&d.join(m.file)).metadata().ok())
                        .map(|md| md.len())
                        .filter(|_| path.is_none());
                    ModelInfo {
                        spec: *m,
                        downloaded: path.is_some(),
                        removable: path
                            .as_ref()
                            .is_some_and(|p| download_dir.as_ref().is_some_and(|d| p.starts_with(d))),
                        path: path.map(|p| p.display().to_string()),
                        partial_bytes: partial,
                        downloading: downloading == Some(m.id),
                    }
                })
                .collect(),
            active: self.active.lock().ok().and_then(|a| a.clone()),
            next: next.filter(|_| self.active.lock().is_ok_and(|a| a.is_none())),
            last_error: self.last_error.lock().ok().and_then(|e| e.clone()),
        }
    }
}

fn backend_label(b: Backend) -> &'static str {
    match b {
        Backend::Gpu => "GPU",
        Backend::Cpu => "CPU",
    }
}

fn active(choice: &Choice, device: Option<&Device>) -> Active {
    Active {
        model_id: choice.model.id,
        model_name: choice.model.name,
        backend: choice.backend,
        device: device.map(|d| d.name.clone()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    #[serde(flatten)]
    spec: ModelSpec,
    downloaded: bool,
    /// Baixado pelo app (o do `whatsrecap-evals`, em desenvolvimento, não é apagado daqui).
    removable: bool,
    /// Onde está o arquivo (pode ser o do `whatsrecap-evals`, em desenvolvimento).
    path: Option<String>,
    /// Bytes de um download interrompido, que será retomado.
    partial_bytes: Option<u64>,
    downloading: bool,
}

/// Situação do modelo de linguagem, para a tela de configurações.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmInfo {
    status: &'static str,
    /// O `llama-server` foi encontrado.
    runtime: bool,
    /// GPU com mais memória livre agora (com o modelo carregado, a memória livre já o desconta).
    gpu: Option<Device>,
    models: Vec<ModelInfo>,
    /// O que está carregado.
    active: Option<Active>,
    /// O que vai carregar na próxima pergunta livre (se nada estiver carregado).
    next: Option<Active>,
    last_error: Option<String>,
}

/// Ponte entre o chat e o `core-llm`: sobe o servidor na primeira chamada.
pub struct AppLlm<'a> {
    pub state: &'a LlmState,
    pub app: AppHandle,
}

impl core_chat::Llm for AppLlm<'_> {
    fn complete(&self, system: &str, user: &str, schema: &Value, max_tokens: u32) -> Result<Value, String> {
        let client = self.state.ensure(&self.app)?;
        match client.chat::<Value>(system, user, schema, max_tokens) {
            Ok(out) => Ok(out.value),
            Err(e) => {
                // Servidor fora do ar (caiu ou foi encerrado): sobe de novo na próxima pergunta.
                if matches!(e, core_llm::LlmError::Http(_)) {
                    self.state.stop();
                }
                Err(err(e))
            }
        }
    }
}
