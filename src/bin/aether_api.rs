//! `aether-api` — borda HTTP/JSON do AETHER (T2).
//!
//! ```text
//! aether-api [--addr IP:PORTA] [--ledger arquivo.jsonl] [--token T]
//!            [--organism N] [--threads N]
//! ```
//!
//! Nenhuma dependência nova: HTTP/1.1 mínimo sobre `std::net`.

use std::net::SocketAddr;
use std::path::PathBuf;

use aether_engine::api::{self, ApiConfig};

fn usage() {
    println!(
        "aether-api {}\n\
         uso: aether-api [endereço] [--addr IP:PORTA] [--ledger arquivo.jsonl]\n\
         \x20               [--token T] [--organism N] [--threads N]\n\n\
         rotas: GET /health /props /v1/models\n\
         \x20      POST /v1/aether/{{remember,cue,solve}} /v1/chat/completions /completion\n\
         exemplos:\n\
         \x20 aether-api 127.0.0.1:8787 --ledger target/aether_ledger.jsonl\n\
         \x20 aether-api 127.0.0.1:8787 --token s3cr3t --organism 1\n\
         \x20 OPENAI_BASE_URL=http://127.0.0.1:8787/v1",
        api::VERSION
    );
}

fn parse_addr(raw: &str) -> std::io::Result<SocketAddr> {
    raw.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("endereço inválido: {raw} (use ip:porta, ex. 127.0.0.1:8787)"),
        )
    })
}

fn parse_u64(raw: &str, flag: &str) -> std::io::Result<u64> {
    raw.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{flag} espera inteiro, veio: {raw}"),
        )
    })
}

fn main() -> std::io::Result<()> {
    let mut cfg = ApiConfig::default();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut i = 0usize;
    while i < args.len() {
        let arg = args[i].clone();
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        let value = |i: &mut usize| -> std::io::Result<String> {
            if let Some(v) = &inline {
                return Ok(v.clone());
            }
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{flag} exige um valor"),
                )
            })
        };
        match flag.as_str() {
            "-h" | "--help" => {
                usage();
                return Ok(());
            }
            "--addr" => cfg.addr = parse_addr(&value(&mut i)?)?,
            "--ledger" => cfg.ledger_path = Some(PathBuf::from(value(&mut i)?)),
            "--token" => cfg.token = Some(value(&mut i)?),
            "--organism" => cfg.organism = Some(parse_u64(&value(&mut i)?, "--organism")?),
            "--threads" => {
                cfg.threads = parse_u64(&value(&mut i)?, "--threads")?.max(1) as usize;
            }
            other if !other.starts_with("--") && i == 0 => cfg.addr = parse_addr(other)?,
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("argumento desconhecido: {other} (use --help)"),
                ))
            }
        }
        i += 1;
    }

    let listener = api::bind(cfg.addr)?;
    let local = listener.local_addr()?;
    println!("aether-api {} em http://{local}", api::VERSION);
    if let Some(p) = &cfg.ledger_path {
        println!("  ledger: {}", p.display());
    }
    if cfg.token.is_some() {
        println!(
            "  auth: token exigido{}",
            match cfg.organism {
                Some(o) => format!(" (organismo {o})"),
                None => String::new(),
            }
        );
    }
    println!("  openai: OPENAI_BASE_URL=http://{local}/v1");
    println!("  threads: {}", cfg.threads.max(1));
    api::serve_listener(listener, cfg)
}

