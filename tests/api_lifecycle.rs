//! Ciclo de vida do fato: esquecer (túmulo), compactação do ledger e índice
//! por escopo. Fecha os três itens deixados em aberto na avaliação da API.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aether_engine::api::{self, ApiConfig};

fn start_cfg(cfg: ApiConfig) -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(listener, cfg);
    });
    addr
}

fn start() -> SocketAddr {
    start_cfg(ApiConfig {
        threads: 2,
        ..ApiConfig::default()
    })
}

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nhost: localhost\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).expect("write");
    let mut resp = String::new();
    s.read_to_string(&mut resp).expect("read");
    let (head, payload) = resp.split_once("\r\n\r\n").expect("separador");
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, payload.to_string())
}

fn phases() -> String {
    let v = api::text_to_phases("fato");
    let s: Vec<String> = v.iter().map(|p| format!("{p}")).collect();
    format!("[{}]", s.join(","))
}

fn remember_body(key: u64, organism: u64) -> String {
    format!(
        r#"{{"key":"{key}","phases":{},"scope":{{"organism":{organism},"run":1,"app":1}},
            "certificate":{{"energy":-1.0,"gap":0.4,"violations":0}},"authority":255}}"#,
        phases()
    )
}

fn forget_body(key: u64, organism: u64, authority: u64) -> String {
    format!(
        r#"{{"key":"{key}","scope":{{"organism":{organism},"run":1,"app":1}},
            "authority":{authority},"reason":"teste"}}"#
    )
}

fn cue_body(organism: u64) -> String {
    format!(
        r#"{{"scope":{{"organism":{organism},"run":1,"app":1}},"query":{}}}"#,
        phases()
    )
}

fn temp_file(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aether_life_{tag}_{}_{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

#[test]
fn forget_remove_torna_o_fato_nao_citavel() {
    let addr = start();
    assert_eq!(
        request(addr, "POST", "/v1/aether/remember", &remember_body(1, 1)).0,
        200
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert_eq!(st, 200);
    assert!(resp.contains("\"key\":\"1\""), "{resp}");

    // Esquecer exige mandato.
    let (st, resp) = request(addr, "POST", "/v1/aether/forget", &forget_body(1, 1, 0));
    assert_eq!(st, 403, "{resp}");
    assert!(resp.contains("authority_required"), "{resp}");

    let (st, resp) = request(addr, "POST", "/v1/aether/forget", &forget_body(1, 1, 255));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"forgot\":true"), "{resp}");

    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"abstain\":true"), "ainda cita: {resp}");

    // Idempotente: o segundo forget não inventa sucesso.
    let (st, resp) = request(addr, "POST", "/v1/aether/forget", &forget_body(1, 1, 255));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"forgot\":false"), "{resp}");
}

#[test]
fn forget_nao_alcanca_outro_organismo() {
    let addr = start();
    assert_eq!(
        request(addr, "POST", "/v1/aether/remember", &remember_body(7, 1)).0,
        200
    );
    // Tentar esquecer o fato de 1 pelo escopo de 2: não acha, não remove.
    let (st, resp) = request(addr, "POST", "/v1/aether/forget", &forget_body(7, 2, 255));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"forgot\":false"), "{resp}");

    let (_, resp) = request(addr, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"key\":\"7\""), "fato de 1 sumiu: {resp}");
}

#[test]
fn tumulo_sobrevive_ao_reinicio() {
    let file = temp_file("tomb");
    let _ = std::fs::remove_file(&file);
    let cfg = ApiConfig {
        ledger_path: Some(file.clone()),
        threads: 2,
        ..ApiConfig::default()
    };

    let a = start_cfg(cfg.clone());
    assert_eq!(
        request(a, "POST", "/v1/aether/remember", &remember_body(3, 1)).0,
        200
    );
    let (st, resp) = request(a, "POST", "/v1/aether/forget", &forget_body(3, 1, 255));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"tombstoned\":true"), "{resp}");

    // Reinício: o túmulo é reaplicado, o fato não volta.
    let b = start_cfg(cfg);
    let (_, health) = request(b, "GET", "/health", "");
    assert!(health.contains("\"ledger_entries\":0"), "{health}");
    let (_, resp) = request(b, "POST", "/v1/aether/cue", &cue_body(1));
    assert!(resp.contains("\"abstain\":true"), "ressuscitou: {resp}");

    let _ = std::fs::remove_file(&file);
}

#[test]
fn compactacao_descarta_tumulos_e_morto() {
    let file = temp_file("compact");
    let _ = std::fs::remove_file(&file);
    let cfg = ApiConfig {
        ledger_path: Some(file.clone()),
        max_ledger_bytes: 4096,
        threads: 2,
        ..ApiConfig::default()
    };
    let addr = start_cfg(cfg.clone());

    // Cada linha tem ~2,6 KB. Sem compactação, 10 fatos + 8 túmulos passariam
    // de 26 KB e ficariam lá para sempre.
    for k in 0..10u64 {
        let (st, resp) = request(addr, "POST", "/v1/aether/remember", &remember_body(k, 1));
        assert_eq!(st, 200, "{resp}");
    }
    for k in 0..8u64 {
        let (st, resp) = request(addr, "POST", "/v1/aether/forget", &forget_body(k, 1, 255));
        assert_eq!(st, 200, "{resp}");
        assert!(resp.contains("\"tombstoned\":true"), "{resp}");
    }
    let before = std::fs::metadata(&file).expect("arquivo").len();

    // Um novo fato empurra o arquivo acima do teto e dispara a compactação.
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &remember_body(100, 1));
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"compacted\":true"), "{resp}");
    let after = std::fs::metadata(&file).expect("arquivo").len();
    assert!(
        after < before,
        "compactação não reduziu: {before} -> {after}"
    );
    // Um registro de 160 fasores tem ~6 KB: 3 vivos cabem folgado em 24 KB.
    assert!(after < 24_000, "arquivo ainda grande: {after} bytes");

    // Sobraram exatamente os 3 vivos, inclusive após reinício.
    let b = start_cfg(cfg);
    let (_, health) = request(b, "GET", "/health", "");
    assert!(health.contains("\"ledger_entries\":3"), "{health}");
    assert!(health.contains("\"ledger_max_bytes\":4096"), "{health}");

    let _ = std::fs::remove_file(&file);
}

#[test]
fn indice_por_escopo_aguenta_muitos_fatos() {
    let file = temp_file("index");
    let _ = std::fs::remove_file(&file);
    let cfg = ApiConfig {
        ledger_path: Some(file.clone()),
        threads: 4,
        ..ApiConfig::default()
    };
    let addr = start_cfg(cfg);

    // 20 escopos × 100 fatos = 2.000 fatos; cue em um escopo não pode varrer
    // os outros 1.900.
    for org in 0..20u64 {
        for k in 0..100u64 {
            let body = format!(
                r#"{{"key":"{}","phases":{},"scope":{{"organism":{org},"run":1,"app":1}},
                    "authority":200}}"#,
                org * 1000 + k,
                phases()
            );
            let (st, _) = request(addr, "POST", "/v1/aether/remember", &body);
            assert_eq!(st, 200);
        }
    }

    // mesma fase em todo mundo: o vencedor tem de ser do escopo consultado
    let mut lat = Vec::new();
    let mut ok = 0usize;
    for org in 0..20u64 {
        let q = cue_body(org);
        let t0 = Instant::now();
        let (st, resp) = request(addr, "POST", "/v1/aether/cue", &q);
        lat.push(t0.elapsed().as_micros());
        assert_eq!(st, 200);
        if resp.contains(&format!("\"key\":\"{}", org * 1000)) || resp.contains("\"abstain\":false")
        {
            ok += 1;
        }
    }
    lat.sort_unstable();
    let p50 = lat[lat.len() / 2];
    println!("índice: 2.000 fatos, cue p50 = {p50} µs");
    assert_eq!(ok, 20, "todo escopo tem de responder do próprio escopo");
    assert!(
        p50 < 5_000,
        "cue ficou lento demais: {p50} µs (índice não aplicado?)"
    );

    let _ = std::fs::remove_file(&file);
}
