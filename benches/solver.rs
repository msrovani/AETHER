//! Benchmarks do Módulo 1 — relaxamento termodinâmico (bSB).
//!
//! Mede o custo do caminho quente `reset` + `relax_into` (buffers reutilizados,
//! zero alocação) para as combinações `(B, N)` do ARCHITECTURE §3:
//! `N ∈ {32, 64, 128}`, `B ∈ {1, 8, 64}`, sempre com 200 passos.
//!
//! Metas de referência (mediana em hardware de desenvolvimento):
//! * passo do integrador, `N = 64`, lote 1  ≤ 50 µs
//! * relaxamento completo, 200 passos        ≤ 10 ms

use std::hint::black_box;

use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const STEPS: usize = 200;
const DT: f64 = 0.05;

/// `J` simétrica (diagonal zero) e `h` pequeno, ambos determinísticos.
fn landscapes(batch: usize, n: usize, seed: u64) -> (Array3<f64>, Array2<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let scale = 1.0 / (n as f64).sqrt();
    let mut j = Array3::<f64>::zeros((batch, n, n));
    let mut h = Array2::<f64>::zeros((batch, n));
    for b in 0..batch {
        for i in 0..n {
            h[[b, i]] = (rng.gen::<f64>() * 2.0 - 1.0) * 0.1;
            for k in (i + 1)..n {
                let v = (rng.gen::<f64>() * 2.0 - 1.0) * scale;
                j[[b, i, k]] = v;
                j[[b, k, i]] = v;
            }
        }
    }
    (j, h)
}

fn bench_relax(c: &mut Criterion) {
    let mut group = c.benchmark_group("solver_relax_into_200_passos");
    group.sample_size(20);

    let cases: [(usize, usize); 4] = [(1, 64), (1, 128), (8, 64), (64, 32)];
    for (batch, n) in cases {
        let (j, h) = landscapes(batch, n, 0xA37E_0001);
        let mut solver = ThermodynamicIsingSolver::new(batch, n, STEPS, DT);
        let mut out = RelaxOutcome::with_capacity(batch, n, STEPS);
        let label = format!("b{batch}_n{n}");

        // aquecimento fora da medição
        solver.reset(0xDEAD_BEEF);
        solver
            .relax_into(IsingLandscape::new(j.view(), h.view()), &mut out)
            .expect("warmup relax_into");

        group.bench_with_input(
            BenchmarkId::new("relax_into", label),
            &(batch, n),
            |bh, _| {
                bh.iter(|| {
                    solver.reset(0xDEAD_BEEF);
                    solver
                        .relax_into(IsingLandscape::new(j.view(), h.view()), &mut out)
                        .expect("relax_into");
                    black_box(out.best_energy[0])
                })
            },
        );
    }

    group.finish();
}

fn bench_energy_eval(c: &mut Criterion) {
    const N: usize = 64;
    let (j, h) = landscapes(1, N, 0x5150);
    let j2 = j.index_axis(ndarray::Axis(0), 0).to_owned();
    // O contrato de `energy` espera `h: [1, N]` (uma instância do lote).
    let h2 = h.view().into_shape_with_order((1, N)).expect("shape h");
    let solver = ThermodynamicIsingSolver::new(1, N, STEPS, DT);
    let mut s = vec![0i8; N];
    for (i, v) in s.iter_mut().enumerate() {
        *v = if i % 3 == 0 { 1 } else { -1 };
    }

    c.bench_function("solver_energy_n64", |bh| {
        bh.iter(|| black_box(solver.energy(&j2.view(), &h2.view(), black_box(&s))))
    });
}

criterion_group!(benches, bench_relax, bench_energy_eval);
criterion_main!(benches);
