//! E2c — gates da persistência de primeira ordem.
//!
//! * a regra fixa o que é dominado e **preserva o ótimo**;
//! * sem viés, não fixa nada (controle);
//! * soundness contra força bruta em instâncias com viés plantado.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::persistency_qumo;

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

fn optimum_with_fixed(
    n: usize,
    j: &[f64],
    h: &[f64],
    fixed: &[Option<i8>],
    residual: &[usize],
) -> f64 {
    let mut s = vec![0i8; n];
    for (i, f) in fixed.iter().enumerate() {
        if let Some(v) = f {
            s[i] = *v;
        }
    }
    let rn = residual.len();
    let mut best = f64::INFINITY;
    for mask in 0..(1u64 << rn) {
        for (k, &old) in residual.iter().enumerate() {
            s[old] = if (mask >> k) & 1 == 0 { -1 } else { 1 };
        }
        best = best.min(energy(j, h, n, &s));
    }
    best
}

#[test]
fn strong_bias_fixes_the_variable() {
    // J = [[0,1],[1,0]], h = (2, 0): em i=0, s_abs=1 e h=2 ⇒ h - s_abs = 1 > 0 ⇒ s_0 = +1.
    let j = vec![0.0, 1.0, 1.0, 0.0];
    let h = vec![2.0, 0.0];
    let p = persistency_qumo(2, &j, &h);
    assert_eq!(p.fixed[0], Some(1), "s_0 deveria ser fixado em +1");
    // cascata: com s_0 = +1, h_0→1 efetivo em s_1 = +1 e não sobra outro
    // acoplamento ⇒ s_1 também é dominado.
    assert_eq!(p.fixed[1], Some(1), "cascata deveria fixar s_1");

    let h_neg = vec![-2.0, 0.0];
    let p = persistency_qumo(2, &j, &h_neg);
    assert_eq!(p.fixed[0], Some(-1));
}

#[test]
fn no_bias_fixes_nothing() {
    // Vidro de spin puro: h = 0, J ±1 ⇒ nenhuma dominância.
    let n = 6usize;
    let mut rng = ChaCha8Rng::seed_from_u64(0xE2C_0001);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            j[i * n + k] = w;
            j[k * n + i] = w;
        }
    }
    let h = vec![0.0f64; n];
    let p = persistency_qumo(n, &j, &h);
    assert_eq!(p.fixed_count(), 0, "sem viés não há o que fixar");
    assert_eq!(p.residual.len(), n);
}

#[test]
fn persistency_preserves_optimum() {
    let n = 10usize;
    let instances = 120usize;
    let mut rng = ChaCha8Rng::seed_from_u64(0xE2C_0002);

    for _ in 0..instances {
        let mut j = vec![0.0f64; n * n];
        for i in 0..n {
            for k in (i + 1)..n {
                let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
                j[i * n + k] = w;
                j[k * n + i] = w;
            }
        }
        let mut h = vec![0.0f64; n];
        let k = 3usize; // 30% com viés forte
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
            h[v] = if rng.gen::<bool>() { 1.0 } else { -1.0 } * (row_abs + 0.6);
        }

        let e_star = brute_force(n, &j, &h);
        let p = persistency_qumo(n, &j, &h);
        let e_red = optimum_with_fixed(n, &j, &h, &p.fixed, &p.residual);

        assert!(
            (e_red - e_star).abs() < 1e-9,
            "fixação mudou o ótimo: {e_star} -> {e_red}"
        );
        assert!(p.fixed_count() >= k, "viés plantado não foi detectado");
    }
}
