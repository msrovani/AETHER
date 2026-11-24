//! A5 / T-HIER — gate: LSH + re-rank f32 contra o MoE plano.
//!
//! GO: `recall@1` do caminho LSH **não menor** que o plano, com p99 menor.
//! Medido no mesmo bank, mesma verdade e mesmo `k` (top-32). Sem os artefatos
//! do round, o gate não inventa número: sai com skip explícito.

#![cfg(feature = "agent")]

use std::path::Path;

use aether_engine::agent::{
    best_passing, sweep, HierMoeStatus, LshConfig, Round, DEFAULT_ROUND, HIER_MOE_STATUS_PATH,
    RERANK_TOP,
};
use aether_engine::json::Json;

/// Configurações medidas. A default do bank (4×16) é a mais esparsa; as de 8
/// bits trocam candidatos por cobertura.
const CONFIGS: &[LshConfig] = &[
    LshConfig::new(4, 16, 8),
    LshConfig::new(8, 8, 8),
    LshConfig::new(12, 8, 8),
    LshConfig::new(16, 8, 8),
    LshConfig::new(8, 10, 16),
    LshConfig::new(4, 12, 8),
];

#[test]
fn hier_moe_gate_lsh_vs_plano() {
    let dir = std::env::var("AETHER_ROUND").unwrap_or_else(|_| DEFAULT_ROUND.to_string());
    let dir = Path::new(&dir);
    for rel in [
        "hv/bank.bin",
        "hv/bank.idx",
        "emb/bank.f32",
        "s2_query_hv.bin",
        "s2_query_f32.bin",
    ] {
        if !dir.join(rel).exists() {
            eprintln!(
                "A5: {} ausente — skip (sem bank/query não há medida)",
                dir.join(rel).display()
            );
            return;
        }
    }

    let mut round = Round::load(dir).expect("round do A5");
    let n = std::env::var("AETHER_HIER_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200usize)
        .min(round.len());
    let (plain, rows) = sweep(&mut round, n, RERANK_TOP, CONFIGS);
    eprintln!(
        "A5 plano: n={} recall@1={:.4} p99={}us cand={:.1}",
        plain.n, plain.recall1, plain.p99_us, plain.candidates_avg
    );
    for (cfg, m) in &rows {
        eprintln!(
            "A5 lsh {}: recall@1={:.4} p99={}us cand={:.1}",
            cfg.label(),
            m.recall1,
            m.p99_us,
            m.candidates_avg
        );
    }

    let mut status = HierMoeStatus::from_env();
    let chosen = best_passing(plain, &rows).or_else(|| {
        // Negativo honesto: registra a config de maior recall para o artefato.
        rows.iter()
            .max_by(|a, b| {
                a.1.recall1
                    .total_cmp(&b.1.recall1)
                    .then(b.1.p99_us.cmp(&a.1.p99_us))
            })
            .copied()
    });
    let (cfg, lsh) = chosen.expect("sweep vazio");
    let over_capacity = round.bank.over_capacity();
    status.measured(over_capacity, RERANK_TOP, cfg, plain, lsh);

    std::fs::create_dir_all("target").ok();
    std::fs::write(HIER_MOE_STATUS_PATH, status.to_json().to_text()).expect("artefato A5");
    eprintln!("A5 artefato: {HIER_MOE_STATUS_PATH}");

    let j: Json = status.to_json();
    assert_eq!(
        j.get("default_path").and_then(|v| v.as_bool()),
        Some(false),
        "A5 nunca entra no caminho default por gate"
    );
    assert!(
        status.lsh_ok(),
        "GO do A5 não medido: nenhuma config LSH preserva recall {:.4} com p99 menor que {}us (melhor: {} recall {:.4} p99 {}us, cand {:.1})",
        plain.recall1,
        plain.p99_us,
        cfg.label(),
        lsh.recall1,
        lsh.p99_us,
        lsh.candidates_avg
    );
    // Gate medido: sem feature+env, mesmo com números verdes, não liga.
    assert_eq!(status.may_enable(), status.feature_on && status.env_on);
}
