//! Benchmark do `recall` do M5: kernel matricial complexo x laço antigo.
//!
//! O `integrate` passou a somar o acoplamento como
//! `Im(e^{−iφ_i} · Σ_j A_ij e^{iφ_j})`, com `A_ij = W_ij·e^{−iδ_ij}`
//! pré-calculado: um `sin_cos` por nó por passo em vez de um `sin` por **aresta**.
//!
//! Este bench reimplementa o laço anterior (`legacy`) com os campos públicos
//! `weights`/`phase_shifts` da mesma memória e roda os dois lado a lado. Os dois
//! chegam ao mesmo atrator; a diferença é a ordem de integração (o antigo
//! atualizava as fases no lugar, Gauss-Seidel; o kernel é simultâneo, Jacobi).
//!
//! Execução: `cargo bench --bench phase_recall`

use std::time::Duration;

use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

const TAU: f32 = std::f32::consts::TAU;

fn cfg() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.001,
    }
}

fn wrap(x: f32) -> f32 {
    let mut v = x % TAU;
    if v < 0.0 {
        v += TAU;
    }
    v
}

fn wrap_signed(x: f32) -> f32 {
    let mut v = x % TAU;
    if v > TAU / 2.0 {
        v -= TAU;
    } else if v < -TAU / 2.0 {
        v += TAU;
    }
    v
}

/// Laço anterior: um `sin` por aresta, atualização no lugar (Gauss-Seidel).
#[allow(clippy::too_many_arguments)]
fn legacy_integrate(
    phases: &mut [f32],
    cue: &[f32],
    mask: &[bool],
    steps: usize,
    dt: f32,
    clamp: f32,
    weights: &[f32],
    shifts: &[f32],
    n: usize,
) {
    for i in 0..n {
        if mask[i] {
            phases[i] = wrap(cue[i]);
        }
    }
    let inv_n = 1.0 / n as f32;
    for _ in 0..steps {
        for i in 0..n {
            let mut drive = 0.0f32;
            let base = i * n;
            for j in 0..n {
                if j == i {
                    continue;
                }
                let idx = base + j;
                let dphi = phases[j] - phases[i] - shifts[idx];
                drive += weights[idx] * dphi.sin();
            }
            drive *= inv_n;
            if mask[i] {
                let err = wrap_signed(wrap(cue[i]) - phases[i]);
                drive += clamp * err.sin();
            }
            phases[i] = wrap(phases[i] + drive * dt);
        }
    }
}

fn build(n: usize, patterns: usize) -> (ComplexPhaseMemory, Vec<f32>, Vec<bool>) {
    let mut mem = ComplexPhaseMemory::with_config(n, cfg());
    let pats: Vec<Vec<f32>> = (0..patterns)
        .map(|k| {
            (0..n)
                .map(|i| {
                    let x = i as f32 / n as f32;
                    (TAU * (k as f32 + 1.0) * x + 0.37 * k as f32).rem_euclid(TAU)
                })
                .collect()
        })
        .collect();
    for p in &pats {
        mem.imprint(p, 16);
    }
    let cue = pats[0].clone();
    // 40% oculto, determinístico.
    let mask: Vec<bool> = (0..n).map(|i| !(i % 5 < 2)).collect();
    (mem, cue, mask)
}

fn bench_recall(c: &mut Criterion) {
    let mut group = c.benchmark_group("m5_recall");
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    group.sample_size(15);

    for (n, patterns, steps) in [(32usize, 5usize, 600usize), (64, 8, 200), (200, 50, 80)] {
        let (mut mem, cue, mask) = build(n, patterns);
        let weights = mem.weights.clone();
        let shifts = mem.phase_shifts.clone();
        let clamp = cfg().clamp_strength;
        let dt = 0.05f32;
        let mut out = vec![0.0f32; n];
        let mut legacy = vec![0.0f32; n];

        // Sanidade: os dois chegam ao mesmo atrator?
        mem.reset_phases(0.0);
        mem.recall_into(&cue, &mask, steps, dt, &mut out);
        legacy.fill(0.0);
        legacy_integrate(
            &mut legacy,
            &cue,
            &mask,
            steps,
            dt,
            clamp,
            &weights,
            &shifts,
            n,
        );
        let mut max_d = 0.0f32;
        for i in 0..n {
            let mut d = (legacy[i] - out[i]).abs() % TAU;
            if d > TAU / 2.0 {
                d = TAU - d;
            }
            max_d = max_d.max(d);
        }
        println!(
            "N={n} passos={steps} ocultos={}%: divergência kernel x laço antigo = {max_d:.3e} rad",
            100 - mask.iter().filter(|m| **m).count() * 100 / n
        );

        let id = format!("N{n}");
        group.bench_with_input(BenchmarkId::new("kernel", &id), &n, |b, _| {
            b.iter(|| {
                mem.reset_phases(0.0);
                mem.recall_into(&cue, &mask, steps, dt, &mut out);
            });
        });
        group.bench_with_input(BenchmarkId::new("legacy", &id), &n, |b, _| {
            b.iter(|| {
                legacy.fill(0.0);
                legacy_integrate(
                    &mut legacy,
                    &cue,
                    &mask,
                    steps,
                    dt,
                    clamp,
                    &weights,
                    &shifts,
                    n,
                );
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_recall);
criterion_main!(benches);
