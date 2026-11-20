//! F5 T4 — `/v1/aether/decision`: o mesmo contrato do `DecisionPort` pela borda HTTP.
//!
//! O app externo recebe o schema tipado do crate e a degradação honesta do
//! `StubPort`; não há atalho para o Python e nenhuma chamada externa por default.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use aether_engine::api::json::Json;
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

fn request(addr: SocketAddr, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "POST /v1/aether/decision HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
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

fn json_of(body: &str) -> Json {
    Json::parse(body).unwrap_or_else(|e| panic!("resposta não é JSON ({e}): {body}"))
}

#[test]
fn decision_abstém_com_schema_do_crate_sem_engine() {
    let addr = start();
    let (status, payload) = request(
        addr,
        r#"{"state":"estado de teste","questions":{"passou":{"type":"boolean","instructions":"passa?"}}}"#,
    );
    assert_eq!(status, 200, "{payload}");
    let doc = json_of(&payload);
    let answers = doc.get("answers").expect("answers");
    let list = answers.as_arr().expect("answers deve ser array");
    let passou = list
        .iter()
        .find(|a| a.get("question").and_then(Json::as_str) == Some("passou"))
        .expect("answer passou");
    assert_eq!(
        passou.get("abstain").and_then(Json::as_bool),
        Some(true),
        "{payload}"
    );
    assert_eq!(
        passou
            .get("abstain_reason")
            .and_then(Json::as_str)
            .unwrap_or(""),
        "engine_unavailable",
        "{payload}"
    );
    let meta = doc.get("meta").expect("meta");
    assert_eq!(
        meta.get("engine").and_then(Json::as_str).unwrap_or(""),
        "stub",
        "{payload}"
    );
    assert_eq!(meta.get("degraded").and_then(Json::as_bool), Some(true));
}

#[test]
fn decision_rejeita_state_fora_do_orcamento() {
    let addr = start();
    let big = "x".repeat(320 * 4 + 1);
    let body = format!(
        "{{\"state\":\"{big}\",\"questions\":{{\"q\":{{\"type\":\"boolean\"}}}}}}"
    );
    let (status, payload) = request(addr, &body);
    assert_eq!(status, 400, "{payload}");
    let doc = json_of(&payload);
    let err = doc.get("error").expect("error");
    assert_eq!(err.get("type").and_then(Json::as_str), Some("bad_request"), "{payload}");
}

#[test]
fn decision_rejeita_questions_vazio() {
    let addr = start();
    let (status, payload) = request(addr, r#"{"state":"ok","questions":{}}"#);
    assert_eq!(status, 400, "{payload}");
}
