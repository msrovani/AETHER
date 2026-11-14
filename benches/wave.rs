//! Benchmark da interferencia espectral (Módulo 2). Preenchido na missão M5.
use criterion::{criterion_group, criterion_main, Criterion};

fn bench(c: &mut Criterion) {
    c.bench_function("wave_placeholder", |b| {
        b.iter(|| std::hint::black_box(1u64))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
