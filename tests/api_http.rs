//! Gate da borda externa: HTTP/JSON real (sem dependência de cliente).
//!
//! Sobe o servidor em porta efêmera e fala HTTP/1.1 cru por `TcpStream`.
//! Cobre: saúde, catálogo, `remember`/`cue` com isolamento de escopo, abstenção,
//! `solve` com ótimo conhecido, dialeto OpenAI e dialeto llama.cpp.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use aether_engine::api;

fn start() -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve(listener);
    });
    addr
}

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).expect("write");
    let mut resp = String::new();
    s.read_to_string(&mut resp).expect("read");
    let (head, payload) = resp.split_once("\r\n\r\n").expect("separador http");
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, payload.to_string())
}

fn phases() -> String {
    let v: Vec<String> = (0..160)
        .map(|i| format!("{:.6}", (i as f32 * 0.01).sin()))
        .collect();
    format!("[{}]", v.join(","))
}

#[test]
fn saude_e_catalogo() {
    let addr = start();
    let (st, body) = request(addr, "GET", "/health", "");
    assert_eq!(st, 200);
    assert!(body.contains("\"status\":\"ok\""), "{body}");
    assert!(body.contains("\"t0_ledger_calls\":0"), "{body}");

    let (st, body) = request(addr, "GET", "/v1/models", "");
    assert_eq!(st, 200);
    assert!(body.contains("aether-phase-160"), "{body}");

    let (st, body) = request(addr, "GET", "/props", "");
    assert_eq!(st, 200);
    assert!(body.contains("\"phase_dims\":160"), "{body}");
}

#[test]
fn remember_e_cue_com_isolamento_de_escopo() {
    let addr = start();
    let body = format!(
        r#"{{"key":"42","phases":{},"scope":{{"organism":1,"run":1,"app":1}},
            "certificate":{{"energy":-1.5,"gap":0.25,"violations":0}},"authority":255}}"#,
        phases()
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &body);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"stored\":true"), "{resp}");

    let query = format!(r#"{{"scope":{{"organism":1,"run":1,"app":1}},"query":{}}}"#, phases());
    let (st, resp) = request(addr, "POST", "/v1/aether/cue", &query);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"abstain\":false"), "{resp}");
    assert!(resp.contains("\"key\":\"42\""), "{resp}");
    assert!(resp.contains("\"authority\":255"), "{resp}");

    // Outro organismo no mesmo servidor: abstém, nunca devolve o de 1.
    let outro = format!(r#"{{"scope":{{"organism":2,"run":1,"app":1}},"query":{}}}"#, phases());
    let (st, resp) = request(addr, "POST", "/v1/aether/cue", &outro);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"abstain\":true"), "{resp}");
    assert!(resp.contains("empty_scope"), "{resp}");
    assert!(!resp.contains("\"key\":\"42\""), "{resp}");
}

#[test]
fn solve_atinge_otimo_conhecido() {
    let addr = start();
    // Ferromagneto n=2: H = -s1·s2 ⇒ ótimo -1.
    let body = r#"{"n":2,"j":[[0,1],[1,0]],"h":[0,0],"steps":80,"restarts":4,"seed":7}"#;
    let (st, resp) = request(addr, "POST", "/v1/aether/solve", body);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"energy\":-1"), "{resp}");
    assert!(resp.contains("\"coherent\":true"), "{resp}");
    assert!(resp.contains("\"states\":[-1,-1]") || resp.contains("\"states\":[1,1]"), "{resp}");
}

#[test]
fn openai_chat_abstem_sem_ledger_e_cita_com_ledger() {
    let addr = start();
    let chat = r#"{"model":"aether-phase-160","messages":[{"role":"user","content":"ola"}]}"#;
    let (st, resp) = request(addr, "POST", "/v1/chat/completions", chat);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("chat.completion"), "{resp}");
    assert!(resp.contains("content_filter"), "{resp}");
    assert!(resp.contains("empty_scope"), "{resp}");

    // Popula o escopo default (0,0,0) e o mesmo prompt passa a citar.
    let body = format!(
        r#"{{"key":"7","phases":{},"scope":{{"organism":0,"run":0,"app":0}},
            "certificate":{{"energy":-2.0,"gap":0.5,"violations":0}},"authority":200}}"#,
        phases()
    );
    let (st, _) = request(addr, "POST", "/v1/aether/remember", &body);
    assert_eq!(st, 200);

    let (st, resp) = request(addr, "POST", "/v1/chat/completions", chat);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"finish_reason\":\"stop\""), "{resp}");
    assert!(resp.contains("\"verdict\":\"act\""), "{resp}");
}

#[test]
fn dialeto_llama_cpp_e_404() {
    let addr = start();
    let (st, resp) = request(addr, "POST", "/completion", r#"{"prompt":"teste"}"#);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"stop\":true"), "{resp}");
    assert!(resp.contains("<abstain>"), "{resp}");

    let (st, resp) = request(addr, "GET", "/nao-existe", "");
    assert_eq!(st, 404, "{resp}");
    assert!(resp.contains("not_found"), "{resp}");
}
