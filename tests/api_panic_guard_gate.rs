//! Gate **R1** — contrato de resiliência da borda (`catch_unwind` × perfil).
//!
//! Duas verdades documentadas (ver auditoria 2026-11-27, A4.1):
//! 1. `catch_unwind` só captura panics que fazem *unwind* — sob `panic =
//!    "abort"` ele não captura nada. Por isso a borda tem o perfil dedicado
//!    `[profile.release-unwind]` no `Cargo.toml`: `aether-api`/`aether-daemon`
//!    empacotados rodam com unwind e o guard de `serve_listener` é REAL.
//! 2. A suíte de testes sempre faz unwind (Cargo ignora `panic` em perfis de
//!    test), então este gate pode exercitar o guard sem risco de abort.
//!
//! Cobertura: (a) o mecanismo captura de fato; (b) requests malformados não
//! derrubam o loop do servidor — `/health` responde depois deles.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use aether_engine::api::{self, ApiConfig};

#[test]
fn catch_unwind_captura_panic_no_perfil_unwind() {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        panic!("boom-controlado (gate R1)");
    }));
    assert!(
        outcome.is_err(),
        "catch_unwind devia capturar o panic (perfil de test sempre faz unwind)"
    );
}

#[test]
fn request_malformado_nao_derruba_a_borda() {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, ApiConfig::default());
    });

    // 1) linha de request vazia (sem método/versão).
    let mut s = TcpStream::connect(addr).expect("connect 1");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = s.write_all(b"\r\n");
    let _ = s.read(&mut [0u8; 64]);
    drop(s);

    // 2) header truncado no meio (cliente fecha a conexão).
    let mut s2 = TcpStream::connect(addr).expect("connect 2");
    s2.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = s2.write_all(b"GET /health HTTP/1.1\r\nhost: x");
    drop(s2);

    // 3) corpo menor que o content-length declarado → read_exact curto → Err.
    let mut s3 = TcpStream::connect(addr).expect("connect 3");
    s3.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ =
        s3.write_all(b"POST /v1/aether/cue HTTP/1.1\r\nhost: x\r\ncontent-length: 100\r\n\r\n{}");
    drop(s3);

    // O servidor segue vivo e responde saúde normal.
    let mut s4 = TcpStream::connect(addr).expect("connect 4");
    s4.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s4.write_all(b"GET /health HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n")
        .expect("write health");
    let mut resp = String::new();
    s4.read_to_string(&mut resp).expect("health após ataques");
    assert!(resp.contains("200"), "borda morreu: {resp}");
}
