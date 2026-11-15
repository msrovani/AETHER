//! SONDA TEMPORARIA (BUFFY) — diagnostico do recall do padrao harmonico 2.
//! Nao faz parte da suite: sera removida apos a investigacao.

#![cfg(feature = "m6")]

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

const N: usize = 32;
const HIDDEN: usize = 12;

fn harmonic(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (std::f32::consts::TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(std::f32::consts::TAU)
        })
        .collect()
}

fn ang(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d.abs()
}

fn mask_exact(seed: u64, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..N).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; N];
    for &i in idx.iter().take(hidden) {
        mask[i] = false;
    }
    mask
}

fn mean_err(rec: &[f32], tgt: &[f32], mask: &[bool]) -> f32 {
    let mut s = 0.0;
    let mut c = 0usize;
    for i in 0..tgt.len() {
        if mask[i] {
            continue;
        }
        s += ang(rec[i], tgt[i]);
        c += 1;
    }
    s / c as f32
}

/// Roda uma configuracao e devolve (media p0,p1,p2) e (pior p0,p1,p2).
fn run(
    label: &str,
    cycles: usize,
    steps: usize,
    dt: f32,
    cfg: PhaseMemoryConfig,
) -> (f32, f32, f32, f32) {
    let pats = [harmonic(N, 0), harmonic(N, 1), harmonic(N, 2)];
    let mut mem = ComplexPhaseMemory::with_config(N, cfg);
    for p in &pats {
        mem.imprint(p, cycles);
    }
    let seeds = 30u64;
    let mut sum = [0.0f32; 3];
    let mut worst = [0.0f32; 3];
    for seed in 0..seeds {
        let mask = mask_exact(seed, HIDDEN);
        for (k, p) in pats.iter().enumerate() {
            mem.reset_phases(0.0);
            let (rec, _e) = mem.recall(p, &mask, steps, dt);
            let e = mean_err(&rec, p, &mask);
            sum[k] += e;
            worst[k] = worst[k].max(e);
        }
    }
    let mean = [
        sum[0] / seeds as f32,
        sum[1] / seeds as f32,
        sum[2] / seeds as f32,
    ];
    let worst = [worst[0], worst[1], worst[2]];
    let worst_overall = worst.iter().cloned().fold(0.0f32, f32::max);
    println!(
        "{label:>34} | p0={:.4} p1={:.4} p2={:.4} | pior p0={:.4} p1={:.4} p2={:.4}",
        mean[0], mean[1], mean[2], worst[0], worst[1], worst[2]
    );
    (mean[0], mean[1], mean[2], worst_overall)
}

fn base_cfg() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

#[test]
fn sonda_parametros_recall() {
    println!("\n=== SONDA: causa do erro no padrao harmonico 2 ===");
    run("base (40c,600,s=.05,K=8)", 40, 600, 0.05, base_cfg());
    run("mais passos de recall (1800)", 40, 1800, 0.05, base_cfg());
    run("recall muito mais longo (4000)", 40, 4000, 0.05, base_cfg());
    run("dt menor (0.02, 1500 passos)", 40, 1500, 0.02, base_cfg());
    run("imprint mais relaxado (160c)", 160, 600, 0.05, base_cfg());
    run("imprint 160c + eta_delta .20", 160, 600, 0.05, {
        let mut c = base_cfg();
        c.eta_delta = 0.20;
        c
    });
    run("eta_delta .20 (40c)", 40, 600, 0.05, {
        let mut c = base_cfg();
        c.eta_delta = 0.20;
        c
    });
    run("clamp forte (K=20)", 40, 600, 0.05, {
        let mut c = base_cfg();
        c.clamp_strength = 20.0;
        c
    });
    run("clamp fraco (K=3)", 40, 600, 0.05, {
        let mut c = base_cfg();
        c.clamp_strength = 3.0;
        c
    });
    run("eta_w .30 (40c)", 40, 600, 0.05, {
        let mut c = base_cfg();
        c.eta_w = 0.30;
        c
    });
    println!("=== fim da sonda ===\n");
}
