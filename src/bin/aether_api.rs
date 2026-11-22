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
         \x20               [--token T] [--organism N] [--threads N]\n\
         \x20               [--sgdb mock|http://host:porta/mcp|stdio:cmd]\n\
         \x20               [--sgdb-outbox arquivo.jsonl] [--sgdb-token T]\n\n\
         rotas: GET /health /props /v1/models\n\
         \x20      POST /v1/aether/{{remember,cue,solve}} /v1/chat/completions /completion\n\
         exemplos:\n\
         \x20 aether-api 127.0.0.1:8787 --ledger target/aether_ledger.jsonl\n\
         \x20 aether-api 127.0.0.1:8787 --token s3cr3t --organism 1\n\
         \x20 aether-api 127.0.0.1:8787 --sgdb http://127.0.0.1:8090/mcp \\\n\
         \x20            --sgdb-outbox target/aether_sgdb.jsonl\n\
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
    // Memória do chat: carrega `.env` (AETHER_SGDB_*) sem sobrescrever o ambiente.
    load_repo_dotenv();
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
            "--ledger-max-bytes" => {
                cfg.max_ledger_bytes = parse_u64(&value(&mut i)?, "--ledger-max-bytes")?;
            }
            "--token" => cfg.token = Some(value(&mut i)?),
            "--organism" => cfg.organism = Some(parse_u64(&value(&mut i)?, "--organism")?),
            "--threads" => {
                cfg.threads = parse_u64(&value(&mut i)?, "--threads")?.max(1) as usize;
            }
            "--sgdb" => cfg.sgdb = Some(value(&mut i)?),
            "--sgdb-outbox" => cfg.sgdb_outbox = Some(PathBuf::from(value(&mut i)?)),
            "--sgdb-token" => cfg.sgdb_token = Some(value(&mut i)?),
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

    // Sem --sgdb: herda AETHER_SGDB_STDIO/URL do .env (comando solto → stdio:…).
    if cfg.sgdb.is_none() {
        for key in ["AETHER_SGDB_STDIO", "AETHER_SGDB_URL"] {
            if let Ok(spec) = std::env::var(key) {
                let s = spec.trim();
                if s.is_empty() {
                    continue;
                }
                cfg.sgdb = Some(normalize_sgdb_spec(s));
                break;
            }
        }
    } else if let Some(spec) = cfg.sgdb.take() {
        cfg.sgdb = Some(normalize_sgdb_spec(&spec));
    }
    if cfg.sgdb_outbox.is_none() {
        if let Ok(p) = std::env::var("AETHER_SGDB_OUTBOX") {
            let p = p.trim();
            if !p.is_empty() {
                cfg.sgdb_outbox = Some(PathBuf::from(p));
            }
        }
    }

    let listener = api::bind(cfg.addr)?;
    let local = listener.local_addr()?;
    println!("aether-api {} em http://{local}", api::VERSION);
    if let Some(p) = &cfg.ledger_path {
        println!("  ledger: {}", p.display());
        if cfg.max_ledger_bytes > 0 && cfg.max_ledger_bytes < 16_384 {
            println!(
                "  aviso: --ledger-max-bytes={} é menor que ~3 registros (~6 KB cada); \
                 a compactação vai rodar a cada escrita",
                cfg.max_ledger_bytes
            );
        }
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
    if let Some(spec) = &cfg.sgdb {
        println!(
            "  sgdb: {spec} (replica remember/cue{})",
            cfg.sgdb_outbox
                .as_ref()
                .map(|p| format!(", outbox {}", p.display()))
                .unwrap_or_default()
        );
    }
    println!("  openai: OPENAI_BASE_URL=http://{local}/v1");
    println!("  threads: {}", cfg.threads.max(1));
    api::serve_listener(listener, cfg)
}

/// Prefixa `stdio:` quando o `.env` traz comando solto (MSG 1056).
fn normalize_sgdb_spec(spec: &str) -> String {
    let s = spec.trim();
    if s.is_empty() {
        return s.to_string();
    }
    if s == "mock"
        || s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("stdio:")
    {
        return s.to_string();
    }
    format!("stdio:{s}")
}

/// Carrega `.env` na raiz do repo sem sobrescrever variáveis já definidas.
fn load_repo_dotenv() {
    let mut dir = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return,
    };
    for _ in 0..8 {
        let cargo = dir.join("Cargo.toml");
        let env_path = dir.join(".env");
        if cargo.is_file() {
            if let Ok(text) = std::fs::read_to_string(&env_path) {
                for raw in text.lines() {
                    let line = raw.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let Some((key, value)) = line.split_once('=') else {
                        continue;
                    };
                    let key = key.trim();
                    if key.is_empty() || std::env::var_os(key).is_some() {
                        continue;
                    }
                    let mut val = value.trim().to_string();
                    if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
                        || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
                    {
                        val = val[1..val.len() - 1].to_string();
                    }
                    std::env::set_var(key, val);
                }
            }
            return;
        }
        if !dir.pop() {
            break;
        }
    }
}
