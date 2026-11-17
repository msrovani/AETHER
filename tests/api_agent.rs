//! A rota `/v1/aether/agent`: inferência + coerência pela borda HTTP.
//!
//! Um app de IA não precisa linkar a biblioteca: fala JSON e recebe ação,
//! resposta, certificado e abstenção — as mesmas garantias da CLI.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use aether_engine::api::{self, ApiConfig};

fn start() -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(
            listener,
            ApiConfig {
                threads: 1,
                ..ApiConfig::default()
            },
        );
    });
    addr
}

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
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

#[test]
fn api_agent_responde_fato_e_abstem_no_desconhecido() {
    let addr = start();
    let (status, health) = request(addr, "GET", "/health", "");
    assert_eq!(status, 200);
    assert!(health.contains("\"agent\":true"), "{health}");

    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"subject":"Brasil","relation":"capital"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"act\""), "{body}");
    assert!(body.contains("\"answer\":\"Brasília\""), "{body}");
    assert!(body.contains("\"experts\":[\"transplant\"]"), "{body}");

    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"subject":"Atlântida","relation":"capital"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"abstain\""), "{body}");
}

#[test]
fn api_agent_resolve_sat_e_qubo_com_certificado() {
    let addr = start();
    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"n":3,"clauses":[[1,2,0],[-1,3,0]]}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"act\""), "{body}");
    assert!(body.contains("\"certificate\""), "{body}");
    assert!(body.contains("\"violations\":0"), "{body}");

    let (status, body) = request(
        addr,
        "POST",
        "/v1/aether/agent",
        r#"{"n":4,"j":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"h":[1,1,1,1]}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"action\":\"act\""), "{body}");
    assert!(body.contains("\"answer\":\"++++\""), "{body}");
}

#[test]
fn api_agent_aceita_pipeline_steps() {
    let addr = start();
    let body = r#"{
      "steps":[
        {"kind":"fact","subject":"Brasil","relation":"capital"},
        {"kind":"sat","n":3,"clauses":[[1,2,0],[-1,3,0]]}
      ],
      "budget":{"max_steps":6,"max_ms":5000}
    }"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", body);
    assert_eq!(status, 200, "{resp}");
    assert!(resp.contains("\"pipeline\":true"), "{resp}");
    assert!(resp.contains("\"trace\""), "{resp}");
    assert!(resp.contains("\"success\":true"), "{resp}");
}

#[test]
fn api_agent_pipeline_rejeita_acima_do_cap() {
    let addr = start();
    let too_many = r#"{"steps":[
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"},
      {"kind":"fact","subject":"Brasil","relation":"capital"}
    ]}"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", too_many);
    assert_eq!(status, 400, "{resp}");
    assert!(resp.contains("8 steps") || resp.contains("pipeline"), "{resp}");

    let big_n = r#"{"steps":[{"kind":"sat","n":65,"clauses":[[1,2,0]]}]}"#;
    let (status, resp) = request(addr, "POST", "/v1/aether/agent", big_n);
    assert_eq!(status, 400, "{resp}");
    assert!(resp.contains("n fora") || resp.contains("64"), "{resp}");
}
