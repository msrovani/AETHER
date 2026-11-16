//! E2c-3 — gates da busca exata guiada por persistência.
//!
//! * soundness contra força bruta (`n=14`, três regimes);
//! * o root de um vidro puro fixa 0 (teorema Z2) mas a busca continua exata;
//! * o custo (nós) fica dentro do orçamento medido no probe.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::persistency_search::ExactSearch;

const N: usize = 14;
const DELTA: f64 = 0.6;
const SEED0: u64 = 0xE2C_2026;

fn make_instance(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for m in (i + 1)..n {
            let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            j[i * n + m] = w;
            j[m * n + i] = w;
        }
    }
    let mut h = vec![0.0f64; n];
    let mut order: Vec<usize> = (0..n).collect();
    for idx in (1..n).rev() {
        let sw = rng.gen_range(0..=idx);
        order.swap(idx, sw);
    }
    for &v in order.iter().take(k) {
        let row_abs: f64 = j[v * n..(v + 1) * n]
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != v)
            .map(|(_, x)| x.abs())
            .sum();
        let sign = if rng.gen::<bool>() { 1.0 } else { -1.0 };
        h[v] = sign * (row_abs + delta);
    }
    (j, h)
}

fn brute_force(n: usize, j: &[f64], h: &[f64]) -> f64 {
    let mut best = f64::INFINITY;
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        best = best.min(energy(j, h, n, &s));
    }
    best
}

#[test]
fn exato_igual_a_forca_bruta_nos_tres_regimes() {
    let mut search = ExactSearch::new(u32::MAX);
    for (plant, expect_min_root) in [(0.0f64, 0usize), (0.25, 3), (0.45, 5)] {
        let k = (plant * N as f64).round() as usize;
        for inst in 0..50u64 {
            let (j, h) = make_instance(N, k, DELTA, SEED0.wrapping_add(inst));
            let r = search.solve(N, &j, &h);
            let bf = brute_force(N, &j, &h);
            assert!(
                !r.exhausted,
                "orçamento estourou (plant={plant} inst={inst})"
            );
            assert!(
                (r.energy - bf).abs() < 1e-9,
                "plant={plant} inst={inst}: exato={} bruto={}",
                r.energy,
                bf
            );
            assert!(
                r.root_fixed as usize >= expect_min_root,
                "plant={plant} inst={inst}: root_fixed={} < {}",
                r.root_fixed,
                expect_min_root
            );
        }
    }
}

#[test]
fn root_de_vidro_puro_e_zero_mas_busca_fecha() {
    // 10 instâncias plant=0: root fixa 0; nós << 2^14.
    let mut search = ExactSearch::new(u32::MAX);
    let mut total_nodes = 0u64;
    for inst in 0..10u64 {
        let (j, h) = make_instance(N, 0, DELTA, SEED0.wrapping_add(inst));
        let r = search.solve(N, &j, &h);
        assert!(!r.exhausted);
        assert_eq!(r.root_fixed, 0, "h=0 deveria fixar 0 no root");
        assert!(r.energy.is_finite());
        total_nodes += u64::from(r.nodes);
    }
    let mean = total_nodes / 10;
    assert!(
        mean < 3_000,
        "média de nós no vidro puro alta demais: {mean} (2^14 = 16384)"
    );
}

#[test]
fn vies_forte_reduz_a_arvore() {
    // plant=45%: poucos nós (árvore pequena), ótimo exato.
    let mut search = ExactSearch::new(u32::MAX);
    let mut total_nodes = 0u64;
    for inst in 0..50u64 {
        let (j, h) = make_instance(N, 6, DELTA, SEED0.wrapping_add(inst));
        let r = search.solve(N, &j, &h);
        assert!(!r.exhausted);
        total_nodes += u64::from(r.nodes);
    }
    let mean = total_nodes / 50;
    assert!(mean < 200, "média de nós com viés alto demais: {mean}");
}

/// Escala: n=20 com viés forte, contra força bruta. A árvore tem de ficar
/// muito abaixo de `2^20` — é o que separa "propagador" de "força bruta".
#[test]
fn n20_sob_vies_fecha_com_arvore_pequena() {
    let n = 20usize;
    let mut search = ExactSearch::new(u32::MAX);
    let mut total_nodes = 0u64;
    let instances = 5u64;
    for inst in 0..instances {
        let (j, h) = make_instance(n, 9, DELTA, SEED0.wrapping_add(inst));
        let r = search.solve(n, &j, &h);
        let bf = brute_force(n, &j, &h);
        assert!(!r.exhausted, "n=20 inst={inst} estourou");
        assert!(
            (r.energy - bf).abs() < 1e-9,
            "n=20 inst={inst}: exato={} bruto={}",
            r.energy,
            bf
        );
        total_nodes += u64::from(r.nodes);
    }
    let mean = total_nodes / instances;
    println!("[E2c3] n=20 mean_nodes={mean} (2^20 = 1048576)");
    assert!(
        mean < 20_000,
        "árvore n=20 grande demais: {mean} (2^20 = 1048576)"
    );
}
