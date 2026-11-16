//! E1b / E1b-2 — gates de capacidade de memória.
//!
//! Mede o `FhrrStore` (fatos `e⊗r⊗v`) em D=160 e confirma a lei de escala.
//! Valores de referência do experimento completo em `target/aether_e1b.json`.
//!
//! E1b-2 acrescenta o **braço harmônico** da memória de fase (M5): a fronteira
//! operacional medida é `M_harm = 8` (erro < 0,15 rad) contra `M_rand = 4` do
//! caso adversarial aleatório — e não os 40 declarados por `pattern_capacity()`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

use aether_engine::core::fhrr::FhrrStore;
use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};

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

// ---------------------------------------------------------------------------
// E1b-2 — memória de fase: aleatório (adversarial) vs harmônico (estruturado)
// ---------------------------------------------------------------------------

/// Padrão harmônico contínuo — **o mesmo** usado nos testes do M5.
fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(TAU)
        })
        .collect()
}

/// Máscara com exatamente `hidden` posições ocultas (sem reposição).
fn mask_exact(seed: u64, n: usize, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = rng.gen_range(0..=i);
        idx.swap(i, j);
    }
    let mut mask = vec![true; n];
    for &h in idx.iter().take(hidden.min(n)) {
        mask[h] = false;
    }
    mask
}

fn audit_config() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

fn wrapped_abs(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % TAU;
    if d > std::f32::consts::PI {
        d -= TAU;
    }
    if d < -std::f32::consts::PI {
        d += TAU;
    }
    d.abs()
}

/// Erro angular médio nas posições ocultas, com `M` padrões de uma classe.
fn phase_error(n: usize, patterns: usize, harmonic: bool, hidden: usize) -> f32 {
    let mut mem = ComplexPhaseMemory::with_config(n, audit_config());
    let mask = mask_exact(SEED ^ 0x9E37, n, hidden);
    let stored: Vec<Vec<f32>> = if harmonic {
        (0..patterns).map(|o| harmonic_pattern(n, o)).collect()
    } else {
        let mut rng = ChaCha8Rng::seed_from_u64(SEED);
        (0..patterns)
            .map(|_| (0..n).map(|_| rng.gen::<f32>() * TAU).collect())
            .collect()
    };
    for p in &stored {
        mem.imprint(p, 40);
    }
    let mut err = 0.0f32;
    let mut count = 0usize;
    for p in &stored {
        mem.reset_phases(0.0);
        let (out, _) = mem.recall(p, &mask, 200, 0.05);
        for i in 0..n {
            if !mask[i] {
                err += wrapped_abs(out[i], p[i]);
                count += 1;
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        err / count as f32
    }
}

const GATE_RAD: f32 = 0.15;

#[test]
fn e1b2_m4_ambos_passam_do_gate() {
    let (n, hidden) = (160usize, 32usize);
    let rand_err = phase_error(n, 4, false, hidden);
    let harm_err = phase_error(n, 4, true, hidden);
    assert!(
        rand_err < GATE_RAD,
        "aleatório M=4: {rand_err} ≥ {GATE_RAD} rad"
    );
    assert!(
        harm_err < GATE_RAD,
        "harmônico M=4: {harm_err} ≥ {GATE_RAD} rad"
    );
}

#[test]
fn e1b2_m8_separa_harmonico_de_aleatorio() {
    let (n, hidden) = (160usize, 32usize);
    let harm_err = phase_error(n, 8, true, hidden);
    let rand_err = phase_error(n, 8, false, hidden);
    assert!(
        harm_err < GATE_RAD,
        "harmônico M=8: {harm_err} deveria ficar < {GATE_RAD} rad (M_harm = 8)"
    );
    assert!(
        rand_err >= GATE_RAD,
        "aleatório M=8: {rand_err} deveria estourar o gate (M_rand = 4)"
    );
}

#[test]
fn e1b2_fronteira_e_8_nao_40() {
    // Em M = 16 os dois braços já estouraram: a fronteira do M5 é 8.
    let (n, hidden) = (160usize, 32usize);
    let harm16 = phase_error(n, 16, true, hidden);
    assert!(
        harm16 >= GATE_RAD,
        "harmônico M=16: {harm16} deveria estourar o gate (fronteira = 8, não 40)"
    );
    let declared = ComplexPhaseMemory::new(n).pattern_capacity();
    assert_eq!(declared, 40, "pattern_capacity() declarado mudou");
    assert!(
        declared >= 4 * 8,
        "o declarado ({declared}) é otimista por ≥ 5x frente a M_harm = 8"
    );
}
