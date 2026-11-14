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

fn mean_err(recovered: &[f32], target: &[f32], mask: &[bool]) -> f32 {
    let mut s = 0.0;
    let mut c = 0;
    for i in 0..N {
        if mask[i] {
            continue;
        }
        let d = (recovered[i] - target[i] + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        s += d.abs();
        c += 1;
    }
    s / c as f32
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
    for steps in [600usize, 1500] {
        let mut means = [0.0f32; 3];
        let mut worst = [0.0f32; 3];
        let mut fails = [0usize; 3];
        let mut spurious = [0usize; 3];
        let seeds = 30u64;
        for seed in 0..seeds {
            let mut mem = ComplexPhaseMemory::with_config(N, cfg);
            for p in &patterns {
                mem.imprint(p, 40);
            }
            for (k, p) in patterns.iter().enumerate() {
                let mask = mask_exact_hidden(seed, 12);
                mem.reset_phases(0.0);
                mem.phases.copy_from_slice(p);
                let e_true = mem.phase_energy();
                let (rec, e_rec) = mem.recall(p, &mask, steps, 0.05);
                let e = mean_err(&rec, p, &mask);
                means[k] += e;
                worst[k] = worst[k].max(e);
                if e >= 0.15 {
                    fails[k] += 1;
                }
                if e_rec < e_true - 1e-4 {
                    spurious[k] += 1;
                }
            }
        }
        for k in 0..3 {
            means[k] /= seeds as f32;
        }
        println!(
            "[steps={steps}] mean p0={:.4} p1={:.4} p2={:.4} | worst p0={:.4} p1={:.4} p2={:.4} | fails {} {} {} | spurious {} {} {}",
            means[0],
            means[1],
            means[2],
            worst[0],
            worst[1],
            worst[2],
            fails[0],
            fails[1],
            fails[2],
            spurious[0],
            spurious[1],
            spurious[2]
        );
    }
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    for p in &patterns {
        mem.imprint(p, 40);
    }
    let mut ws: Vec<f32> = mem.weights.clone();
    ws.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "weights: mean={:.3} median={:.3} max={:.3} p10={:.3} | mean_coupling={:.4}",
        ws.iter().sum::<f32>() / ws.len() as f32,
        ws[ws.len() / 2],
        ws[ws.len() - 1],
        ws[ws.len() / 10],
        mem.mean_coupling()
    );
}
