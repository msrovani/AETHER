//! `aether-api` — borda HTTP/JSON do AETHER (T2).
//!
//! Sobe o servidor compatível com o dialeto da OpenAI e com o do llama.cpp.
//! Nenhuma dependência nova: HTTP/1.1 mínimo sobre `std::net`.
//!
//! ```text
//! cargo run --release --features api --bin aether-api -- 127.0.0.1:8787
//! curl http://127.0.0.1:8787/health
//! ```
//!
//! App externo (llama.cpp / Ollama / SDK):
//!
//! ```text
//! OPENAI_BASE_URL=http://127.0.0.1:8787/v1
//! ```

use std::net::SocketAddr;

use aether_engine::api;

fn main() -> std::io::Result<()> {
    let raw = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8787".to_string());
    let addr: SocketAddr = raw.parse().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("endereço inválido: {raw} (use ip:porta, ex. 127.0.0.1:8787)"),
        )
    })?;

    let listener = api::bind(addr)?;
    let local = listener.local_addr()?;
    println!("aether-api {} em http://{local}", api::VERSION);
    println!("  OpenAI:  OPENAI_BASE_URL=http://{local}/v1");
    println!("  llama.cpp: POST http://{local}/completion");
    println!("  nativo:  /v1/aether/{{remember,cue,solve}}");
    api::serve(listener)
}
