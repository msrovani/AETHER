//! `aether-sgdb` — CLI do encaixe `neural-sgdb` (`T2`).
//!
//! Toda a lógica (parse, comandos, saída) vive na biblioteca
//! (`aether_engine::knowledge::sgdb::cli`) para ser testável sem processo.
//! Este binário só a chama e propaga o código de saída.
//!
//! ```text
//! aether-sgdb doctor
//! aether-sgdb put "fato durável" --entity aether/f4
//! aether-sgdb recall "fato" --json
//! aether-sgdb sync --outbox target/aether_sgdb.jsonl
//! ```

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out = std::io::stdout().lock();
    let code = aether_engine::knowledge::sgdb::cli::run(&args, &mut out);
    let _ = out.flush();
    std::process::exit(code);
}
