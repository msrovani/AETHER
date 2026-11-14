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

fn eval(
    label: &str,
    cfg: PhaseMemoryConfig,
    profile: impl Fn(f32) -> f32,
    quantile_cut: f32,
    seeds: u64,
) {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    for p in &patterns {
        mem.imprint(p, 40);
    }
    if quantile_cut > 0.0 {
        let mut off: Vec<f32> = (0..N)
            .filter(|i| *i != 0)
            .map(|j| mem.weights[j])
            .collect();
        off.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let th = off[((off.len() as f32) * quantile_cut) as usize];
        mem.prune(th);
    }
    let mut sum = 0.0f32;
    for idx in 0..(N * N) {
        let i = idx / N;
        let j = idx % N;
        if i == j {
            continue;
        }
        mem.weights[idx] = profile(mem.weights[idx]);
        sum += mem.weights[idx];
    }
    if sum > 0.0 {
        let g = (N * N) as f32 / sum;
        for w in mem.weights.iter_mut() {
            *w *= g;
        }
    }
    let mut means = [0.0f32; 3];
    let mut worst = [0.0f32; 3];
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
        "[{label}] mean p0={:.4} p1={:.4} p2={:.4} | worst p0={:.4} p1={:.4} p2={:.4}",
        means[0] / seeds as f32,
        means[1] / seeds as f32,
        means[2] / seeds as f32,
        worst[0],
        worst[1],
        worst[2]
    );
}

fn cfg(gamma: f32) -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: gamma,
    }
}

#[test]
fn probe_stats() {
    eval("gamma=0 linear", cfg(0.0), |w| w, 0.0, 40);
    eval("gamma=0 W^0.5", cfg(0.0), |w| w.sqrt(), 0.0, 40);
    eval("gamma=0 W^0.75", cfg(0.0), |w| w.powf(0.75), 0.0, 40);
    eval("gamma=0 prune 20%", cfg(0.0), |w| w, 0.2, 40);
    eval("gamma=0 prune 40%", cfg(0.0), |w| w, 0.4, 40);
    eval("gamma=0.0005", cfg(0.0005), |w| w, 0.0, 40);
    eval("gamma=0.002", cfg(0.002), |w| w, 0.0, 40);
    eval(
        "default",
        PhaseMemoryConfig::default(),
        |w| w,
        0.0,
        40,
    );
}
