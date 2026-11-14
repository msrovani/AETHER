//! Benchmark do solver termodinâmico (Módulo 1).
//!
//! Metas da Seção 3 da arquitetura:
//! * passo do integrador, `N=64`, lote 1: ≤ 50 µs (p99);
//! * relaxamento completo, 200 passos, `N=64`: ≤ 10 ms.
//!
//! O critério reporta mediana; o custo **por passo** é mediana/200 e o p99
//! pode ser obtido com `--verbose` (ou re-run com `--sample-size 100`).

use std::time::Duration;

use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// J simétrica determinística N(0, 1/sqrt(N)), diagonal zero.
fn symmetric_landscape(n: usize, batch: usize, seed: u64) -> (Array3<f64>, Array2<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = Array3::<f64>::zeros((batch, n, n));
    for b in 0..batch {
        for i in 0..n {
            for k in (i + 1)..n {
                let v = (rng.gen::<f64>() * 2.0 - 1.0) / (n as f64).sqrt();
                j[[b, i, k]] = v;
                j[[b, k, i]] = v;
            }
        }
    }
    (j, Array2::<f64>::zeros((batch, n)))
}

const STEPS: usize = 200;
const DT: f64 = 0.05;

fn bench_solver(c: &mut Criterion) {
    let mut group = c.benchmark_group("solver");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10);

    for &n in &[32usize, 64, 128] {
        for &batch in &[1usize, 8, 64] {
            let (j, h) = symmetric_landscape(n, batch, 0xBEA7 + n as u64 + batch as u64);
            let mut solver = ThermodynamicIsingSolver::new(batch, n, STEPS, DT);
            let mut out = RelaxOutcome::with_capacity(batch, n, STEPS);
            solver.reset(1);

            group.bench_with_input(
                BenchmarkId::new("relax_into", format!("N{}xB{}", n, batch)),
                &(n, batch),
                |b, _| {
                    b.iter(|| {
                        solver
                            .relax_into(IsingLandscape::new(j.view(), h.view()), &mut out)
                            .expect("relax_into")
                    })
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_solver);
criterion_main!(benches);
