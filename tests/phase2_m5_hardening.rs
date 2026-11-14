#![cfg(all(feature = "m5", feature = "m6", feature = "m7"))]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
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

fn random_pattern(n: usize, seed: u64) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..n)
        .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
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

fn run(label: &str, patterns: &[Vec<f32>], dt: f32, steps: usize, seeds: u64) {
    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    };
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    for p in patterns {
        mem.imprint(p, 40);
    }
    let mut means = [0.0f32; 3];
    let mut worst = [0.0f32; 3];
    let mut fails = [0usize; 3];
    for seed in 0..seeds {
        for (k, p) in patterns.iter().enumerate().take(3) {
            let mask = mask_exact_hidden(seed, 12);
            mem.reset_phases(0.0);
            let (rec, _e) = mem.recall(p, &mask, steps, dt);
            let e = mean_err(&rec, p, &mask);
            means[k] += e;
            worst[k] = worst[k].max(e);
            if e >= 0.15 {
                fails[k] += 1;
            }
        }
    }
    println!(
        "[{label} dt={dt} steps={steps} seeds={seeds}] mean p0={:.4} p1={:.4} p2={:.4} | worst p0={:.4} p1={:.4} p2={:.4} | fails {} {} {}",
        means[0] / seeds as f32,
        means[1] / seeds as f32,
        means[2] / seeds as f32,
        worst[0],
        worst[1],
        worst[2],
        fails[0],
        fails[1],
        fails[2]
    );
}

#[test]
fn probe_stats() {
    let harm = vec![
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    for (dt, steps) in [
        (0.05f32, 600usize),
        (0.02, 1500),
        (0.01, 3000),
        (0.005, 6000),
    ] {
        run("harm", &harm, dt, steps, 60);
    }
    let rnd = vec![
        random_pattern(N, 1),
        random_pattern(N, 2),
        random_pattern(N, 3),
    ];
    run("rand", &rnd, 0.05, 600, 60);
}
