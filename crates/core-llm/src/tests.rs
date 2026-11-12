//! Testes sem modelo: um servidor HTTP falso faz o papel do `llama-server`.

use super::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// Responde a uma requisição com `status` e `body` e devolve, pelo canal, o corpo recebido.
fn fake_server(status: u16, body: Value) -> (String, mpsc::Receiver<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut len = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':')
                && k.eq_ignore_ascii_case("content-length")
            {
                len = v.trim().parse().unwrap();
            }
        }
        let mut req = vec![0; len];
        reader.read_exact(&mut req).unwrap();
        let _ = tx.send(serde_json::from_slice(&req).unwrap_or(Value::Null));
        let body = body.to_string();
        let mut stream = stream;
        write!(
            stream,
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    (url, rx)
}

fn completion(content: &str, finish_reason: &str) -> Value {
    json!({
        "choices": [{ "message": { "role": "assistant", "content": content }, "finish_reason": finish_reason }],
        "timings": { "prompt_n": 812, "prompt_ms": 640.5, "predicted_n": 19, "predicted_ms": 505.0 }
    })
}

#[derive(Debug, Deserialize, PartialEq)]
struct Out {
    r#type: String,
}

#[test]
fn chat_sends_schema_and_parses_output() {
    let (url, rx) = fake_server(200, completion(r#"{"type":"quantitative"}"#, "stop"));
    let schema = json!({ "type": "object", "properties": { "type": { "enum": ["quantitative"] } } });
    let out = Client::new(&url)
        .chat::<Out>("sistema", "pergunta", &schema, 50)
        .unwrap();

    assert_eq!(
        out.value,
        Out {
            r#type: "quantitative".into()
        }
    );
    assert_eq!(out.content, r#"{"type":"quantitative"}"#);
    assert_eq!(out.timings.prompt_tokens, 812);
    assert_eq!(out.timings.generated_tokens, 19);

    let req = rx.recv().unwrap();
    assert_eq!(req["response_format"]["json_schema"]["schema"], schema);
    assert_eq!(req["messages"][0], json!({ "role": "system", "content": "sistema" }));
    assert_eq!(req["messages"][1], json!({ "role": "user", "content": "pergunta" }));
    assert_eq!(req["temperature"], 0.0);
    assert_eq!(req["max_tokens"], 50);
}

#[test]
fn truncated_output_is_an_error() {
    let (url, _rx) = fake_server(200, completion(r#"{"type":"quan"#, "length"));
    let err = Client::new(&url).chat::<Out>("s", "u", &json!({}), 5).unwrap_err();
    assert!(matches!(err, LlmError::Truncated(5)), "{err}");
}

#[test]
fn output_outside_the_type_is_an_error() {
    let (url, _rx) = fake_server(200, completion(r#"{"tipo":"x"}"#, "stop"));
    let err = Client::new(&url).chat::<Out>("s", "u", &json!({}), 50).unwrap_err();
    assert!(
        matches!(err, LlmError::InvalidOutput { ref content, .. } if content == r#"{"tipo":"x"}"#),
        "{err}"
    );
}

#[test]
fn http_error_keeps_the_server_message() {
    let (url, _rx) = fake_server(500, json!({ "error": { "message": "falha no schema" } }));
    let err = Client::new(&url).chat::<Out>("s", "u", &json!({}), 50).unwrap_err();
    assert!(
        matches!(err, LlmError::Http(ref m) if m.contains("500") && m.contains("falha no schema")),
        "{err}"
    );
}

#[test]
fn readiness_follows_health_status() {
    let (url, _rx) = fake_server(200, json!({ "status": "ok" }));
    assert!(Client::new(&url).is_ready());
    let (url, _rx) = fake_server(503, json!({ "error": { "message": "Loading model" } }));
    assert!(!Client::new(&url).is_ready());
}

#[test]
fn missing_files_are_reported() {
    let cfg = ServerConfig::new("nao-existe/llama-server", env!("CARGO_MANIFEST_DIR"));
    assert!(matches!(Server::start(&cfg), Err(LlmError::NotFound(p)) if p.ends_with("llama-server")));
}

#[test]
fn server_that_exits_reports_the_log() {
    // O próprio executável de teste faz o papel de um llama-server que falha: ele não reconhece
    // as flags (`-m`, `--host`…) e encerra na hora, como o llama-server sem memória para o modelo.
    let log = std::env::temp_dir().join(format!("core-llm-test-{}.log", std::process::id()));
    let mut cfg = ServerConfig::new(std::env::current_exe().unwrap(), env!("CARGO_MANIFEST_DIR"));
    cfg.log = Some(log.clone());
    let err = Server::start(&cfg).err().expect("o processo deveria ter saído");
    let _ = std::fs::remove_file(&log);
    assert!(
        matches!(err, LlmError::Exited { ref log, .. } if !log.is_empty()),
        "{err}"
    );
}

#[test]
fn free_port_is_usable() {
    let port = server::free_port().unwrap();
    assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
}

#[test]
fn parses_list_devices() {
    let out = "ggml_vulkan: Found 1 Vulkan devices:\n\
ggml_vulkan: 0 = AMD Radeon RX 7600 (AMD proprietary driver) | uma: 0 | fp16: 1\n\
Available devices:\n  Vulkan0: AMD Radeon RX 7600 (8176 MiB, 7362 MiB free)\n";
    assert_eq!(
        devices::parse_devices(out),
        vec![Device {
            id: "Vulkan0".into(),
            name: "AMD Radeon RX 7600".into(),
            total_mib: 8176,
            free_mib: 7362
        }]
    );
    assert!(devices::parse_devices("Available devices:\n").is_empty());
}

#[test]
fn plan_prefers_the_largest_model_that_fits() {
    let both = [&QWEN35_9B, &QWEN35_4B];
    let gpu = |m| Choice {
        model: m,
        backend: Backend::Gpu,
    };
    let cpu = |m| Choice {
        model: m,
        backend: Backend::Cpu,
    };
    assert_eq!(
        plan(&both, Some(7362)),
        vec![gpu(&QWEN35_9B), gpu(&QWEN35_4B), cpu(&QWEN35_4B)]
    );
    assert_eq!(plan(&both, Some(4000)), vec![gpu(&QWEN35_4B), cpu(&QWEN35_4B)]);
    assert_eq!(plan(&[&QWEN35_9B], Some(4000)), vec![cpu(&QWEN35_9B)]);
    assert_eq!(plan(&both, None), vec![cpu(&QWEN35_4B)]);
    assert!(plan(&[], Some(8000)).is_empty());
    // Flags por modelo depois das de hardware.
    assert_eq!(gpu(&QWEN35_9B).args(None), RECOMMENDED_ARGS);
    assert_eq!(
        gpu(&QWEN35_9B).args(Some("Vulkan0")),
        [
            "-ngl",
            "99",
            "-c",
            "16384",
            "-dev",
            "Vulkan0",
            "-fa",
            "off",
            "--reasoning",
            "off"
        ]
    );
    assert_eq!(
        cpu(&QWEN35_4B).args(Some("Vulkan0")),
        [
            "-ngl",
            "0",
            "-c",
            "8192",
            "-dev",
            "none",
            "-fa",
            "off",
            "--reasoning",
            "off"
        ]
    );
}

/// Servidor de arquivo com suporte a `Range`; `cut` encerra a primeira resposta depois de tantos
/// bytes (conexão caída no meio do download).
fn file_server(data: Vec<u8>, cut: Option<usize>) -> (String, mpsc::Receiver<Option<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/modelo.gguf", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut cut = cut;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut range = None;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':')
                    && k.eq_ignore_ascii_case("range")
                {
                    range = Some(v.trim().to_string());
                }
            }
            let _ = tx.send(range.clone());
            let start: usize = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes=")?.strip_suffix('-')?.parse().ok())
                .unwrap_or(0);
            let (status, body) = if start > 0 {
                (206, &data[start..])
            } else {
                (200, &data[..])
            };
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let send = cut.take().map_or(body.len(), |c| c.min(body.len()));
            let _ = stream.write_all(&body[..send]);
        }
    });
    (url, rx)
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wr-llm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn download_resumes_and_checks_the_hash() {
    use std::sync::atomic::AtomicBool;
    let data: Vec<u8> = (0..3_500_000u32).map(|i| (i % 251) as u8).collect();
    let hash = sha256_hex(&data);
    let dir = temp_dir("download");
    let dest = dir.join("modelo.gguf");
    let no_cancel = AtomicBool::new(false);

    // A conexão cai depois de 1,5 MB: erro, e o `.partial` fica.
    let (url, ranges) = file_server(data.clone(), Some(1_500_000));
    let spec = DownloadSpec {
        url: &url,
        size: data.len() as u64,
        sha256: &hash,
    };
    assert!(download(&spec, &dest, &no_cancel, &mut |_, _, _| {}).is_err());
    assert!(!dest.exists());
    let kept = std::fs::metadata(partial_path(&dest)).unwrap().len();
    assert!(kept > 0 && kept < data.len() as u64, "{kept}");
    assert_eq!(ranges.recv().unwrap(), None);

    // Retoma de onde parou e confere o hash do arquivo inteiro.
    let mut phases = Vec::new();
    download(&spec, &dest, &no_cancel, &mut |p, done, _| phases.push((p, done))).unwrap();
    assert_eq!(ranges.recv().unwrap(), Some(format!("bytes={kept}-")));
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert!(!partial_path(&dest).exists());
    assert_eq!(phases.first().map(|p| p.0), Some(DownloadPhase::Resume));
    assert_eq!(phases.last(), Some(&(DownloadPhase::Download, data.len() as u64)));

    // Já baixado: não faz requisição.
    download(&spec, &dest, &no_cancel, &mut |_, _, _| {}).unwrap();
    assert!(ranges.try_recv().is_err());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn download_rejects_a_wrong_hash_and_can_be_cancelled() {
    use std::sync::atomic::AtomicBool;
    let data = vec![7u8; 2_000_000];
    let dir = temp_dir("hash");
    let dest = dir.join("modelo.gguf");
    let (url, _) = file_server(data.clone(), None);
    let wrong = DownloadSpec {
        url: &url,
        size: data.len() as u64,
        sha256: &"0".repeat(64),
    };
    let e = download(&wrong, &dest, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap_err();
    assert!(matches!(e, LlmError::Checksum { .. }), "{e}");
    assert!(!dest.exists() && !partial_path(&dest).exists());

    let hash = sha256_hex(&data);
    let spec = DownloadSpec { sha256: &hash, ..wrong };
    let e = download(&spec, &dest, &AtomicBool::new(true), &mut |_, _, _| {}).unwrap_err();
    assert!(matches!(e, LlmError::Cancelled), "{e}");
    assert!(!dest.exists());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn warm_sends_each_prompt_once() {
    // O servidor falso atende uma conexão: um prompt só.
    let (a, rx_a) = fake_server(200, completion("{", "length"));
    let client = Client::new(&a);
    client.warm(&[("sistema".into(), json!({}))]);
    let req = rx_a.recv().unwrap();
    assert_eq!(req["messages"][0]["content"], "sistema");
    assert_eq!(req["max_tokens"], 1);
}
