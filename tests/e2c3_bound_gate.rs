//! E2c-3C — gates do **bound** do roof dual (`bound_into`).
//!
//! * casos analíticos: `n=1` (`H=−s`), `n=2` submodular (`H=−s₁s₂`) e `n=2`
//!   linear (`H=−s₁`) fecham no ótimo;
//! * 300 instâncias `n=14` (3 regimes): `bound ≤ ótimo` (nunca corta o ótimo);
//! * o gap é reportado: é o preço do LP, não erro.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::{RoofDualWorkspace, ROOF_MAX_N};

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
fn bound_bate_nos_casos_analiticos() {
    let mut ws = RoofDualWorkspace::new();
    let mut fixed = [None::<i8>; ROOF_MAX_N];

    // n=1, J=0, h=1: H(s) = −s, ótimo = −1.
    let b = ws.bound_into(1, &[0.0], &[1.0], &mut fixed[..1]);
    assert!((b + 1.0).abs() < 1e-9, "n=1: bound={b} (esperado −1)");

    // n=2, J=+1, h=0: H = −s₁s₂, ótimo = −1 (submodular, bound exato).
    let j = [0.0, 1.0, 1.0, 0.0];
    let h = [0.0, 0.0];
    let b = ws.bound_into(2, &j, &h, &mut fixed[..2]);
    assert!((b + 1.0).abs() < 1e-9, "n=2 submodular: bound={b}");

    // n=2, J=0, h=(1,0): H = −s₁, ótimo = −1 (só linear).
    let j = [0.0; 4];
    let h = [1.0, 0.0];
    let b = ws.bound_into(2, &j, &h, &mut fixed[..2]);
    assert!((b + 1.0).abs() < 1e-9, "n=2 linear: bound={b}");
}

#[test]
fn bound_nunca_excede_a_forca_bruta() {
    let mut ws = RoofDualWorkspace::new();
    let mut fixed = [None::<i8>; ROOF_MAX_N];
    let mut gap_sum = 0.0f64;
    let mut gap_max = 0.0f64;
    let mut cases = 0usize;
    for plant in [0.0f64, 0.25, 0.45] {
        let k = (plant * N as f64).round() as usize;
        for inst in 0..100u64 {
            let (j, h) = make_instance(N, k, DELTA, SEED0.wrapping_add(inst));
            let b = ws.bound_into(N, &j, &h, &mut fixed[..N]);
            let opt = brute_force(N, &j, &h);
            assert!(
                b.is_finite() && b <= opt + 1e-9,
                "bound={b} > ótimo={opt} (plant={plant} inst={inst})"
            );
            let gap = opt - b;
            gap_sum += gap;
            gap_max = gap_max.max(gap);
            cases += 1;
        }
    }
    println!(
        "[E2c3] bound ok em {cases} casos; gap médio={:.2} máx={:.2}",
        gap_sum / cases as f64,
        gap_max
    );
}
