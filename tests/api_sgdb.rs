//! A borda HTTP usa o `neural-sgdb` de verdade (quando configurada).
//!
//! Prova que um app de IA falando OpenAI/llama.cpp com o AETHER ganha
//! durabilidade sem mudar a rota:
//! 1. `remember` replica o padrão de fase e responde `sgdb=stored`;
//! 2. `cue` devolve `source=local` quando o cache local tem o padrão;
//! 3. banco fora não derruba a escrita (`sgdb=offline`) nem o `/health`;
//! 4. `/health` reflete `on`/`error`/`off`.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use aether_engine::api::{self, ApiConfig};

fn start(cfg: ApiConfig) -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });
    addr
}

fn request(addr: SocketAddr, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    let (head, payload) = response.split_once("\r\n\r\n").expect("separador");
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, payload.to_string())
}

fn get(addr: SocketAddr, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    let (head, payload) = response.split_once("\r\n\r\n").expect("separador");
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, payload.to_string())
}

fn phases() -> String {
    let values: Vec<String> = (0..8)
        .map(|i| format!("{:.6}", (i as f32 * 0.7).cos()))
        .collect();
    values.join(",")
}

fn remember_body(key: u128) -> String {
    format!(
        r#"{{"scope":{{"organism":1,"run":0,"app":1}},"phases":[{}],"key":"{}","authority":255}}"#,
        phases(),
        key
    )
}

fn cue_body() -> String {
    format!(
        r#"{{"scope":{{"organism":1,"run":0,"app":1}},"query":[{}]}}"#,
        phases()
    )
}

#[test]
fn api_replica_remember_e_serve_cue_com_sgdb() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    let addr = start(ApiConfig {
        threads: 1,
        sgdb: Some("mock".to_string()),
        ..ApiConfig::default()
    });

    let (status, health) = get(addr, "/health");
    assert_eq!(status, 200);
    assert!(health.contains("\"sgdb\":\"on\""), "{health}");

    let (status, body) = request(addr, "/v1/aether/remember", &remember_body(42));
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"sgdb\":\"stored\""), "{body}");

    let (status, body) = request(addr, "/v1/aether/cue", &cue_body());
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"abstain\":false"), "{body}");
    assert!(body.contains("\"source\":\"local\""), "{body}");
    assert!(body.contains("\"authority\":255"), "{body}");
}

#[test]
fn api_nao_derruba_escrita_quando_sgdb_esta_fora() {
    let _env_lock = aether_engine::test_env::env_test_lock();
    // O ambiente é lido pelo SgdbConfig::from_env() no boot da borda.
    std::env::set_var("AETHER_SGDB_RETRIES", "0");
    std::env::set_var("AETHER_SGDB_TIMEOUT_MS", "100");
    let addr = start(ApiConfig {
        threads: 1,
        sgdb: Some("http://127.0.0.1:1/mcp".to_string()),
        ..ApiConfig::default()
    });

    let (status, body) = request(addr, "/v1/aether/remember", &remember_body(43));
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"sgdb\":\"offline\""), "{body}");
    assert!(body.contains("\"stored\":true"), "{body}");

    let (status, health) = get(addr, "/health");
    assert_eq!(status, 200);
    assert!(health.contains("\"sgdb\":\"error\""), "{health}");

    // `cue` continua funcionando com o cache local mesmo sem o banco.
    let (status, body) = request(addr, "/v1/aether/cue", &cue_body());
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"abstain\":false"), "{body}");
}
