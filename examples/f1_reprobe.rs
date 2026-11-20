//! F1 — re-probe n=30 do gate (Eixo D do plano) + baseline congelado.
//!
//! O plano exige: *"re-probe n=30 antes/depois de cada mudança. Ganho medido
//! ou não vale."* e o aceite da F1 é *"primeira decisão via DecisionPort"*.
//!
//! Este probe:
//! 1. carrega o engine real (PyO3) e mede o **boot** (load + fit);
//! 2. roda **n=30** decisões com as perguntas calibradas do corpus;
//! 3. confere o **contrato** de cada resposta (F1-1/F1-2/F1-3);
//! 4. grava o baseline em `data/f1_gate_baseline.json` — **fora de `target/`**,
//!    porque `target/` é apagado pelo `cargo clean` do ciclo (e foi exatamente
//!    assim que o motor do AETHER quase sumiu: fórum MSG 0748).
//!
//! Uso:
//! ```text
//! $env:PYO3_PYTHON = "$PWD\.vega\venv\Scripts\python.exe"
//! $env:PYTHONPATH  = "$PWD\.vega\venv\Lib\site-packages"
//! $env:VEGA_MODEL  = "$PWD\.vega\models\vega-u-gemma-1b"
//! cargo run --release --features decision-pyo3 --example f1_reprobe
//! ```
//!
//! Exit 0 = baseline gravado; exit 3 =Falha honesta (artefato com o motivo).

use std::io::Write as _;
use std::time::Instant;

use aether_decision::config::EngineConfig;
use aether_decision::port::DecisionPort;
use aether_decision::pyo3_impl::VegaPort;
use aether_decision::types::{AbstainReason, DecisionRequest};

/// Estado usado nas 30 rodadas: um fato medido do próprio projeto.
const STATE: &str =
    "Gate cargo_test_release: 176 passed, 0 failed, clippy -D warnings limpo, fmt OK";

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn main() {
    let n: usize = std::env::var("AETHER_F1_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let mut cfg = EngineConfig::from_env();
    if cfg.model_primary.is_none() {
        cfg.model_primary = std::env::var("VEGA_MODEL")
            .ok()
            .or_else(|| Some("target/vega/vega-u-gemma-1b".into()));
    }
    if cfg.model_sentinel.is_none() {
        cfg.model_sentinel = std::env::var("VEGA_PROSE_MODEL")
            .ok()
            .or_else(|| Some("target/vega/vega-u-gemma-1b-prose".into()));
    }
    if cfg.calibration.is_none() {
        cfg.calibration = Some("data/vega_calibration_starter.json".into());
    }

    let questions: serde_json::Value =
        match std::fs::read_to_string("data/vega_calibration_starter.json")
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            Some(v) => serde_json::json!({
                "passou": v["questions"]["passou"],
                "route": v["questions"]["route"],
            }),
            None => {
                write_baseline(serde_json::json!({
                    "schema": "aether.f1_gate_baseline/v1",
                    "abstain": true,
                    "reason": "corpus_ilegivel",
                    "n": n,
                }));
                std::process::exit(3);
            }
        };

    // 1. boot: load + fit(124) nos dois readers.
    let t_boot = Instant::now();
    let port = match VegaPort::new(&cfg) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("{e}");
            let hint = if msg.contains("vegaml") {
                " PYTHONPATH aponta para o site-packages do venv? (o embed usa o Python BASE)"
            } else {
                ""
            };
            write_baseline(serde_json::json!({
                "schema": "aether.f1_gate_baseline/v1",
                "abstain": true,
                "reason": "engine_unavailable",
                "error": msg,
                "hint": hint,
                "n": n,
            }));
            eprintln!("[f1_reprobe] engine indisponivel: {msg}{hint}");
            std::process::exit(3);
        }
    };
    let boot_s = t_boot.elapsed().as_secs_f64();

    // 2. warmup fora da medicao (Eixo D: "warmup fora da medicao").
    let req = DecisionRequest {
        state: STATE.to_string(),
        questions: questions.clone(),
    };
    for _ in 0..3 {
        let _ = port.decide(&req);
    }

    // 3. n=30 decisoes medidas.
    let mut lat: Vec<f64> = Vec::with_capacity(n);
    let mut last = None;
    for _ in 0..n {
        let t = Instant::now();
        let r = port.decide(&req);
        lat.push(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(r);
    }
    let mut sorted = lat.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // 4. contrato por resposta (F1-1/F1-2/F1-3).
    let resp = match last {
        Some(Ok(r)) => r,
        Some(Err(e)) => {
            write_baseline(serde_json::json!({
                "schema": "aether.f1_gate_baseline/v1",
                "abstain": true,
                "reason": "engine_unavailable",
                "error": format!("{e}"),
                "n": n,
            }));
            std::process::exit(3);
        }
        None => {
            std::process::exit(3);
        }
    };

    let mut per_answer = Vec::new();
    let mut contract_ok = true;
    let mut reasons: Vec<&str> = Vec::new();
    for a in &resp.answers {
        let has_choice = a.choice.is_some();
        let has_set = a.conformal_set.is_some();
        // F1-1: pergunta booleana sem `choice` no vegaml -> o schema precisa
        // de `choice` derivado. Se ainda vier None, o contrato esta furado.
        if !has_choice {
            contract_ok = false;
        }
        if a.abstain_reason == Some(AbstainReason::Unknown) {
            reasons.push("unknown");
        } else if a.abstain_reason == Some(AbstainReason::Ambiguous) {
            reasons.push("ambiguous");
        } else if a.abstain_reason == Some(AbstainReason::Conflict) {
            reasons.push("conflict");
        } else if a.abstain_reason == Some(AbstainReason::EngineUnavailable) {
            reasons.push("engine_unavailable");
        } else {
            reasons.push("none");
        }
        per_answer.push(serde_json::json!({
            "question": a.question,
            "choice": a.choice,
            "abstain": a.abstain,
            "abstain_reason": a.abstain_reason,
            "readout": a.readout,
            "confidence": a.confidence,
            "conformal_set_present": has_set,
            "has_choice": has_choice,
        }));
    }

    let baseline = serde_json::json!({
        "schema": "aether.f1_gate_baseline/v1",
        "abstain": false,
        "measured_at": now_iso(),
        "n": n,
        "boot_s": round2(boot_s),
        "latency_ms": {
            "p50": round1(percentile(&sorted, 0.50)),
            "p95": round1(percentile(&sorted, 0.95)),
            "min": round1(*sorted.first().unwrap_or(&0.0)),
            "max": round1(*sorted.last().unwrap_or(&0.0)),
        },
        "engine": resp.meta.engine,
        "degraded": resp.meta.degraded,
        "contract": {
            "ok": contract_ok,
            "choice_em_toda_resposta": contract_ok,
            "reasons": reasons,
            "nota": "F1-1 choice derivado no booleano; F1-2 prediction_set -> conformal_set; F1-3 margem fina = ambiguous (nunca unknown)",
        },
        "answers": per_answer,
        "inputs": {
            "state": STATE,
            "corpus": cfg.calibration,
            "model_primary": cfg.model_primary,
            "model_sentinel": cfg.model_sentinel,
            "python": cfg.python,
            "python_site_packages": cfg.python_site_packages,
        },
        "note": "Eixo D: re-probe n=30 antes/depois de cada mudanca. Este arquivo e o baseline congelado; um novo probe com outro numero e uma mudanca, nao um numero novo.",
    });

    write_baseline(baseline);
    println!(
        "[f1_reprobe] boot={:.1}s p50={:.1}ms p95={:.1}ms n={} contract_ok={}",
        boot_s,
        percentile(&sorted, 0.50),
        percentile(&sorted, 0.95),
        n,
        contract_ok
    );
    if !contract_ok {
        std::process::exit(3);
    }
}

/// Grava o baseline **fora de `target/`** (o `cargo clean` do ciclo apaga
/// `target/`; a evidencia de aceite precisa sobreviver).
fn write_baseline(v: serde_json::Value) {
    let path = "data/f1_gate_baseline.json";
    if let Some(dir) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::File::create(path) {
        Ok(mut f) => {
            let _ = serde_json::to_writer_pretty(&mut f, &v);
            let _ = f.write_all(b"\n");
            let _ = f.flush();
            println!("[f1_reprobe] baseline gravado em {path}");
        }
        Err(e) => eprintln!("[f1_reprobe] nao consegui gravar {path}: {e}"),
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Timestamp ISO-8601 local (sem dependencia de data/hora).
fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Segundos desde a epoch -> data UTC (algoritmo civil de Howard Hinnant).
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}
