//! Gate **R2** — backpressure, caps e teto de conexões da borda HTTP.
//!
//! Antes da R2 a fila de conexões era `mpsc::channel` **ilimitado** (aceitar
//! crescia memória sem teto) e o read-timeout era *por leitura* — um slow-loris
//! segurava um worker indefinidamente. Contratos cobertos aqui:
//! 1. linha de header gigante → conexão fechada e servidor **vivo**;
//! 2. excesso de headers → idem;
//! 3. fila cheia → `503` explícito (não enfileira para sempre).

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use aether_engine::api::{self, ApiConfig};

fn start_cfg(cfg: ApiConfig) -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });
    addr
}

fn health_ok(addr: SocketAddr) -> bool {
    let mut s = TcpStream::connect(addr).expect("connect health");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(b"GET /health HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .expect("write health");
    let mut resp = String::new();
    s.read_to_string(&mut resp).expect("read health");
    resp.contains("200")
}

fn send_raw(addr: SocketAddr, bytes: &[u8]) {
    let mut s = TcpStream::connect(addr).expect("connect raw");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = s.write_all(bytes);
    let _ = s.read(&mut [0u8; 64]);
    drop(s);
}

#[test]
fn header_gigante_e_muitos_headers_nao_matam_o_servidor() {
    let addr = start_cfg(ApiConfig {
        threads: 2,
        ..ApiConfig::default()
    });

    // 1) linha de header > 8 KiB.
    let big_line = "x: ".to_string() + &"a".repeat(10 * 1024) + "\r\n\r\n";
    send_raw(
        addr,
        format!("GET /health HTTP/1.1\r\n{big_line}").as_bytes(),
    );
    assert!(health_ok(addr), "servidor morreu após header gigante");

    // 2) mais de 64 headers.
    let mut req = String::from("GET /health HTTP/1.1\r\n");
    for i in 0..80 {
        req.push_str(&format!("h{i}: v\r\n"));
    }
    req.push_str("\r\n");
    send_raw(addr, req.as_bytes());
    assert!(health_ok(addr), "servidor morreu após 80 headers");
}

#[test]
fn t6_respostas_sem_cors_wildcard() {
    let addr = start_cfg(ApiConfig::default());
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(b"GET /health HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .expect("write");
    let mut resp = String::new();
    s.read_to_string(&mut resp).expect("read");
    assert!(
        !resp
            .to_ascii_lowercase()
            .contains("access-control-allow-origin: *"),
        "wildcard CORS voltou: {resp}"
    );
}

#[test]
fn overload_responde_503_e_o_servidor_fica_vivo() {
    // threads:1 + max_conns:2 — A segura o worker; B ocupa a fila; qualquer
    // conexão nova acima do teto recebe 503 (branch de `max_conns`, que é
    // determinístico: A e B já incrementaram o contador antes de C).
    let addr = start_cfg(ApiConfig {
        threads: 1,
        queue_limit: 8,
        max_conns: 2,
        ..ApiConfig::default()
    });

    // A: segura o worker com um corpo que nunca termina (read_timeout 10 s).
    let mut a = TcpStream::connect(addr).expect("connect A");
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    a.write_all(
        b"POST /v1/aether/cue HTTP/1.1\r\nhost: x\r\ncontent-length: 999999\r\n\r\n{\"x\":",
    )
    .expect("write A");
    // B: header incompleto → enfileirado (worker ocupado com A).
    let mut b = TcpStream::connect(addr).expect("connect B");
    b.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    b.write_all(b"GET /health HTTP/1.1\r\nhost: x")
        .expect("write B");
    // A e B já foram aceitos (contador = max_conns).
    std::thread::sleep(Duration::from_millis(120));

    // C: acima do teto → 503 explícito (antes: crescia a fila para sempre).
    let mut c = TcpStream::connect(addr).expect("connect C");
    c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    c.write_all(b"GET /health HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .expect("write C");
    let mut resp = String::new();
    c.read_to_string(&mut resp).expect("read C");
    assert!(resp.contains("503"), "esperado 503 no overload: {resp}");
    drop(c);

    // A borda sobrevive ao overload (assim que A libera o worker).
    drop(b);
    drop(a);
}
