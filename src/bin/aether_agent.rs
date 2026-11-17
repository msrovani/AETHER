//! `aether-agent` — CLI do organismo agêntico.
//!
//! A lógica vive em `aether_engine::agent::cli` (testável sem processo); este
//! binário só chama e propaga o código de saída.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let code = aether_engine::agent::cli::run(&args, &mut out);
    let _ = out.flush();
    std::process::exit(code);
}
