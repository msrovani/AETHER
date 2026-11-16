//! Avaliação de **usabilidade como IA**: o que a borda acerta, o que ela
//! recusa e o que ela custa.
//!
//! O experimento é desenhado para **falsificar** a expectativa de que o
//! serviço "entende" texto. Ele mede quatro coisas:
//!
//! 1. memória exata (mesma string) → deve acertar;
//! 2. generalização (paráfrase, caixa, 1 caractere, prefixo) → deve **abster**,
//!    porque a borda texto→fase é hash, não embedding;
//! 3. tolerância numérica (mesmas fases + ruído) → deve acertar;
//! 4. custo (p50/p99 do `cue`, tempo do `solve`).
//!
//! ```text
//! cargo test --release --features api --test api_eval -- --nocapture
//! ```

#![cfg(feature = "api")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use aether_engine::api::{self, ApiConfig};

fn start() -> SocketAddr {
    let listener = api::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let _ = api::serve_listener(
            listener,
            ApiConfig {
                threads: 4,
                ..ApiConfig::default()
            },
        );
    });
    addr
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

fn f32_json(v: &[f32]) -> String {
    let s: Vec<String> = v.iter().map(|x| format!("{x}")).collect();
    format!("[{}]", s.join(","))
}

fn fact(i: usize) -> String {
    format!("fato {i} sobre o tema {} com valor {}", i % 5, i * 7)
}

fn percentile(sorted: &[u128], p: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

#[test]
fn avaliacao_de_usabilidade() {
    let addr = start();
    let scope = r#"{"organism":1,"run":1,"app":1}"#;
    let n = 60usize;

    // ---- 1. população ------------------------------------------------------
    for i in 0..n {
        let body = format!(
            r#"{{"key":"{i}","phases":{},"scope":{scope},
                "certificate":{{"energy":-2.0,"gap":0.3,"violations":0}},
                "authority":255,"tick":{},"wall_ms":{}}}"#,
            f32_json(&api::text_to_phases(&fact(i))),
            i,
            1_700_000_000_000u64 + i as u64
        );
        let (st, resp) = request(addr, "POST", "/v1/aether/remember", &body);
        assert_eq!(st, 200, "{resp}");
    }

    // ---- 2. memória exata --------------------------------------------------
    let mut exact_ok = 0usize;
    let mut exact_lat = Vec::new();
    for i in 0..n {
        let q = format!(
            r#"{{"scope":{scope},"query":{}}}"#,
            f32_json(&api::text_to_phases(&fact(i)))
        );
        let t0 = Instant::now();
        let (st, resp) = request(addr, "POST", "/v1/aether/cue", &q);
        exact_lat.push(t0.elapsed().as_micros());
        assert_eq!(st, 200);
        if !resp.contains("\"abstain\":true") && resp.contains(&format!("\"key\":\"{i}\"")) {
            exact_ok += 1;
        }
    }

    // ---- 3. ruído numérico (mesmas fases + 1e-3) ---------------------------
    let mut noise_ok = 0usize;
    for i in 0..n {
        let mut p = api::text_to_phases(&fact(i));
        for (k, v) in p.iter_mut().enumerate() {
            *v += ((k as f32) * 0.001).sin() * 1e-3;
        }
        let q = format!(r#"{{"scope":{scope},"query":{}}}"#, f32_json(&p));
        let (st, resp) = request(addr, "POST", "/v1/aether/cue", &q);
        assert_eq!(st, 200);
        if !resp.contains("\"abstain\":true") && resp.contains(&format!("\"key\":\"{i}\"")) {
            noise_ok += 1;
        }
    }

    // ---- 4. generalização que NÃO deve existir ----------------------------
    let variants: [(&str, fn(&str) -> String); 4] = [
        ("paráfrase", |f| format!("{f}?")),
        ("maiúsculas", |f| f.to_uppercase()),
        ("1 caractere", |f| format!("{f}x")),
        ("prefixo", |f| f.split_whitespace().take(2).collect::<Vec<_>>().join(" ")),
    ];
    let mut results: Vec<(&str, usize)> = Vec::new();
    for (name, f) in variants {
        let mut abstained = 0usize;
        for i in 0..n {
            let q = format!(
                r#"{{"scope":{scope},"query":{}}}"#,
                f32_json(&api::text_to_phases(&f(&fact(i))))
            );
            let (st, resp) = request(addr, "POST", "/v1/aether/cue", &q);
            assert_eq!(st, 200);
            if resp.contains("\"abstain\":true") {
                abstained += 1;
            }
        }
        results.push((name, abstained));
    }

    // escopo alheio
    let mut cross_abstained = 0usize;
    for i in 0..n {
        let q = format!(
            r#"{{"scope":{{"organism":2,"run":1,"app":1}},"query":{}}}"#,
            f32_json(&api::text_to_phases(&fact(i)))
        );
        let (_, resp) = request(addr, "POST", "/v1/aether/cue", &q);
        if resp.contains("\"abstain\":true") {
            cross_abstained += 1;
        }
    }

    // ---- 5. dialeto OpenAI (o que um app realmente chama) ------------------
    let chat = r#"{"model":"aether-phase-160","aether":{"scope":{"organism":1,"run":1,"app":1}},
                   "messages":[{"role":"user","content":"fato 7 sobre o tema 2 com valor 49"}]}"#;
    let (st, chat_resp) = request(addr, "POST", "/v1/chat/completions", chat);
    assert_eq!(st, 200, "{chat_resp}");
    let chat_ok = chat_resp.contains("\"finish_reason\":\"stop\"") && chat_resp.contains("\"key\":\"7\"");

    // ---- 6. custo do solver ------------------------------------------------
    let mut solve_lat = Vec::new();
    for seed in 0..5u64 {
        let body = format!(
            r#"{{"n":12,"j":[[{}]],"h":[{}],"steps":80,"restarts":4,"seed":{seed}}}"#,
            (0..12)
                .map(|i| (0..12)
                    .map(|k| if i == k { "0".to_string() } else if i < k { "1".to_string() } else { "1".to_string() })
                    .collect::<Vec<_>>()
                    .join(","))
                .collect::<Vec<_>>()
                .join("],["),
            vec!["0"; 12].join(",")
        );
        let t0 = Instant::now();
        let (st, resp) = request(addr, "POST", "/v1/aether/solve", &body);
        solve_lat.push(t0.elapsed().as_micros());
        assert_eq!(st, 200, "{resp}");
    }

    // ---- 7. relatório ------------------------------------------------------
    let mut lat = exact_lat.clone();
    lat.sort_unstable();
    let (p50, p99) = (percentile(&lat, 0.50), percentile(&lat, 0.99));
    let mut sl = solve_lat.clone();
    sl.sort_unstable();

    println!("\n== AETHER · avaliação de usabilidade (n={n} fatos, 160 fases) ==");
    println!("memória exata (mesma string)      : {exact_ok}/{n}");
    println!("ruído 1e-3 nas fases              : {noise_ok}/{n}");
    for (name, abst) in &results {
        println!("{name:<33} : abstém {abst}/{n}");
    }
    println!("escopo alheio                     : abstém {cross_abstained}/{n}");
    println!("chat OpenAI com fato exato        : {}", if chat_ok { "act + key=7" } else { "FALHOU" });
    println!("latência cue p50/p99              : {p50} µs / {p99} µs");
    println!(
        "latência solve n=12 p50/p99       : {} µs / {} µs",
        percentile(&sl, 0.50),
        percentile(&sl, 0.99)
    );

    // ---- 8. o contrato que o teste cobra -----------------------------------
    assert_eq!(exact_ok, n, "memória exata tem de acertar 100%");
    assert_eq!(noise_ok, n, "fases com ruído 1e-3 têm de acertar");
    for (name, abst) in &results {
        assert_eq!(*abst, n, "{name} não pode acertar: a borda é hash, não embedding");
    }
    assert_eq!(cross_abstained, n, "R8: escopo alheio não vaza");
    assert!(chat_ok, "dialeto OpenAI tem de citar o fato exato");
    assert!(p50 < 20_000, "p50 do cue estourou: {p50} µs");
    assert!(p99 < 100_000, "p99 do cue estourou: {p99} µs");
}
