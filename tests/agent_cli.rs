//! Processo real do `aether-agent` (binário), não só a biblioteca.
//!
//! Prova que a face executável responde, abstém, ensina e roda a demo com os
//! gates de inferência/MoE/coerência/aprendizado.

#![cfg(feature = "agent")]

use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_aether-agent")
}

fn run(args: &[&str]) -> (i32, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("executa aether-agent");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

#[test]
fn cli_agent_ask_abstem_e_demo() {
    let (code, text) = run(&[
        "ask",
        "--subject",
        "Brasil",
        "--relation",
        "capital",
        "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"action\":\"act\""));
    assert!(text.contains("Brasília"));

    let (code, text) = run(&[
        "ask",
        "--subject",
        "Atlântida",
        "--relation",
        "capital",
        "--json",
    ]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("\"action\":\"abstain\""));

    let (code, text) = run(&["demo", "--json"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"known_ok\":5"));
    assert!(text.contains("\"abstained_unknown\":true"));
    assert!(text.contains("\"learned_ok\":5"));
}

#[test]
fn cli_agent_teach_e_sat() {
    let (code, text) = run(&[
        "teach",
        "--subject",
        "Itália",
        "--relation",
        "capital",
        "--value",
        "Roma",
        "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"after\":\"act\""));
    assert!(text.contains("\"answer\":\"Roma\""));

    let path = std::env::temp_dir().join(format!(
        "aether_agent_sat_{}_{}.dimacs",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&path, "p cnf 3 2\n1 2 0\n-1 3 0\n").unwrap();
    let (code, text) = run(&["sat", path.to_str().unwrap(), "--json"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("\"action\":\"act\""));
    assert!(text.contains("\"certificate\""));
    let _ = std::fs::remove_file(&path);
}
