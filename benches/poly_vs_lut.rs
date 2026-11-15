//! Onde o erro de 1,6e-3 do `poly_sin_q14` importa — medido, não arguido.
//!
//! O `bam_store` usa a parábola em dois lugares com naturezas diferentes:
//! * `cos_sum_q14` — similaridade agregada, entra em **argmax** (decisão);
//! * `unbind_into` — kernel de reconstrução, cujo resultado vira fase.
//!
//! Este probe mede três coisas, com as mesmas estatísticas do T12:
//! 1. erro da similaridade **média** (que é o que o argmax compara), parábola
//!    e LUT, contra referência `f64`;
//! 2. a margem real entre o 1º e o 2º valor que o store produz — se a margem
//!    for muito maior que o erro, a parábola não decide nada errado;
//! 3. custo de cada caminho no laço de similaridade.
//!
//! Execução: `cargo bench --no-default-features --features m1 --bench poly_vs_lut`

use std::hint::black_box;
use std::time::Instant;

use aether_engine::core::bam_phase::{
    poly_sin_q14, BamHolographicVector, BamLut, BamMask, BamPhase, LANES, Q14_ONE,
};

/// `π/2` em BAM (o `QUARTER` do módulo é `pub(crate)`).
const QUARTER: u16 = 0x4000;
use aether_engine::core::bam_store::BamHolographicStore;
use aether_engine::core::silicon_entropy::SiliconEntropy;

const D: usize = 256;
const FACTS: usize = 50;
const QUERIES: usize = 200;

/// `Σ cos(a−b)/D` pela parábola Q14, igual ao laço de `cos_sum_q14`.
fn cos_mean_poly(a: &[u16], b: &[u16]) -> f64 {
    let mut acc: i64 = 0;
    for (ca, cb) in a.chunks_exact(LANES).zip(b.chunks_exact(LANES)) {
        let mut s = [0i16; LANES];
        for l in 0..LANES {
            s[l] = ca[l].wrapping_sub(cb[l]).wrapping_add(QUARTER) as i16;
        }
        let c = poly_sin_q14(&s);
        for l in 0..LANES {
            acc += c[l] as i64;
        }
    }
    acc as f64 / Q14_ONE as f64 / a.len() as f64
}

/// Mesma soma, agora pela LUT de 1024 entradas.
fn cos_mean_lut(a: &[u16], b: &[u16]) -> f64 {
    let lut = BamLut::global();
    let mut acc = 0.0f64;
    for i in 0..a.len() {
        let d = BamPhase(a[i].wrapping_sub(b[i]));
        acc += lut.sin_cos(d).0 as f64;
    }
    acc / a.len() as f64
}

fn cos_mean_exact(a: &[u16], b: &[u16]) -> f64 {
    let step = std::f64::consts::TAU / 65_536.0;
    let mut acc = 0.0f64;
    for i in 0..a.len() {
        let d = (a[i].wrapping_sub(b[i]) as i16) as f64 * step;
        acc += d.cos();
    }
    acc / a.len() as f64
}

fn time<F: FnMut() -> u32>(mut f: F, reps: usize) -> f64 {
    for _ in 0..64 {
        black_box(f());
    }
    let mut best = f64::INFINITY;
    for _ in 0..reps {
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
    let mut rng = SiliconEntropy::from_seed(0xBEEF);

    // --- 1. erro da similaridade media (o que o argmax compara) ---
    let mut max_poly = 0.0f64;
    let mut max_lut = 0.0f64;
    let mut sum_poly = 0.0f64;
    let mut sum_lut = 0.0f64;
    let trials = 500;
    for _ in 0..trials {
        let x: Vec<u16> = (0..D).map(|_| rng.next_u64() as u16).collect();
        let y: Vec<u16> = (0..D).map(|_| rng.next_u64() as u16).collect();
        let exact = cos_mean_exact(&x, &y);
        let p = cos_mean_poly(&x, &y);
        let l = cos_mean_lut(&x, &y);
        max_poly = max_poly.max((p - exact).abs());
        max_lut = max_lut.max((l - exact).abs());
        sum_poly += (p - exact).abs();
        sum_lut += (l - exact).abs();
    }
    println!(
        "SIMILARIDADE MEDIA (D={D}, {trials} pares aleatorios):\n  \
parabola: erro max={max_poly:.3e} medio={:.3e}\n  LUT:      erro max={max_lut:.3e} medio={:.3e}",
        sum_poly / trials as f64,
        sum_lut / trials as f64
    );

    // --- 2. margem real do store (poly no caminho) ---
    let mut store = BamHolographicStore::<D>::new(6).expect("store");
    let mut atoms: Vec<(BamHolographicVector<D>, BamHolographicVector<D>, BamHolographicVector<D>)> =
        Vec::new();
    for _ in 0..FACTS {
        let e = BamHolographicVector::<D>::random(&mut rng);
        let r = BamHolographicVector::<D>::random(&mut rng);
        let v = BamHolographicVector::<D>::random(&mut rng);
        store.add_fact(&e, &r, &v);
        atoms.push((e, r, v));
    }
    let mut sc = store.scratch(FACTS);
    let mut margins: Vec<f64> = Vec::new();
    let mut sims: Vec<f64> = Vec::new();
    let mut erros = 0usize;
    for _ in 0..QUERIES {
        let truth = (rng.next_u64() as usize) % FACTS;
        let vis = D * 60 / 100;
        let hidden: Vec<bool> = (0..D).map(|_| rng.next_u64() % 100 < (100 * vis / D) as u64).collect();
        let mask = BamMask::<D>::from_bools(&hidden).expect("mask");
        let hit = store
            .query_into(&atoms[truth].0, &atoms[truth].1, &mask, &mut sc)
            .expect("query");
        if hit.value_index != truth {
            erros += 1;
        }
        margins.push(hit.margin as f64);
        sims.push(hit.similarity as f64);
    }
    margins.sort_by(f64::total_cmp);
    sims.sort_by(f64::total_cmp);
    let p = |v: &[f64], q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    println!(
        "STORE (50 fatos, K=6, 60% visivel, {QUERIES} consultas):\n  \
margem p1={:.4} p50={:.4} min={:.4} | similaridade p1={:.4} p50={:.4} | erros={erros}",
        p(&margins, 0.01),
        p(&margins, 0.50),
        p(&margins, 0.0),
        p(&sims, 0.01),
        p(&sims, 0.50)
    );
    println!(
        "  veredito: margem_minima/erro_parabola = {:.0}x",
        p(&margins, 0.0) / max_poly.max(1e-12)
    );

    // --- 3. custo dos dois caminhos ---
    let a: Vec<u16> = (0..D).map(|_| rng.next_u64() as u16).collect();
    let b: Vec<u16> = (0..D).map(|_| rng.next_u64() as u16).collect();
    let t_poly = time(|| cos_mean_poly(&a, &b).to_bits() as u32, 2000);
    let t_lut = time(|| cos_mean_lut(&a, &b).to_bits() as u32, 2000);
    println!(
        "CUSTO (D={D}): parabola={t_poly:.0} ns | LUT={t_lut:.0} ns | LUT/parabola={:.2}x",
        t_lut / t_poly
    );
}
