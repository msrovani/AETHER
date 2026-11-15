//! Revisao independente de `core::bam_phase`: quanto o BAM realmente entrega.
//!
//! Mesmas cargas do `benches/bam_probe.rs` (que mede a versao ingenua em `f32`
//! escrita por mim), agora sobre a implementacao de verdade do CURSOR, para que
//! a comparacao seja maca-a-maca e nao contra um cosseno artificialmente caro.
//!
//! Nao depende de `m5`, entao roda mesmo com o `fhrr_bam` quebrado:
//! `cargo bench --no-default-features --features m1 --bench bam_phase_review`

use std::hint::black_box;
use std::time::Instant;

use aether_engine::core::bam_phase::{BamLut, BamPhase};

const N: usize = 4096;
const REPS: usize = 2000;

fn time<F: FnMut() -> u64>(mut f: F) -> f64 {
    // Aquecimento.
    for _ in 0..64 {
        black_box(f());
    }
    let mut best = f64::INFINITY;
    for _ in 0..REPS {
        let t = Instant::now();
        black_box(f());
        let dt = t.elapsed().as_nanos() as f64;
        if dt < best {
            best = dt;
        }
    }
    best
}

fn main() {
    let lut = BamLut::global();

    let ba: Vec<BamPhase> = (0..N)
        .map(|i| BamPhase::from_radians(i as f32 * 0.37))
        .collect();
    let bb: Vec<BamPhase> = (0..N)
        .map(|i| BamPhase::from_radians(i as f32 * 0.11))
        .collect();
    let fa: Vec<f32> = (0..N)
        .map(|i| (i as f32 * 0.37).rem_euclid(std::f32::consts::TAU))
        .collect();
    let fb: Vec<f32> = (0..N)
        .map(|i| (i as f32 * 0.11).rem_euclid(std::f32::consts::TAU))
        .collect();

    let t_f32_sincos = time(|| {
        let mut acc = 0.0f32;
        for p in fa.iter() {
            let (s, c) = p.sin_cos();
            acc += s * c;
        }
        acc.to_bits() as u64
    });

    let t_bam_lut = time(|| {
        let mut acc = 0i64;
        for p in ba.iter() {
            let (s, c) = lut.sin_cos_q15(*p);
            acc += (s as i64) * (c as i64);
        }
        acc as u64
    });

    let t_f32_add = time(|| {
        let mut acc = 0.0f32;
        for i in 0..N {
            acc += (fa[i] + fb[i]).rem_euclid(std::f32::consts::TAU);
        }
        acc.to_bits() as u64
    });

    let t_bam_add = time(|| {
        let mut acc: u16 = 0;
        for i in 0..N {
            acc = acc.wrapping_add(ba[i].wrapping_add(bb[i]).0);
        }
        acc as u64
    });

    println!("bam_phase (CURSOR) vs f32 ingenuo, N={N}, melhor de {REPS}");
    println!(
        "  sin_cos:      f32 {:8.2} us   BAM {:8.2} us   {:6.1}x",
        t_f32_sincos / 1000.0,
        t_bam_lut / 1000.0,
        t_f32_sincos / t_bam_lut
    );
    println!(
        "  binding:      f32 {:8.2} us   BAM {:8.2} us   {:6.1}x",
        t_f32_add / 1000.0,
        t_bam_add / 1000.0,
        t_f32_add / t_bam_add
    );
}
