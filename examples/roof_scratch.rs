//! Scratch: gauge (complementação) muda o alcance do roof dual?
use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::{persistency_qumo, RoofDualWorkspace};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn glass(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
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

/// Aplica gauge `z ∈ {±1}^n`: `h'_i = z_i h_i`, `J'_ij = z_i z_j J_ij`.
fn gauge(n: usize, j: &[f64], h: &[f64], z: &[i8]) -> (Vec<f64>, Vec<f64>) {
    let mut j2 = vec![0.0f64; n * n];
    let mut h2 = vec![0.0f64; n];
    for i in 0..n {
        h2[i] = f64::from(z[i]) * h[i];
        for k in 0..n {
            j2[i * n + k] = f64::from(z[i] * z[k]) * j[i * n + k];
        }
    }
    (j2, h2)
}

fn solve_gauge(n: usize, j: &[f64], h: &[f64], z: &[i8]) -> usize {
    let (j2, h2) = gauge(n, j, h, z);
    let mut ws = RoofDualWorkspace::new();
    let mut fixed = vec![None::<i8>; n];
    ws.solve_into(n, &j2, &h2, &mut fixed);
    fixed.iter().filter(|f| f.is_some()).count()
}

fn main() {
    for plant in [0.0f64, 0.25, 0.45] {
        let n = 14usize;
        let k = (plant * n as f64).round() as usize;
        let instances = 40usize;
        let mut dom = 0.0f64;
        let mut base = 0.0f64;
        let mut by_field = 0.0f64;
        let mut alt = 0.0f64;
        let mut best_of_3 = 0.0f64;
        for inst in 0..instances {
            let (j, h) = glass(n, k, 0.6, 0xE2C_0001 + inst as u64);
            dom += persistency_qumo(n, &j, &h).fixed_count() as f64;
            let ident = vec![1i8; n];
            let b = solve_gauge(n, &j, &h, &ident);
            base += b as f64;
            // gauge pelos campos (alinha h em +)
            let zf: Vec<i8> = h.iter().map(|x| if *x < 0.0 { -1 } else { 1 }).collect();
            let f = solve_gauge(n, &j, &h, &zf);
            by_field += f as f64;
            // gauge alternado
            let za: Vec<i8> = (0..n).map(|i| if i % 2 == 0 { 1 } else { -1 }).collect();
            let a = solve_gauge(n, &j, &h, &za);
            alt += a as f64;
            best_of_3 += b.max(f).max(a) as f64;
        }
        let pct = |x: f64| 100.0 * x / (instances * n) as f64;
        println!(
            "plant={:>3.0}%  dom={:>5.1}%  roof_id={:>5.1}%  roof_campo={:>5.1}%  roof_alt={:>5.1}%  max3={:>5.1}%",
            100.0 * plant,
            pct(dom),
            pct(base),
            pct(by_field),
            pct(alt),
            pct(best_of_3)
        );
    }
    let _ = energy::<'_>;
}
