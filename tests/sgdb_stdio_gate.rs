//! Onda M — remember, recall, audit_checkpoint e contradicts no neural-sgdb real.
//!
//! O transporte é stdio para o `mcp_server` já compilado. O banco fica em
//! `target/sgdb-stdio-probe.db` (não mexe em `.env`). Sem o binário, o teste
//! retorna: não há mock no caminho.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_engine::core::knowledge_bridge_f1::ScopeId;
use aether_engine::knowledge::sgdb::{open, SgdbConfig};

const SERVER: &str =
    r"C:\Users\msrov\Documents\neural-sgdb\target\mcp-release\release\examples\mcp_server.exe";

#[test]
fn stdio_remember_recall_audit_contradicts() {
    if !Path::new(SERVER).exists() {
        eprintln!("stdio gate: mcp_server ausente, sem mock");
        return;
    }
    let db = Path::new("target").join("sgdb-stdio-probe.db");
    if let Some(parent) = db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::env::set_var("NEURAL_SGDB_DB", &db);

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(1);
    let text_a = format!("aether fato probe {stamp} gato");
    let text_b = format!("aether fato probe {stamp} gato ausente");

    let mut cfg = SgdbConfig::default();
    cfg.if_exists = "add".into();
    cfg.retries = 0;
    cfg.retry_backoff_ms = 0;
    let mut client = open(&format!("stdio:{SERVER}"), cfg).expect("stdio");
    let scope = ScopeId::new(1, 0, 1);

    let key_a = client
        .remember_text(scope, &text_a, &[], "L3", 80, None)
        .expect("remember a")
        .expect("chave a");
    let key_b = client
        .remember_text(scope, &text_b, &[], "L3", 80, None)
        .expect("remember b")
        .expect("chave b");

    let hits = client
        .recall_text(scope, &format!("probe {stamp} gato"), 8)
        .expect("recall");
    assert!(
        hits.iter().any(|hit| hit.text.contains(&text_a)),
        "recall não devolveu o fato: {hits:?}"
    );

    client
        .associate_contradicts(&key_a, &key_b)
        .expect("contradicts");
    let listed = client.contradicts_of(&key_a).expect("lista");
    let listed_text = listed.to_text();
    assert!(
        listed_text.contains(&key_b) || listed_text.contains("contrad"),
        "contradicts não registrou o par: {listed_text}"
    );

    let again = client
        .recall_text(scope, &format!("probe {stamp} gato"), 8)
        .expect("recall 2");
    assert!(
        again.iter().any(|hit| hit.text.contains(&text_a)),
        "o fato anterior sumiu depois do contradicts"
    );

    client.audit_checkpoint().expect("audit_checkpoint");
    assert!(!client.describe().contains("mock"));
}
