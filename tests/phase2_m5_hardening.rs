#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

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

fn mask_exact_hidden(n: usize, seed: u64, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; n];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn ang_err(a: f32, b: f32) -> f32 {
    (a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn mean_err(recovered: &[f32], target: &[f32], mask: &[bool], n: usize) -> f32 {
    let mut s = 0.0;
    let mut c = 0;
    for i in 0..n {
        if mask[i] {
            continue;
        }
        s += ang_err(recovered[i], target[i]).abs();
        c += 1;
    }
    s / c as f32
}

fn eval(n: usize, pcount: usize, seed_count: u64) {
    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    };
    let patterns: Vec<Vec<f32>> = (0..pcount).map(|o| harmonic_pattern(n, o)).collect();
    let hidden = 3 * n / 8; // 37.5% mascarado
    let mut mem = ComplexPhaseMemory::with_config(n, cfg);
    for p in &patterns {
        mem.imprint(p, 40);
    }
    let mut means = [0.0f32; 5];
    let mut worst = [0.0f32; 5];
    for seed in 0..seed_count {
        for (k, p) in patterns.iter().enumerate() {
            let mask = mask_exact_hidden(n, seed, hidden);
            mem.reset_phases(0.0);
            let (rec, _e) = mem.recall(p, &mask, 600, 0.05);
            let e = mean_err(&rec, p, &mask, n);
            means[k] += e;
            worst[k] = worst[k].max(e);
        }
    }
    println!(
        "N={n} P={pcount} hidden={hidden} seeds={seed_count} | mean {} | worst {}",
        (0..pcount)
            .map(|k| format!("{:.4}", means[k] / seed_count as f32))
            .collect::<Vec<_>>()
            .join(" "),
        (0..pcount)
            .map(|k| format!("{:.4}", worst[k]))
            .collect::<Vec<_>>()
            .join(" ")
    );
}

#[test]
fn probe_scaling() {
    for pcount in [1usize, 2, 3] {
        eval(32, pcount, 30);
    }
    for n in [48usize, 64, 96] {
        eval(n, 3, 20);
    }
}
