//! F5 — usabilidade da CLI `aether-sgdb` (processo real, não só a lib).
//!
//! Prova que o binário existe, fala os dois dialetos (texto/JSON), devolve
//! códigos de saída úteis e não depende de servidor para as rotinas de teste.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_aether-sgdb")
}

fn run(args: &[&str]) -> (i32, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("executa aether-sgdb");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

fn temp_path(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "aether_sgdb_f5_{tag}_{}_{unique}.jsonl",
        std::process::id()
    ))
}

#[test]
fn f5_cli_doctor_demo_e_put() {
    let (code, text) = run(&["--version"]);
    assert_eq!(code, 0);
    assert!(text.contains("aether-sgdb"));

    let (code, text) = run(&["doctor", "--mock", "--json"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"ok\":true"));
    assert!(text.contains("\"transport\":\"mock"));

    let (code, text) = run(&["demo", "--json"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"ok\":true"));
    assert!(text.contains("\"seed_error\":"));

    let (code, text) = run(&[
        "put",
        "fato de teste",
        "--entity",
        "aether/f5",
        "--mock",
        "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"key\":\"md/L3/aether/"));
}

#[test]
fn f5_cli_export_import_sync_e_falha_acionavel() {
    // Export de um escopo vazio: cria o arquivo, sem drama.
    let exported = temp_path("export");
    let (code, text) = run(&["export", exported.to_str().unwrap(), "--mock", "--json"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"documents\":0"));
    assert!(exported.exists());

    // Import de um JSONL vazio: erro claro, não panic.
    let (code, text) = run(&["import", exported.to_str().unwrap(), "--mock"]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("nenhum documento"));

    // Sync de um outbox inexistente: zero entregas, sem pânico.
    let outbox = temp_path("outbox");
    let (code, text) = run(&[
        "sync",
        "--mock",
        "--outbox",
        outbox.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"delivered\":0"));

    // Falha de conexão: código 1 e dica acionável.
    let (code, text) = run(&[
        "doctor",
        "--url",
        "http://127.0.0.1:1/mcp",
        "--retries",
        "0",
        "--timeout-ms",
        "100",
        "--json",
    ]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("\"ok\":false"));
    assert!(text.contains("dica") || text.contains("\"hint\""));

    let _ = std::fs::remove_file(&exported);
    let _ = std::fs::remove_file(&outbox);
}
