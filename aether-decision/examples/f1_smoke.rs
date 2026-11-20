//! F1 smoke — o porto real, contra o interpretador real, com os checkpoints locais.
//!
//! Prova o critério de aceite da F1 ("primeira decisão via DecisionPort") sem
//! tocar no chat: `VegaPort::new` (load + fit da calibração) e depois `decide`
//! numa pergunta booleana e numa de escolha. Grava `target/f1_decision_smoke.json`.
//!
//! Rodar (o interpretador precisa do `vegaml`; ver `docs/F1_CONTRATO_MEDIDO.md`):
//!
//! ```text
//! PYO3_PYTHON=target/vega/Scripts/python.exe \
//!   cargo run -p aether_decision --features pyo3 --example f1_smoke
//! ```
//!
//! Exit 0 = decidiu; exit 3 = ABSTAIN honesto (artefato com o motivo).

use std::io::Write as _;
use std::time::Instant;

use aether_decision::config::EngineConfig;
use aether_decision::port::DecisionPort;
use aether_decision::pyo3_impl::VegaPort;
use aether_decision::types::DecisionRequest;

const STATE: &str =
    "Gate cargo_test_release: 176 passed, 0 failed, clippy -D warnings limpo, fmt OK";

fn write_artifact(rec: &serde_json::Value) {
    if let Ok(s) = serde_json::to_string_pretty(rec) {
        let _ = std::fs::create_dir_all("target");
        if let Ok(mut fh) = std::fs::File::create("target/f1_decision_smoke.json") {
            let _ = fh.write_all(s.as_bytes());
        }
        println!("{s}");
    }
}

fn main() {
    let corpus_path = "data/vega_calibration_starter.json";
    let mut cfg = EngineConfig::gate_f1(Some(corpus_path.to_string()));
    // `gate_f1` deixa o primário no hub (download). O smoke é offline por
    // contrato: usa o checkpoint local que já está no disco.
    cfg.model_primary = Some(
        std::env::var("VEGA_PRIMARY").unwrap_or_else(|_| "target/vega/vega-u-gemma-1b".into()),
    );

    let corpus: serde_json::Value = match std::fs::read_to_string(corpus_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
    {
        Some(v) => v,
        None => {
            write_artifact(&serde_json::json!({
                "schema": "aether.f1_smoke/v1",
                "abstain": true,
                "error": format!("corpus ilegivel: {corpus_path}"),
            }));
            std::process::exit(3);
        }
    };
    let questions = serde_json::json!({
        "passou": corpus["questions"]["passou"],
        "route": corpus["questions"]["route"],
    });

    let t0 = Instant::now();
    let port = match VegaPort::new(&cfg) {
        Ok(p) => p,
        Err(e) => {
            write_artifact(&serde_json::json!({
                "schema": "aether.f1_smoke/v1",
                "abstain": true,
                "reason": "engine_unavailable",
                "error": format!("{e}"),
            }));
            std::process::exit(3);
        }
    };
    let boot_s = t0.elapsed().as_secs_f64();

    let req = DecisionRequest {
        state: STATE.to_string(),
        questions,
    };
    match port.decide(&req) {
        Ok(resp) => {
            let abstains = resp.answers.iter().filter(|a| a.abstain).count();
            write_artifact(&serde_json::json!({
                "schema": "aether.f1_smoke/v1",
                "abstain": false,
                "boot_s": (boot_s * 10.0).round() / 10.0,
                "latency_ms": resp.meta.latency_ms,
                "engine": resp.meta.engine,
                "degraded": resp.meta.degraded,
                "conformal_set_present": resp.conformal_set.is_some(),
                "conformal_set": resp.conformal_set,
                "n_answers": resp.answers.len(),
                "n_abstain": abstains,
                "answers": resp.answers,
            }));
        }
        Err(e) => {
            write_artifact(&serde_json::json!({
                "schema": "aether.f1_smoke/v1",
                "abstain": true,
                "reason": "engine_unavailable",
                "boot_s": (boot_s * 10.0).round() / 10.0,
                "error": format!("{e}"),
            }));
            std::process::exit(3);
        }
    }
}
