//! Scratch: por que o braço roof+BSB cai no regime sem viés?
use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::roof_dual_qumo;
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};

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

fn bf(n: usize, j: &[f64], h: &[f64]) -> f64 {
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

fn solve_bsb(
    res: &[usize],
    j: &[f64],
    h_eff: &[f64],
    n: usize,
    steps: usize,
    restarts: usize,
    seed: u64,
) -> Vec<i8> {
    let rn = res.len();
    if rn == 0 {
        return Vec::new();
    }
    let mut js = vec![0.0f64; rn * rn];
    let mut hs = vec![0.0f64; rn];
    for (a, &ia) in res.iter().enumerate() {
        hs[a] = h_eff[ia];
        for (b, &ib) in res.iter().enumerate() {
            js[a * rn + b] = j[ia * n + ib];
        }
    }
    let mut j3 = Array3::<f64>::zeros((1, rn, rn));
    let mut h2 = Array2::<f64>::zeros((1, rn));
    for (idx, chunk) in js.chunks(rn).enumerate() {
        for (k, &v) in chunk.iter().enumerate() {
            j3[[0, idx, k]] = v;
        }
    }
    for i in 0..rn {
        h2[[0, i]] = hs[i];
    }
    let mut solver = ThermodynamicIsingSolver::new(1, rn, steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, rn, steps);
    let mut best_e = f64::INFINITY;
    let mut best = vec![0i8; rn];
    for r in 0..restarts {
        solver.reset(seed.wrapping_mul(0x9E37_79B9).wrapping_add(r as u64));
        solver
            .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
            .expect("relax");
        if out.best_energy[0] < best_e {
            best_e = out.best_energy[0];
            for (i, b) in best.iter_mut().enumerate() {
                *b = out.states[[0, i]];
            }
        }
    }
    best
}

fn main() {
    let n = 14usize;
    let instances = 200usize;
    for (steps, restarts) in [(20usize, 4usize), (20, 8), (40, 8)] {
        let mut full = 0usize;
        let mut roof = 0usize;
        let mut roof_nofield = 0usize;
        for inst in 0..instances {
            let seed = 0xE2C_0001u64.wrapping_add(inst as u64);
            let (j, h) = glass(n, 0, 0.6, seed);
            let e_star = bf(n, &j, &h);

            let s_full = solve_bsb(&(0..n).collect::<Vec<_>>(), &j, &h, n, steps, restarts, seed);
            let mut sf = vec![0i8; n];
            sf.copy_from_slice(&s_full);
            if (energy(&j, &h, n, &sf) - e_star).abs() <= 1e-9 {
                full += 1;
            }

            let pre = roof_dual_qumo(n, &j, &h);
            let sol = solve_bsb(&pre.residual, &j, &pre.h_eff, n, steps, restarts, seed);
            let mut s = vec![0i8; n];
            for (i, f) in pre.fixed.iter().enumerate() {
                if let Some(v) = f {
                    s[i] = *v;
                }
            }
            for (k, &old) in pre.residual.iter().enumerate() {
                s[old] = sol[k];
            }
            if (energy(&j, &h, n, &s) - e_star).abs() <= 1e-9 {
                roof += 1;
            }

            // diagnóstico: mesmo resíduo, mas sem o campo injetado
            let h0 = vec![0.0f64; n];
            let sol0 = solve_bsb(&pre.residual, &j, &h0, n, steps, restarts, seed);
            let mut s0 = vec![0i8; n];
            for (i, f) in pre.fixed.iter().enumerate() {
                if let Some(v) = f {
                    s0[i] = *v;
                }
            }
            for (k, &old) in pre.residual.iter().enumerate() {
                s0[old] = sol0[k];
            }
            if (energy(&j, &h, n, &s0) - e_star).abs() <= 1e-9 {
                roof_nofield += 1;
            }
        }
        println!(
            "steps={steps} restarts={restarts}: BSB puro={full}/{instances}  roof+BSB={roof}/{instances}  roof+BSB(sem campo injetado)={roof_nofield}/{instances}"
        );
    }
}
