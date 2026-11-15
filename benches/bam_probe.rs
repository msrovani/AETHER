//! Sonda: fases em `f32` (radianos) contra fases inteiras BAM (`u16`).
//!
//! Mede, neste CPU, o custo real das três operações que dominam o caminho
//! quente do `phase_memory`/`fhrr`:
//!
//! * `sin_cos` por elemento (o `sin_cos` do `integrate`/`fhrr`);
//! * normalização de ângulo (`rem_euclid` sobre `f32`, o "imposto do círculo");
//! * similaridade por cosseno de diferença (o `cleanup` do FHRR).
//!
//! A versão BAM guarda a fase como `u16` (0x0000 = 0, 0x4000 = π/2,
//! 0x8000 = π, 0xC000 = 3π/2). Somar vira `wrapping_add`; o módulo 2π é
//! grátis; a trigonometria vira lookup numa tabela `i16` de 257 entradas
//! (514 B, cabe inteira na L1d). Tudo em Rust seguro — a vetorização AVX2
//! (`vpaddw`, 16 pistas) vem do LLVM, não de intrínsecos.
//!
//! Execução: `cargo bench --bench bam_probe`

use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

const TAU: f32 = std::f32::consts::TAU;

/// Tabela `cos` para BAM de 8 bits (256 passos), em `i16` com escala 1<<14.
struct Lut {
    cos: [i16; 256],
}

impl Lut {
    fn new() -> Self {
        let mut cos = [0i16; 256];
        for (i, c) in cos.iter_mut().enumerate() {
            let theta = TAU * i as f32 / 256.0;
            *c = (theta.cos() * 16384.0) as i16;
        }
        Self { cos }
    }

    /// `cos(2π·phase/65536)` por lookup na L1.
    #[inline(always)]
    fn cos_bam(&self, phase: u16) -> f32 {
        self.cos[(phase >> 8) as usize] as f32
    }
}

fn setup(n: usize) -> (Vec<f32>, Vec<f32>, Vec<u16>, Vec<u16>) {
    let fa: Vec<f32> = (0..n).map(|i| (i as f32 * 0.37).rem_euclid(TAU)).collect();
    let fb: Vec<f32> = (0..n).map(|i| (i as f32 * 0.11).rem_euclid(TAU)).collect();
    let ba: Vec<u16> = (0..n)
        .map(|i| ((i as f32 * 0.37 / TAU) * 65536.0) as u16)
        .collect();
    let bb: Vec<u16> = (0..n)
        .map(|i| ((i as f32 * 0.11 / TAU) * 65536.0) as u16)
        .collect();
    (fa, fb, ba, bb)
}

fn bench_ops(c: &mut Criterion) {
    let lut = Lut::new();
    let mut group = c.benchmark_group("bam_vs_f32");
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_millis(800));
    group.sample_size(20);

    for n in [256usize, 4096] {
        let (fa, fb, ba, bb) = setup(n);
        let id = format!("N{n}");

        // 1. trigonometria: sin_cos em f32 x lookup em tabela BAM.
        group.bench_with_input(BenchmarkId::new("f32_sin_cos", &id), &n, |b, _| {
            b.iter(|| {
                let mut acc = 0.0f32;
                for p in fa.iter() {
                    let (s, c) = p.sin_cos();
                    acc += s * c;
                }
                black_box(acc)
            });
        });
        group.bench_with_input(BenchmarkId::new("bam_lut_cos", &id), &n, |b, _| {
            b.iter(|| {
                let mut acc = 0.0f32;
                for p in ba.iter() {
                    acc += lut.cos_bam(*p);
                }
                black_box(acc)
            });
        });

        // 2. binding (soma de fases) + normalização do ângulo.
        group.bench_with_input(BenchmarkId::new("f32_bind_rem_euclid", &id), &n, |b, _| {
            b.iter(|| {
                let mut acc = 0.0f32;
                for i in 0..n {
                    acc += (fa[i] + fb[i]).rem_euclid(TAU);
                }
                black_box(acc)
            });
        });
        group.bench_with_input(BenchmarkId::new("bam_wrapping_add", &id), &n, |b, _| {
            b.iter(|| {
                let mut acc: u16 = 0;
                for i in 0..n {
                    acc = acc.wrapping_add(ba[i].wrapping_add(bb[i]));
                }
                black_box(acc)
            });
        });

        // 3. similaridade por diferença de fase (cleanup do FHRR / recall do M5).
        group.bench_with_input(BenchmarkId::new("f32_similarity", &id), &n, |b, _| {
            b.iter(|| {
                let mut acc = 0.0f32;
                for i in 0..n {
                    acc += (fa[i] - fb[i]).cos();
                }
                black_box(acc)
            });
        });
        group.bench_with_input(BenchmarkId::new("bam_similarity", &id), &n, |b, _| {
            b.iter(|| {
                let mut acc = 0.0f32;
                for i in 0..n {
                    acc += lut.cos_bam(ba[i].wrapping_sub(bb[i]));
                }
                black_box(acc)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_ops);
criterion_main!(benches);
