//! Gate **R9** — sharding do `ApiState`: I/O de rede do SGDB **fora** do lock
//! global.
//!
//! Antes da R9, um `remember` com SGDB configurado segurava o `Mutex<ApiState>`
//! durante a chamada de rede (timeout 1500 ms) — qualquer request em outra
//! conexão ficava bloqueado atrás dele (head-of-line blocking). Contrato novo:
//! durante o I/O do SGDB, `/health` responde normalmente.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

use aether_engine::api::{self, ApiConfig};

/// Servidor-fantasma: aceita conexões e **nunca responde** (rede lenta real).
fn dead_sgdb_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ghost");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            // Segura a conexão aberta (sem ler, sem responder).
            std::thread::sleep(Duration::from_secs(30));
            drop(s);
        }
    });
    port
}

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String, Duration) {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let started = Instant::now();
    s.write_all(
        format!(
            "{method} {path} HTTP/1.1\r\nhost: x\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .as_bytes(),
    )
    .expect("write");
    let mut resp = String::new();
    s.read_to_string(&mut resp).expect("read");
    let status: u16 = resp
        .split("\r\n")
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    (status, resp, started.elapsed())
}

fn phases() -> String {
    let v = api::text_to_phases("ola");
    v.iter()
        .map(|p| format!("{p}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn r9_sgdb_lento_nao_bloqueia_o_resto_da_borda() {
    let ghost_port = dead_sgdb_server();
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    let cfg = ApiConfig {
        sgdb: Some(format!("http://127.0.0.1:{ghost_port}/mcp")),
        ..ApiConfig::default()
    };
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });

    // Remember cuja réplica SGDB vai travar até o timeout do transporte.
    let body = format!(
        r#"{{"key":"9","phases":[{}],"scope":{{"organism":1,"run":1,"app":1}},
            "certificate":{{"energy":-2.0,"gap":0.3,"violations":0}}}}"#,
        phases()
    );
    let t0 = Instant::now();
    let h = std::thread::spawn(move || request(addr, "POST", "/v1/aether/remember", &body));

    // Enquanto o SGDB está lento, /health responde rápido (fora do lock).
    std::thread::sleep(Duration::from_millis(250));
    let (st, resp, dt) = request(addr, "GET", "/health", "");
    assert_eq!(st, 200, "{resp}");
    assert!(
        dt < Duration::from_millis(1500),
        "/health bloqueada ({dt:?}) — rede do SGDB voltou ao lock global"
    );

    // O remember termina com SGDB offline honesto.
    let (st, resp, _dt) = h.join().expect("remember join");
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"sgdb\":\"offline\""), "{resp}");
    assert!(
        t0.elapsed() >= Duration::from_millis(1000),
        "timeout do SGDB curto demais ({:?}); o gate assume rede travada",
        t0.elapsed()
    );
}

#[test]
fn r9_health_mostra_status_do_shard() {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    let cfg = ApiConfig::default(); // sem sgdb → off
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });
    let (st, resp, _) = request(addr, "GET", "/health", "");
    assert_eq!(st, 200);
    assert!(resp.contains("\"sgdb\":\"off\""), "{resp}");
}
