//! Gate da borda externa: HTTP/JSON real (sem dependência de cliente).
//!
//! Sobe o servidor em porta efêmera e fala HTTP/1.1 cru por `TcpStream`.
//! Cobre: saúde, catálogo, `remember`/`cue` com isolamento de escopo, abstenção,
//! `solve`, SSE, auth/escopo, persistência em disco, concorrência e os dois
//! dialetos (OpenAI e llama.cpp).

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use aether_engine::api::{self, ApiConfig};

fn start() -> SocketAddr {
    start_cfg(ApiConfig {
        threads: 2,
        ..ApiConfig::default()
    })
}

fn start_cfg(cfg: ApiConfig) -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });
    addr
}

fn request_h(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> (u16, String) {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
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

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    request_h(addr, method, path, body, &[])
}

fn phases() -> String {
    // Fasores unitários (320 floats): é o que o F1 espera e o que a borda
    // gera para "ola" — assim `remember` e `chat` casam de verdade.
    let v = api::text_to_phases("ola");
    let s: Vec<String> = v.iter().map(|p| format!("{p}")).collect();
    format!("[{}]", s.join(","))
}

fn remember_body(key: &str, organism: u64) -> String {
    format!(
        r#"{{"key":"{key}","phases":{},"scope":{{"organism":{organism},"run":1,"app":1}},
            "certificate":{{"energy":-1.5,"gap":0.25,"violations":0}},"authority":255}}"#,
        phases()
    )
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
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &remember_body("42", 1));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"stored\":true"), "{resp}");

    let query = format!(
        r#"{{"scope":{{"organism":1,"run":1,"app":1}},"query":{}}}"#,
        phases()
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/cue", &query);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"abstain\":false"), "{resp}");
    assert!(resp.contains("\"key\":\"42\""), "{resp}");
    assert!(resp.contains("\"authority\":255"), "{resp}");

    // Outro organismo no mesmo servidor: abstém, nunca devolve o de 1.
    let outro = format!(
        r#"{{"scope":{{"organism":2,"run":1,"app":1}},"query":{}}}"#,
        phases()
    );
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
    assert!(
        resp.contains("\"states\":[-1,-1]") || resp.contains("\"states\":[1,1]"),
        "{resp}"
    );
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

    let (st, _) = request(addr, "POST", "/v1/aether/remember", &remember_body("7", 0));
    assert_eq!(st, 200);

    let chat_scoped = r#"{"model":"aether-phase-160",
        "aether":{"scope":{"organism":0,"run":1,"app":1}},
        "messages":[{"role":"user","content":"ola"}]}"#;
    let (st, resp) = request(addr, "POST", "/v1/chat/completions", chat_scoped);
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

#[test]
fn sse_entrega_chunks_e_done() {
    let addr = start();
    let (st, _) = request(addr, "POST", "/v1/aether/remember", &remember_body("1", 0));
    assert_eq!(st, 200);
    let chat = r#"{"model":"aether-phase-160","stream":true,
                   "aether":{"scope":{"organism":0,"run":1,"app":1}},
                   "messages":[{"role":"user","content":"ola"}]}"#;
    let (st, resp) = request(addr, "POST", "/v1/chat/completions", chat);
    assert_eq!(st, 200, "{resp}");
    assert!(
        resp.contains("\"object\":\"chat.completion.chunk\""),
        "{resp}"
    );
    assert!(resp.contains("\"role\":\"assistant\""), "{resp}");
    assert!(resp.contains("data: [DONE]"), "{resp}");
    assert!(resp.contains("\"finish_reason\":\"stop\""), "{resp}");
}

#[test]
fn auth_exige_token_e_limita_organismo() {
    let addr = start_cfg(ApiConfig {
        token: Some("s3cr3t".to_string()),
        organism: Some(1),
        threads: 2,
        ..ApiConfig::default()
    });

    // liveness é público
    let (st, _) = request(addr, "GET", "/health", "");
    assert_eq!(st, 200);

    // sem token: 401
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &remember_body("1", 1));
    assert_eq!(st, 401, "{resp}");
    assert!(resp.contains("unauthorized"), "{resp}");

    // token errado: 401
    let (st, _) = request_h(
        addr,
        "POST",
        "/v1/aether/remember",
        &remember_body("1", 1),
        &[("authorization", "Bearer nope")],
    );
    assert_eq!(st, 401);

    // token certo, organismo errado: 403
    let (st, resp) = request_h(
        addr,
        "POST",
        "/v1/aether/remember",
        &remember_body("1", 2),
        &[("authorization", "Bearer s3cr3t")],
    );
    assert_eq!(st, 403, "{resp}");
    assert!(resp.contains("forbidden_scope"), "{resp}");

    // token certo, organismo certo: 200
    let (st, resp) = request_h(
        addr,
        "POST",
        "/v1/aether/remember",
        &remember_body("1", 1),
        &[("authorization", "Bearer s3cr3t")],
    );
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"stored\":true"), "{resp}");
}

#[test]
fn persistencia_em_disco_sobrevive_a_reinicio() {
    let path = std::env::temp_dir().join(format!(
        "aether_api_ledger_{}_{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    let cfg = ApiConfig {
        ledger_path: Some(PathBuf::from(&path)),
        threads: 2,
        ..ApiConfig::default()
    };

    let a = start_cfg(cfg.clone());
    let (st, resp) = request(a, "POST", "/v1/aether/remember", &remember_body("99", 3));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"persisted\":true"), "{resp}");

    // "reinício": outro servidor, mesmo arquivo
    let b = start_cfg(cfg);
    let (st, resp) = request(b, "GET", "/health", "");
    assert_eq!(st, 200);
    assert!(resp.contains("\"ledger_entries\":1"), "{resp}");

    let query = format!(
        r#"{{"scope":{{"organism":3,"run":1,"app":1}},"query":{}}}"#,
        phases()
    );
    let (st, resp) = request(b, "POST", "/v1/aether/cue", &query);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"key\":\"99\""), "{resp}");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn concorrencia_atende_em_paralelo() {
    let addr = start();
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(std::thread::spawn(move || {
            request(addr, "GET", "/health", "").0
        }));
    }
    for h in handles {
        assert_eq!(h.join().unwrap(), 200);
    }
}
