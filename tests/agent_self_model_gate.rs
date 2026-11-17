//! P4 — calibração do self-model (Brier, ECE, temperatura).
//!
//! G-P4a: dados sintéticos perfeitamente calibrados → ECE pequeno e Brier
//! baixo.
//! G-P4b: dados superconfiantes (prevê 0.9, observa 0.6) → ECE alto; a
//! recalibração por temperatura reduz o log loss nas mesmas amostras.
//! G-P4c: persistência roundtrip (counts + amostras + temperatura) e
//! compatibilidade com JSON legado sem os campos novos.
//! G-P4d: `calibrated_success` é monótono não-decrescente em `success` para
//! temperatura fixa `> 0`.

#![cfg(feature = "agent")]

use std::fs;
use std::path::PathBuf;

use aether_engine::agent::{Goal, SelfModel};

/// Log loss calculado de forma independente do crate (verificação do gate).
fn log_loss_local(predicted: &[f64], outcomes: &[bool], temperature: f64) -> f64 {
    let logit = |p: f64| (p / (1.0 - p)).ln();
    let sigmoid = |x: f64| {
        if x >= 0.0 {
            1.0 / (1.0 + (-x).exp())
        } else {
            let e = x.exp();
            e / (1.0 + e)
        }
    };
    let mut sum = 0.0;
    for (p, observed) in predicted.iter().zip(outcomes) {
        let p = sigmoid(logit(*p) / temperature).clamp(1e-6, 1.0 - 1e-6);
        sum += if *observed { -p.ln() } else { -(1.0 - p).ln() };
    }
    sum / predicted.len() as f64
}

/// Caminho de política isolado por teste/processo.
fn policy_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aether_self_model_p4_{}_{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("criar dir temporário");
    dir.join("policy.json")
}

#[test]
fn p4_calibrado_perfeito_ece_e_brier_baixos() {
    let mut model = SelfModel::new(None);
    let bins = 10usize;
    let per_bin = 40usize;
    for bin in 0..bins {
        let predicted = (bin as f64 + 0.5) / bins as f64;
        let successes = (predicted * per_bin as f64).round() as usize;
        for index in 0..per_bin {
            model.record_prediction(predicted, index < successes);
        }
    }
    assert_eq!(model.calibration_len(), bins * per_bin);

    let ece = model.expected_calibration_error(bins).expect("ECE");
    let brier = model.brier_score().expect("Brier");
    println!(
        "P4 calibrado: n={} ECE={ece:.6} Brier={brier:.6}",
        model.calibration_len()
    );

    let table = model.reliability();
    println!("P4 reliability: {table:?}");
    assert_eq!(table.len(), bins, "tabela deveria cobrir os 10 bins");
    for (_, mean_pred, empirical, count) in &table {
        assert_eq!(*count, per_bin);
        assert!((mean_pred - empirical).abs() <= 0.01, "bin descalibrado");
    }

    assert!(ece <= 0.05, "ECE {ece:.4} > 0.05");
    assert!(brier <= 0.25, "Brier {brier:.4} > 0.25");
}

#[test]
fn p4_superconfiante_ece_alto_e_temperatura_reduz_log_loss() {
    let mut model = SelfModel::new(None);
    let n = 100usize;
    let observed_rate = 0.6;
    let predicted_p = 0.9;
    let successes = (observed_rate * n as f64).round() as usize;
    let mut predicted = Vec::with_capacity(n);
    let mut outcomes = Vec::with_capacity(n);
    for index in 0..n {
        let success = index < successes;
        predicted.push(predicted_p);
        outcomes.push(success);
        model.record_prediction(predicted_p, success);
    }
    assert_eq!(model.calibration_len(), n);

    let ece_before = model.expected_calibration_error(10).expect("ECE");
    assert!(
        ece_before > 0.1,
        "ECE {ece_before:.4} deveria denunciar superconfiança (> 0.1)"
    );

    let loss_before = model.log_loss().expect("log loss antes");
    let independent_before = log_loss_local(&predicted, &outcomes, 1.0);
    let fitted = model.fit_temperature().expect("temperatura ajustada");
    assert!(
        (loss_before - independent_before).abs() < 1e-9,
        "log loss do modelo {loss_before:.6} != independente {independent_before:.6}"
    );

    let loss_after = model.log_loss().expect("log loss depois");
    let independent_after = log_loss_local(&predicted, &outcomes, fitted);
    println!(
        "P4 superconfiante: ECE={ece_before:.4} p=0.9 y={observed_rate} \
         T={fitted:.4} logloss {loss_before:.6} -> {loss_after:.6} \
         (independente {independent_before:.6} -> {independent_after:.6})"
    );

    assert_eq!(model.temperature(), fitted);
    assert!(
        fitted > 1.0,
        "temperatura {fitted:.4} deveria suavizar p=0.9"
    );
    assert!(
        loss_after < loss_before - 1e-6,
        "recalibração não reduziu log loss ({loss_before:.6} -> {loss_after:.6})"
    );
    assert!((loss_after - independent_after).abs() < 1e-9);
}

#[test]
fn p4_persistencia_roundtrip_e_json_legado() {
    let path = policy_path("roundtrip");
    let goal = Goal::fact("Brasil", "capital");

    let mut model = SelfModel::new(Some(path.clone()));
    model.update("knowledge", &goal, true);
    model.update("knowledge", &goal, false);
    model.record_prediction(0.25, false);
    model.record_prediction(0.75, true);
    model.set_temperature(2.5);

    let expected_success = model.success("knowledge", &goal);
    let expected_brier = model.brier_score().expect("Brier");
    let expected_ece = model.expected_calibration_error(5).expect("ECE");
    let expected_samples = model.calibration_len();
    model.persist();
    assert!(path.exists(), "política não foi escrita em {path:?}");

    let mut loaded = SelfModel::new(Some(path.clone()));
    loaded.load();
    assert_eq!(loaded.episodes(), model.episodes());
    assert_eq!(loaded.calibration_len(), expected_samples);
    assert!((loaded.temperature() - 2.5).abs() < 1e-12);
    assert!((loaded.success("knowledge", &goal) - expected_success).abs() < 1e-12);
    assert!((loaded.brier_score().expect("Brier") - expected_brier).abs() < 1e-12);
    assert!((loaded.expected_calibration_error(5).expect("ECE") - expected_ece).abs() < 1e-12);
    println!(
        "P4 roundtrip: episodes={} samples={} T={:.3} Brier={:.6} ECE={:.6}",
        loaded.episodes(),
        loaded.calibration_len(),
        loaded.temperature(),
        expected_brier,
        expected_ece
    );

    // Legado: JSON versão 1, sem `temperature` nem `calibration`.
    let legacy = path.with_file_name("legacy.json");
    fs::write(
        &legacy,
        r#"{"version":1,"episodes":3,"stats":[{"tool":"legacy","cluster":"0000000000000009","alpha":4,"beta":2}]}"#,
    )
    .expect("escrever legado");
    let mut old = SelfModel::new(Some(legacy));
    old.load();
    assert_eq!(old.episodes(), 3);
    assert_eq!(old.temperature(), 1.0, "default de temperatura");
    assert_eq!(old.calibration_len(), 0, "default do ring");
    assert!(old.brier_score().is_none());
    let snapshot = old.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].0, "legacy");
    assert!((snapshot[0].2 - 4.0 / 6.0).abs() < 1e-12);

    if let Some(dir) = path.parent() {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn p4_calibrated_success_monotono_em_success() {
    let goal = Goal::fact("AETHER", "gate");
    let temperature = 2.0;
    let mut previous = f64::NEG_INFINITY;
    for successes in 0..=6 {
        let mut model = SelfModel::new(None);
        model.set_temperature(temperature);
        for _ in 0..successes {
            model.update("tool", &goal, true);
        }
        let raw = model.success("tool", &goal);
        let calibrated = model.calibrated_success("tool", &goal);
        println!(
            "P4 monotônico: sucessos={successes} raw={raw:.6} calibrado={calibrated:.6} T={temperature}"
        );
        assert!(
            (0.0..=1.0).contains(&calibrated),
            "calibrado {calibrated:.6} fora de [0,1]"
        );
        assert!(
            calibrated >= previous - 1e-12,
            "não monótono: {calibrated:.6} < {previous:.6}"
        );
        previous = calibrated;
    }

    // T = 1 é identidade: `calibrated_success` preserva a semântica de `success`.
    let identity = SelfModel::new(None);
    assert_eq!(identity.temperature(), 1.0);
    assert!(
        (identity.calibrated_success("tool", &goal) - identity.success("tool", &goal)).abs()
            < 1e-12
    );
}
