//! Benchmarks do Módulo 2 — interferência espectral (operador de Fourier).
//!
//! Mede o caminho quente `forward_into` (sem alocação, planos FFT cacheados) e
//! as duas entropias, para `C_in = 4`, `C_out = 8`, `modes = 32` e
//! `L ∈ {128, 512, 2048, 8192}`.
//!
//! Meta de referência: FFT ida+volta com `C = 8`, `L = 2048` ≤ 300 µs.

use std::hint::black_box;

use aether_engine::core::wave_interference::WaveInterferenceBlock;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use ndarray::Array2;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const C_IN: usize = 4;
const C_OUT: usize = 8;
const MODES: usize = 32;
const MAX_LEN: usize = 8192;

fn field(c: usize, l: usize, seed: u64) -> Array2<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    Array2::<f32>::from_shape_fn((c, l), |_| rng.gen::<f32>() * 2.0 - 1.0)
}

fn bench_forward(c: &mut Criterion) {
    let mut group = c.benchmark_group("wave_forward_into");
    group.sample_size(20);

    for &l in &[128usize, 512, 2048, 8192] {
        let mut block = WaveInterferenceBlock::with_max_len(C_IN, C_OUT, MODES, MAX_LEN, 0xC0FFEE);
        let u = field(C_IN, l, 0xA11CE);
        let mut out = Array2::<f32>::zeros((C_OUT, l));
        block.forward_into(u.view(), &mut out).expect("warmup");

        group.bench_with_input(BenchmarkId::from_parameter(l), &l, |bh, _| {
            bh.iter(|| {
                block
                    .forward_into(u.view(), &mut out)
                    .expect("forward_into");
                black_box(out[[0, 0]])
            })
        });
    }

    group.finish();
}

fn bench_entropy(c: &mut Criterion) {
    let mut group = c.benchmark_group("wave_entropia");
    group.sample_size(20);

    for &l in &[128usize, 512, 2048, 8192] {
        let mut block = WaveInterferenceBlock::with_max_len(C_IN, C_OUT, MODES, MAX_LEN, 0xBEEF);
        let u = field(C_IN, l, 0x5EED);
        let mut out = Array2::<f32>::zeros((C_OUT, l));
        block.forward_into(u.view(), &mut out).expect("warmup");

        group.bench_with_input(BenchmarkId::new("espectral", l), &l, |bh, _| {
            bh.iter(|| {
                black_box(
                    block
                        .compute_spectral_entropy(u.view())
                        .expect("compute_spectral_entropy"),
                )
            })
        });
        group.bench_with_input(BenchmarkId::new("transferida", l), &l, |bh, _| {
            bh.iter(|| black_box(block.compute_wave_entropy()))
        });
    }

    group.finish();
}

criterion_group!(benches, bench_forward, bench_entropy);
criterion_main!(benches);
