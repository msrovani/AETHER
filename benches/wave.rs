//! Benchmark da interferência espectral (Módulo 2).
//!
//! Metas da Seção 3: FFT ida+volta, `C=8`, `L=2048`: ≤ 300 µs.
//! Mede o caminho quente (`forward_into`, zero alocação) e a entropia.

use std::time::Duration;

use aether_engine::core::wave_interference::WaveInterferenceBlock;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn deterministic_field(channels: usize, len: usize, seed: u64) -> Array2<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut out = Array2::<f32>::zeros((channels, len));
    for c in 0..channels {
        for k in 0..len {
            out[[c, k]] = rng.gen::<f32>() * 2.0 - 1.0;
        }
    }
    out
}

fn bench_wave(c: &mut Criterion) {
    let mut group = c.benchmark_group("wave");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10);

    for &l in &[128usize, 512, 2048, 8192] {
        let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, l, 0xC0FFEE);
        let u = deterministic_field(4, l, 0xF1E1D + l as u64);
        let mut v = Array2::<f32>::zeros((8, l));
        block.forward_into(u.view(), &mut v).expect("warmup");

        group.bench_with_input(BenchmarkId::new("forward_into", l), &l, |b, _| {
            b.iter(|| block.forward_into(u.view(), &mut v).expect("forward"))
        });
        group.bench_with_input(
            BenchmarkId::new("entropy", l),
            &l,
            |b, _| b.iter(|| block.compute_spectral_entropy(u.view()).expect("entropia")),
        );
    }
    group.finish();
}

/// Lote `[B, C_in, L]` — o caminho por item (o M4 usa B=1, mas o contrato
/// cobre lote).
fn bench_wave_batched(c: &mut Criterion) {
    let mut group = c.benchmark_group("wave_batched");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));
    group.sample_size(10);

    let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, 512, 0xC0FFEE);
    let mut ub = Array3::<f32>::zeros((8, 4, 512));
    let mut rng = ChaCha8Rng::seed_from_u64(0xB47C);
    for v in ub.iter_mut() {
        *v = rng.gen::<f32>() * 2.0 - 1.0;
    }
    group.bench_function("L512xB8", |b| {
        b.iter(|| block.forward_batched(ub.view()).expect("batched"))
    });
    group.finish();
}

criterion_group!(benches, bench_wave, bench_wave_batched);
criterion_main!(benches);
