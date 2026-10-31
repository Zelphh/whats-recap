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
