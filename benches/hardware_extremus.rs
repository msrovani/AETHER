//! Benchmark da FASE 5 (hardware extremus): BAM, entropia, JIT e SPSC.
//!
//! Mede, sem descarte de amostras (percentis reais, protocolo declarado):
//!
//! 1. micro-operações BAM (D=256): `sin_cos` f32 vs LUT, bind, similaridade;
//! 2. T12 (RAG holográfico): 50 fatos, K=6, 40 % visível, ±0,25 rad,
//!    p50/p95/p99 por consulta em D = 160 / 256 / 512, e o baseline
//!    `FhrrStore` f32 em D=160;
//! 3. entropia de silício (ns/gaussiana) vs. semente determinística;
//! 4. `dx = Θ(x)·Ξ` SINDy: nativo vs. interpretado vs. JIT (ns/avaliação);
//! 5. SPSC: vazão produtor→consumidor entre duas threads (Mmsg/s).
//!
//! `harness = false` (laço único com `Instant`, mesma convenção de
//! `throughput_tps`). Saída: tabela no terminal e
//! `target/aether_bench_hardware.json`.
//!
//! Execução: `cargo bench --bench hardware_extremus`
//! (com `RUSTFLAGS="-C target-cpu=native"` para o AVX2 da máquina).

use std::fs;
use std::hint::black_box;
use std::time::Instant;

use aether_engine::core::bam_phase::{
    BamHolographicStore, BamHolographicVector, BamLut, BamMask, BamPhase,
};
use aether_engine::core::fhrr::FhrrStore;
use aether_engine::core::silicon_entropy::SiliconEntropy;
use aether_engine::core::sindy_jit::{eval_interpreted, jit_compiled_in, SindyJit};
use aether_engine::core::spsc_ring::SpscRingBuffer;

const FACTS: usize = 50;
const QUERIES: usize = 200;
const REPS: usize = 10;

fn pct(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// ns por iteração de `f`, mediana de 15 lotes de `iters`.
fn ns_per_iter(iters: u32, mut f: impl FnMut()) -> f64 {
    let mut batches: Vec<f64> = (0..15)
        .map(|_| {
            let t = Instant::now();
            for _ in 0..iters {
                f();
            }
            t.elapsed().as_nanos() as f64 / iters as f64
        })
        .collect();
    batches.sort_by(|a, b| a.total_cmp(b));
    batches[batches.len() / 2]
}

fn rand_rad(rng: &mut SiliconEntropy, d: usize) -> Vec<f32> {
    (0..d)
        .map(|_| rng.next_f32() * std::f32::consts::TAU)
        .collect()
}

fn mask_40(d: usize, seed: u64) -> Vec<bool> {
    let mut rng = SiliconEntropy::from_seed(seed);
    let mut idx: Vec<usize> = (0..d).collect();
    for i in (1..d).rev() {
        idx.swap(i, (rng.next_u64() % (i as u64 + 1)) as usize);
    }
    let mut m = vec![false; d];
    idx.iter().take(d * 40 / 100).for_each(|&i| m[i] = true);
    m
}

/// Cenário T12. Devolve `(recall, p50, p95, p99)` em µs. `bam = false` ⇒ FHRR f32.
fn t12<const D: usize>(bam: bool) -> (f64, f64, f64, f64) {
    let mut rng = SiliconEntropy::from_seed(0xFAC7);
    let mut atoms = Vec::new();
    let mut bs = BamHolographicStore::<D>::new(6).expect("bam");
    let mut fs = FhrrStore::with_traces(D, 6).expect("fhrr");
    for _ in 0..FACTS {
        let (e, r, v) = (
            rand_rad(&mut rng, D),
            rand_rad(&mut rng, D),
            rand_rad(&mut rng, D),
        );
        if bam {
            let f = |x: &[f32]| BamHolographicVector::<D>::from_radians(x).expect("rad");
            bs.add_fact(&f(&e), &f(&r), &f(&v));
        } else {
            fs.add_fact(&e, &r, &v).expect("add");
        }
        atoms.push((e, r));
    }
    let (mut bsc, mut fsc) = (bs.scratch(FACTS), fs.scratch(FACTS));
    let (mut samples, mut hits) = (Vec::new(), 0usize);
    for q in 0..QUERIES {
        let truth = (rng.next_u64() % FACTS as u64) as usize;
        let mb = mask_40(D, 0x7000 + q as u64);
        let (mut ec, mut rc) = atoms[truth].clone();
        for i in (0..D).filter(|&i| mb[i]) {
            let n = (rng.next_f32() - 0.5) * 0.5;
            ec[i] += n;
            rc[i] -= n;
        }
        let (be, br) = (
            BamHolographicVector::<D>::from_radians(&ec).expect("e"),
            BamHolographicVector::<D>::from_radians(&rc).expect("r"),
        );
        let bm = BamMask::<D>::from_bools(&mb).expect("mask");
        for _ in 0..REPS {
            let t = Instant::now();
            let got = if bam {
                black_box(bs.query_into(&be, &br, &bm, &mut bsc).expect("q")).value_index
            } else {
                black_box(fs.query_into(&ec, &rc, &mb, &mut fsc).expect("q")).value_index
            };
            samples.push(t.elapsed().as_nanos() as f64 / 1000.0);
            hits += usize::from(got == truth);
        }
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    (
        hits as f64 / (QUERIES * REPS) as f64,
        pct(&samples, 0.50),
        pct(&samples, 0.95),
        pct(&samples, 0.99),
    )
}

fn bench_bam_micro() -> [(&'static str, f64); 5] {
    let mut rng = SiliconEntropy::from_seed(11);
    let a = BamHolographicVector::<256>::random(&mut rng);
    let b = BamHolographicVector::<256>::random(&mut rng);
    let mask = BamMask::<256>::all();
    let lut = BamLut::global();
    let radians = rand_rad(&mut rng, 256);
    let phases: Vec<BamPhase> = radians.iter().map(|&r| BamPhase::from_radians(r)).collect();
    let f32_sc = ns_per_iter(2000, || {
        let mut acc = 0.0f32;
        for &r in &radians {
            let (s, c) = black_box(r).sin_cos();
            acc += s + c;
        }
        black_box(acc);
    });
    let lut_sc = ns_per_iter(20_000, || {
        let mut acc = 0.0f32;
        for &p in &phases {
            let (s, c) = lut.sin_cos(black_box(p));
            acc += s + c;
        }
        black_box(acc);
    });
    let bind = ns_per_iter(200_000, || {
        black_box(black_box(&a).bind(black_box(&b)));
    });
    let sim = ns_per_iter(200_000, || {
        black_box(black_box(&a).similarity_masked(black_box(&b), &mask));
    });
    let unb = ns_per_iter(200_000, || {
        black_box(black_box(&a).unbind(black_box(&b)));
    });
    [
        ("sin_cos_f32_x256", f32_sc),
        ("sin_cos_lut_x256", lut_sc),
        ("bind_256", bind),
        ("unbind_256", unb),
        ("similarity_256", sim),
    ]
}

fn bench_entropy() -> (f64, f64) {
    let mut hw = SiliconEntropy::new();
    let mut det = SiliconEntropy::from_seed(3);
    (
        ns_per_iter(100_000, || {
            black_box(hw.next_gaussian());
        }),
        ns_per_iter(100_000, || {
            black_box(det.next_gaussian());
        }),
    )
}

fn bench_sindy() -> (f64, f64, f64, bool) {
    let mut xi = [[0.0f64; 10]; 2];
    xi[0][2] = 1.0; // x' = y
    xi[1][1] = 1.0; // y' = x − x³ − 0,3 y
    xi[1][6] = -1.0;
    xi[1][2] = -0.3;
    let (mut jit, integ) = SindyJit::new();
    let out = jit.publish_coefficients(&xi).expect("publish");
    let mut xi32 = [0.0f32; 20];
    for (c, row) in xi.iter().enumerate() {
        for (t, v) in row.iter().enumerate() {
            xi32[c * 10 + t] = *v as f32;
        }
    }
    let (x, mut dx) = ([0.7f32, -0.2], [0.0f32; 2]);
    let native = ns_per_iter(2_000_000, || {
        let (a, b) = (black_box(x[0]), black_box(x[1]));
        dx = [b, a - a * a * a - 0.3 * b];
        black_box(&dx);
    });
    let interp = ns_per_iter(2_000_000, || {
        eval_interpreted(black_box(&xi32), black_box(&x), &mut dx);
        black_box(&dx);
    });
    let jitted = ns_per_iter(2_000_000, || {
        integ.eval(black_box(&x), &mut dx);
        black_box(&dx);
    });
    (
        native,
        interp,
        jitted,
        matches!(
            out,
            aether_engine::core::sindy_jit::PublishOutcome::Jitted { .. }
        ),
    )
}

fn bench_spsc() -> f64 {
    const N: u64 = 5_000_000;
    let (mut tx, mut rx) = SpscRingBuffer::<u64, 1024>::new().split();
    let t = Instant::now();
    let h = std::thread::spawn(move || {
        let mut sum = 0u64;
        let mut got = 0u64;
        while got < N {
            if let Some(v) = rx.try_pop() {
                sum = sum.wrapping_add(v);
                got += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        sum
    });
    for i in 0..N {
        while tx.try_push(i).is_err() {
            std::hint::spin_loop();
        }
    }
    black_box(h.join().expect("consumidor"));
    N as f64 / t.elapsed().as_secs_f64() / 1e6
}

fn main() {
    println!("== AETHER FASE 5 — hardware extremus ==");
    let micro = bench_bam_micro();
    for (n, v) in micro {
        println!("[bam] {n:<18} {v:>10.1} ns");
    }
    println!(
        "[bam] ganho sin_cos LUT x{:.0} | jit_compilado={}",
        micro[0].1 / micro[1].1,
        jit_compiled_in()
    );
    let mut json = String::from("{\n");
    for (n, v) in micro {
        json += &format!("  \"bam_{n}_ns\": {v:.2},\n");
    }
    let (bam160, bam256, bam512, fh160) = (
        t12::<160>(true),
        t12::<256>(true),
        t12::<512>(true),
        t12::<160>(false),
    );
    println!(
        "[T12] protocolo: {FACTS} fatos, K=6, 40% visivel, +-0.25 rad, n={}",
        QUERIES * REPS
    );
    for (n, r) in [
        ("BAM D=160", bam160),
        ("BAM D=256", bam256),
        ("BAM D=512", bam512),
        ("FHRR f32 D=160", fh160),
    ] {
        println!(
            "[T12] {n:<15} recall={:.3} p50={:.2} us p95={:.2} us p99={:.2} us",
            r.0, r.1, r.2, r.3
        );
    }
    println!("[T12] speedup D=160 (p50): x{:.2}", fh160.1 / bam160.1);
    json += &format!(
        "  \"t12_bam_d256_p50_us\": {:.3},\n  \"t12_bam_d256_p95_us\": {:.3},\n  \"t12_bam_d160_p50_us\": {:.3},\n  \"t12_bam_d512_p50_us\": {:.3},\n  \"t12_fhrr_f32_d160_p50_us\": {:.3},\n",
        bam256.1, bam256.2, bam160.1, bam512.1, fh160.1
    );
    let (hw, det) = bench_entropy();
    println!("[entropia] gaussiana: silicio {hw:.1} ns | determinista {det:.1} ns");
    let (nat, interp, jit, jitted) = bench_sindy();
    println!(
        "[sindy] dx=Theta*Xi: nativo {nat:.2} ns | interpretado {interp:.2} ns | {} {jit:.2} ns",
        if jitted { "JIT" } else { "fallback" }
    );
    let mps = bench_spsc();
    println!("[spsc] produtor->consumidor 2 threads: {mps:.1} Mmsg/s");
    json += &format!(
        "  \"entropy_gauss_hw_ns\": {hw:.2},\n  \"entropy_gauss_seed_ns\": {det:.2},\n  \"sindy_native_ns\": {nat:.3},\n  \"sindy_interp_ns\": {interp:.3},\n  \"sindy_jit_ns\": {jit:.3},\n  \"sindy_jit_active\": {jitted},\n  \"spsc_mmsg_s\": {mps:.2}\n}}\n"
    );
    if let Err(e) = fs::create_dir_all("target")
        .and_then(|_| fs::write("target/aether_bench_hardware.json", &json))
    {
        eprintln!("aviso: nao foi possivel gravar o JSON: {e}");
    }
}
