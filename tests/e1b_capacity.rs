//! E1b — gates de capacidade de memória.
//!
//! Mede o `FhrrStore` (fatos `e⊗r⊗v`) em D=160 e confirma a lei de escala.
//! Valores de referência do experimento completo em `target/aether_e1b.json`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

use aether_engine::core::fhrr::FhrrStore;

const SEED: u64 = 0xE1B_0001;

fn random_phases(dim: usize, rng: &mut ChaCha8Rng) -> Vec<f32> {
    (0..dim).map(|_| rng.gen::<f32>() * TAU).collect()
}

/// `recall@1` de um store com `m` fatos, consultando 200 amostras.
fn recall_at(dim: usize, traces: usize, m: usize, seed: u64) -> f64 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut store = FhrrStore::with_traces(dim, traces).expect("store");
    let relation = random_phases(dim, &mut rng);
    let mut entities = Vec::with_capacity(m);
    for _ in 0..m {
        let e = random_phases(dim, &mut rng);
        let v = random_phases(dim, &mut rng);
        store.add_fact(&e, &relation, &v).expect("add_fact");
        entities.push(e);
    }

    let mask = vec![true; dim];
    let n_q = m.min(200);
    let mut hits = 0usize;
    for k in 0..n_q {
        let idx = if m <= 200 { k } else { (k * m) / n_q };
        let hit = store
            .query_match(&entities[idx], &relation, &mask)
            .expect("query");
        if hit.value_index == idx {
            hits += 1;
        }
    }
    hits as f64 / n_q as f64
}

/// Maior potência de dois com `recall@1 ≥ 0.99`.
fn capacity(dim: usize, traces: usize, cap: usize) -> usize {
    let mut best = 0;
    let mut m = 8;
    while m <= cap {
        if recall_at(dim, traces, m, SEED) >= 0.99 {
            best = m;
        }
        m *= 2;
    }
    best
}

#[test]
fn d160_holds_64_facts_at_99_percent() {
    let r = recall_at(160, 6, 64, SEED);
    assert!(r >= 0.99, "D=160 K=6 M=64: recall {r} < 0.99");
}

#[test]
fn d160_degrades_by_256_facts() {
    let r = recall_at(160, 6, 256, SEED);
    assert!(r < 0.90, "D=160 K=6 M=256: recall {r} deveria degradar");
}

#[test]
fn capacity_is_linear_in_d_not_quadratic() {
    // Medido: ~0.4·D para K=6. D² daria 160²=25.600 em D=160 (absurdo).
    let c160 = capacity(160, 6, 512);
    let c320 = capacity(320, 6, 1024);
    assert_eq!(c160, 64, "capacidade D=160 mudou");
    assert_eq!(c320, 128, "capacidade D=320 mudou");
    assert!(
        c320 <= 4 * c160,
        "crescimento {c320}/{c160} sugere lei super-linear"
    );
}

#[test]
fn more_traces_help() {
    let one = capacity(160, 1, 256);
    let six = capacity(160, 6, 256);
    assert!(
        six > one,
        "K=6 ({six}) deveria superar K=1 ({one}) em D=160"
    );
}
