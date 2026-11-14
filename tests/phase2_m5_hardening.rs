#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const N: usize = 32;

fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (std::f32::consts::TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(std::f32::consts::TAU)
        })
        .collect()
}

fn mask_exact_hidden(seed: u64, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..N).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; N];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn ang_err(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn mean_err(recovered: &[f32], target: &[f32], mask: &[bool]) -> f32 {
    let mut s = 0.0;
    let mut c = 0;
    for i in 0..N {
        if mask[i] {
            continue;
        }
        s += ang_err(recovered[i], target[i]).abs();
        c += 1;
    }
    s / c as f32
}

fn drive_at(mem: &ComplexPhaseMemory, phases: &[f32], i: usize) -> f32 {
    let n = mem.n;
    let mut d = 0.0;
    for j in 0..n {
        if j == i {
            continue;
        }
        let idx = i * n + j;
        d += mem.weights[idx] * (phases[j] - phases[i] - mem.phase_shifts[idx]).sin();
    }
    d / n as f32
}

fn eval(
    label: &str,
    patterns: &[Vec<f32>],
    cfg: PhaseMemoryConfig,
    transform: impl Fn(f32) -> f32,
    pruna: f32,
) {
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    for p in patterns {
        mem.imprint(p, 40);
    }
    let mut res = [0.0f32; 3];
    for (k, p) in patterns.iter().enumerate() {
        let mut s = 0.0;
        for i in 0..N {
            s += drive_at(&mem, p, i).abs();
        }
        res[k] = s / N as f32;
    }
    // transforma os pesos (só leitura/escrita de campo público) e renormaliza
    let mut sum = 0.0f32;
    for (idx, w) in mem.weights.iter_mut().enumerate() {
        let i = idx / N;
        let j = idx % N;
        if i == j || *w <= pruna {
            *w = 0.0;
        } else {
            *w = transform(*w);
            sum += *w;
        }
    }
    if sum > 0.0 {
        let g = (N * N) as f32 / sum;
        for w in mem.weights.iter_mut() {
            *w *= g;
        }
    }
    let mut means = [0.0f32; 3];
    let mut worst = [0.0f32; 3];
    let seeds = 30u64;
    for seed in 0..seeds {
        for (k, p) in patterns.iter().enumerate() {
            let mask = mask_exact_hidden(seed, 12);
            mem.reset_phases(0.0);
            let (rec, _e) = mem.recall(p, &mask, 600, 0.05);
            let e = mean_err(&rec, p, &mask);
            means[k] += e;
            worst[k] = worst[k].max(e);
        }
    }
    println!(
        "[{label}] res p0={:.4} p1={:.4} p2={:.4} | mean p0={:.4} p1={:.4} p2={:.4} | worst p0={:.4} p1={:.4} p2={:.4}",
        res[0], res[1], res[2],
        means[0] / seeds as f32, means[1] / seeds as f32, means[2] / seeds as f32,
        worst[0], worst[1], worst[2]
    );
}

#[test]
fn probe_stats() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    };
    for p in [1.0f32, 1.5, 2.0, 3.0, 4.0] {
        eval(&format!("W^{p}"), &patterns, cfg, |w| w.powf(p), 0.0);
    }
    for th in [0.2f32, 0.4, 0.6, 0.8] {
        eval(&format!("prune<{th}"), &patterns, cfg, |w| w, th);
    }
    // gamma / ciclos
    for (eta, gamma, cycles) in [(0.12f32, 0.0f32, 40usize), (0.12, 0.01, 40)] {
        let c2 = PhaseMemoryConfig {
            gamma_w: gamma,
            ..cfg
        };
        let mut mem = ComplexPhaseMemory::with_config(N, c2);
        for p in &patterns {
            mem.imprint(p, cycles);
        }
        let _ = eta;
        eval(&format!("gamma={gamma}"), &patterns, c2, |w| w, 0.0);
        drop(mem);
    }
}
