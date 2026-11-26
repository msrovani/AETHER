//! A5 / T-HIER — gate: LSH + re-rank f32 contra o MoE plano, nas duas direções.
//!
//! fwd = `s1|rel|s2` (o bank do round); rev = `s2|rel|s1` (o lado que fecha o
//! B2), gerado por `tools/a5_rev_round.py` em `target/a5_rev_round`.
//! GO: `recall@1` do caminho LSH **não menor** que o plano, com p99 menor, em
//! cada direção medida. Sem os artefatos de uma direção, o gate não inventa
//! número: sai com skip explícito e `both_directions_ok` fica falso.

#![cfg(feature = "agent")]

use std::path::{Path, PathBuf};

use aether_engine::agent::{
    best_passing, sweep, DirectionMeasure, HierMoeStatus, LshConfig, Round, DEFAULT_ROUND,
    HIER_MOE_STATUS_PATH, RERANK_TOP, REV_ROUND,
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

/// Varredura vencedora de uma direção.
struct Sweep {
    dir: DirectionMeasure,
    over_capacity: bool,
}

fn dir_from_env(var: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_string()))
}

/// Mede uma direção; `None` (com skip explícito) se faltar artefato do round.
fn sweep_round(dir: &Path, n: usize, tag: &str) -> Option<Sweep> {
    for rel in [
        "hv/bank.bin",
        "hv/bank.idx",
        "emb/bank.f32",
        "s2_query_hv.bin",
        "s2_query_f32.bin",
    ] {
        if !dir.join(rel).exists() {
            eprintln!(
                "A5 {tag}: {} ausente — skip (sem bank/query não há medida)",
                dir.join(rel).display()
            );
            return None;
        }
    }

    let mut round =
        Round::load(dir).unwrap_or_else(|e| panic!("A5 {tag}: round {}: {e}", dir.display()));
    let n = n.min(round.len());
    let (plain, rows) = sweep(&mut round, n, RERANK_TOP, CONFIGS);
    eprintln!(
        "A5 {tag} plano: n={} recall@1={:.4} p99={}us cand={:.1}",
        plain.n, plain.recall1, plain.p99_us, plain.candidates_avg
    );
    for (cfg, m) in &rows {
        eprintln!(
            "A5 {tag} lsh {}: recall@1={:.4} p99={}us cand={:.1}",
            cfg.label(),
            m.recall1,
            m.p99_us,
            m.candidates_avg
        );
    }

    // Negativo honesto: sem config aprovada, registra a de maior recall.
    let (config, lsh) = best_passing(plain, &rows)
        .or_else(|| {
            rows.iter()
                .max_by(|a, b| {
                    a.1.recall1
                        .total_cmp(&b.1.recall1)
                        .then(b.1.p99_us.cmp(&a.1.p99_us))
                })
                .copied()
        })
        .expect("sweep vazio");
    Some(Sweep {
        dir: DirectionMeasure { config, plain, lsh },
        over_capacity: round.bank.over_capacity(),
    })
}

#[test]
fn hier_moe_gate_lsh_vs_plano() {
    let n = std::env::var("AETHER_HIER_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200usize);

    let base = dir_from_env("AETHER_ROUND", DEFAULT_ROUND);
    let fwd = sweep_round(&base, n, "fwd").expect("A5: round base obrigatório (ver DEFAULT_ROUND)");

    let mut status = HierMoeStatus::from_env();
    status.measured(fwd.over_capacity, RERANK_TOP, fwd.dir);

    let rev_dir = dir_from_env("AETHER_ROUND_REV", REV_ROUND);
    status.rev = sweep_round(&rev_dir, n, "rev").map(|r| r.dir);
    match &status.rev {
        Some(r) => eprintln!(
            "A5 rev: plano recall@1={:.4} p99={}us | lsh {} recall@1={:.4} p99={}us",
            r.plain.recall1,
            r.plain.p99_us,
            r.config.label(),
            r.lsh.recall1,
            r.lsh.p99_us
        ),
        None => eprintln!(
            "A5 rev: {} ausente — rode `python tools/a5_rev_round.py`; `both_directions_ok` fica falso",
            rev_dir.display()
        ),
    }

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
        "GO fwd não medido: nenhuma config LSH preserva recall {:.4} com p99 menor que {}us",
        fwd.dir.plain.recall1,
        fwd.dir.plain.p99_us
    );
    match &status.rev {
        Some(r) => assert!(
            status.both_directions_ok(),
            "GO rev não medido: lsh recall {:.4} p99 {}us vs plano recall {:.4} p99 {}us",
            r.lsh.recall1,
            r.lsh.p99_us,
            r.plain.recall1,
            r.plain.p99_us
        ),
        None => assert!(
            !status.both_directions_ok(),
            "sem medida rev o A5 não pode autorizar o default"
        ),
    }
    // Gate medido: sem feature+env, mesmo com números verdes, não liga.
    assert_eq!(status.may_enable(), status.feature_on && status.env_on);
}
