//! Bateria de **integridade** da borda externa: protocolo hostil, limites,
//! precisão de chave, persistência (inclusive corrompida e concorrente) e
//! concorrência. Nada aqui depende de cliente HTTP.

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use aether_engine::api::json::Json;
use aether_engine::api::{self, ApiConfig};

fn start() -> SocketAddr {
    start_cfg(ApiConfig {
        threads: 4,
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
    s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
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
    split(resp)
}

fn request(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    request_h(addr, method, path, body, &[])
}

fn split(resp: String) -> (u16, String) {
    let (head, payload) = resp.split_once("\r\n\r\n").expect("separador http");
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, payload.to_string())
}

fn phases_text(n: usize) -> String {
    let v: Vec<String> = (0..n).map(|i| format!("{}", (i as f32 * 0.37).sin())).collect();
    format!("[{}]", v.join(","))
}

fn temp_file(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aether_api_{tag}_{}_{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

#[test]
fn protocolo_hostil_nao_derruba_o_servidor() {
    let addr = start();

    // método errado em rota existente
    let (st, _) = request(addr, "PUT", "/health", "");
    assert_eq!(st, 404);

    // JSON truncado
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", "{");
    assert_eq!(st, 400);
    assert!(resp.contains("bad_json"), "{resp}");

    // JSON válido com tipo errado
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", "[]");
    assert_eq!(st, 400);
    assert!(resp.contains("scope ausente"), "{resp}");

    // corpo vazio
    let (st, _) = request(addr, "POST", "/v1/aether/cue", "");
    assert_eq!(st, 400);

    // rota inexistente e caminho absurdo
    assert_eq!(request(addr, "GET", "/nao-existe", "").0, 404);
    let long = format!("/{}", "a".repeat(4000));
    assert_eq!(request(addr, "GET", &long, "").0, 404);

    // o servidor continua vivo depois de tudo
    assert_eq!(request(addr, "GET", "/health", "").0, 200);
}

#[test]
fn requisicao_sem_content_length_nao_pendura() {
    let addr = start();
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(
        b"POST /v1/aether/remember HTTP/1.1\r\nhost: l\r\nconnection: close\r\n\r\n{\"key\":\"1\"}",
    )
    .unwrap();
    let mut resp = String::new();
    let _ = s.read_to_string(&mut resp);
    let (st, _) = split(resp);
    assert_eq!(st, 400, "sem content-length o corpo é vazio: 400, não timeout");
}

#[test]
fn corpo_acima_do_limite_da_413() {
    let addr = start();
    let huge = format!(
        r#"{{"key":"1","phases":[{}],"scope":{{"organism":1}}}}"#,
        "1.0,".repeat(300_000)
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &huge);
    assert_eq!(st, 413, "{resp}");
    assert!(resp.contains("payload_too_large"), "{resp}");
}

#[test]
fn nao_finitos_sao_recusados() {
    let addr = start();
    let body = format!(
        r#"{{"key":"1","phases":[1e999,{}],"scope":{{"organism":1}}}}"#,
        "0.5,".repeat(158)
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &body);
    assert_eq!(st, 400, "{resp}");

    let solve = format!(
        r#"{{"n":2,"j":[[0,1e999],[1e999,0]],"h":[0,0]}}"#
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/solve", &solve);
    assert_eq!(st, 400, "{resp}");
}

#[test]
fn bom_utf8_e_aceito() {
    let addr = start();
    let body = format!(
        "\u{feff}{}",
        format!(
            r#"{{"key":"5","phases":{},"scope":{{"organism":1,"run":1,"app":1}}}}"#,
            phases_text(160)
        )
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &body);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains("\"stored\":true"), "{resp}");
}

#[test]
fn chave_u128_e_fases_preservadas_bit_a_bit() {
    let file = temp_file("bits");
    let _ = std::fs::remove_file(&file);
    let cfg = ApiConfig {
        ledger_path: Some(file.clone()),
        threads: 2,
        ..ApiConfig::default()
    };
    let addr = start_cfg(cfg);

    let phases: Vec<f32> = (0..160).map(|i| (i as f32 * 0.37).sin()).collect();
    let list: Vec<String> = phases.iter().map(|p| format!("{p}")).collect();
    let key = u128::MAX;
    let body = format!(
        r#"{{"key":"{key}","phases":[{}],"scope":{{"organism":1,"run":1,"app":1}},
            "certificate":{{"energy":-1.25,"gap":0.5,"violations":2}},"authority":255,
            "tick":11,"wall_ms":22}}"#,
        list.join(",")
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/remember", &body);
    assert_eq!(st, 200, "{resp}");

    // O arquivo preserva os 160 f32 bit a bit (f32 → f64 → texto → f32).
    let text = std::fs::read_to_string(&file).expect("ledger");
    let line = text.lines().next().expect("linha");
    let doc = Json::parse(line).expect("json da linha");
    let saved = doc.get("phases").and_then(Json::as_arr).expect("phases");
    assert_eq!(saved.len(), 160);
    for (i, v) in saved.iter().enumerate() {
        let back = v.as_f64().expect("num") as f32;
        assert_eq!(back.to_bits(), phases[i].to_bits(), "fase {i} perdeu bits");
    }

    // E o servidor devolve a chave u128 inteira como string.
    let query = format!(
        r#"{{"scope":{{"organism":1,"run":1,"app":1}},"query":[{}]}}"#,
        list.join(",")
    );
    let (st, resp) = request(addr, "POST", "/v1/aether/cue", &query);
    assert_eq!(st, 200, "{resp}");
    assert!(resp.contains(&format!("\"key\":\"{key}\"")), "{resp}");
    assert!(resp.contains("\"abstain\":false"), "{resp}");

    let _ = std::fs::remove_file(&file);
}

#[test]
fn ledger_corrompido_carrega_o_que_e_valido() {
    let file = temp_file("corrupt");
    let _ = std::fs::remove_file(&file);
    let cfg = ApiConfig {
        ledger_path: Some(file.clone()),
        threads: 2,
        ..ApiConfig::default()
    };

    // Escreve dois fatos válidos por um servidor real.
    let a = start_cfg(cfg.clone());
    for k in 0..2 {
        let body = format!(
            r#"{{"key":"{k}","phases":{},"scope":{{"organism":1,"run":1,"app":1}}}}"#,
            phases_text(160)
        );
        assert_eq!(request(a, "POST", "/v1/aether/remember", &body).0, 200);
    }

    // Corrompe o arquivo: lixo, comentário e linha truncada.
    {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .expect("append");
        f.write_all(b"{ isto nao e json\n").unwrap();
        f.write_all(b"# comentario\n").unwrap();
        f.write_all(b"{\"key\":\"9\",\"phases\":[\n").unwrap();
    }

    // Novo servidor: carrega só o válido e conta as linhas ruins.
    // Comentários (`#`) são ignorados por projeto — não contam como ruins.
    let b = start_cfg(cfg);
    let (st, health) = request(b, "GET", "/health", "");
    assert_eq!(st, 200);
    assert!(health.contains("\"ledger_entries\":2"), "{health}");
    assert!(health.contains("\"bad_lines\":2"), "{health}");

    let query = format!(
        r#"{{"scope":{{"organism":1,"run":1,"app":1}},"query":{}}}"#,
        phases_text(160)
    );
    let (st, resp) = request(b, "POST", "/v1/aether/cue", &query);
    assert_eq!(st, 200, "{resp}");
    assert!(!resp.contains("\"abstain\":true"), "{resp}");

    let _ = std::fs::remove_file(&file);
}

#[test]
fn persistencia_sob_concorrencia_nao_perde_linha() {
    let file = temp_file("conc");
    let _ = std::fs::remove_file(&file);
    let cfg = ApiConfig {
        ledger_path: Some(file.clone()),
        threads: 8,
        ..ApiConfig::default()
    };
    let addr = start_cfg(cfg.clone());

    let mut handles = Vec::new();
    for k in 0..24u64 {
        let body = format!(
            r#"{{"key":"{k}","phases":{},"scope":{{"organism":1,"run":1,"app":1}}}}"#,
            phases_text(160)
        );
        handles.push(std::thread::spawn(move || {
            request(addr, "POST", "/v1/aether/remember", &body).0
        }));
    }
    for h in handles {
        assert_eq!(h.join().unwrap(), 200);
    }

    let text = std::fs::read_to_string(&file).expect("ledger");
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(lines, 24, "linhas no arquivo");

    let b = start_cfg(cfg);
    let (_, health) = request(b, "GET", "/health", "");
    assert!(health.contains("\"ledger_entries\":24"), "{health}");
    assert!(health.contains("\"bad_lines\":0"), "{health}");

    let _ = std::fs::remove_file(&file);
}

#[test]
fn concorrencia_mista_sem_erro() {
    let addr = start();
    let mut handles = Vec::new();
    for k in 0..8u64 {
        handles.push(std::thread::spawn(move || {
            let mut ok = 0usize;
            let body = format!(
                r#"{{"key":"{k}","phases":{},"scope":{{"organism":1,"run":1,"app":1}}}}"#,
                phases_text(160)
            );
            if request(addr, "POST", "/v1/aether/remember", &body).0 == 200 {
                ok += 1;
            }
            let query = format!(
                r#"{{"scope":{{"organism":1,"run":1,"app":1}},"query":{}}}"#,
                phases_text(160)
            );
            if request(addr, "POST", "/v1/aether/cue", &query).0 == 200 {
                ok += 1;
            }
            if request(addr, "GET", "/health", "").0 == 200 {
                ok += 1;
            }
            ok
        }));
    }
    for h in handles {
        assert_eq!(h.join().unwrap(), 3, "as três chamadas deveriam dar 200");
    }
}
