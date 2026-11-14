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

#[test]
fn probe_delta() {
    let patterns = [
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
    ];
    for eta_delta in [0.0f32, 0.02, 0.08, 0.3, 1.0] {
        for cycles in [40usize, 200] {
            let cfg = PhaseMemoryConfig {
                omega0: 0.0,
                clamp_strength: 8.0,
                eta_w: 0.12,
                eta_delta,
                gamma_w: 0.002,
            };
            let mut mem = ComplexPhaseMemory::with_config(N, cfg);
            for p in &patterns {
                mem.imprint(p, cycles);
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
                "[eta_delta={eta_delta} cycles={cycles}] mean p0={:.4} p1={:.4} p2={:.4} | worst p0={:.4} p1={:.4} p2={:.4}",
                means[0] / seeds as f32,
                means[1] / seeds as f32,
                means[2] / seeds as f32,
                worst[0],
                worst[1],
                worst[2]
            );
        }
    }
}
