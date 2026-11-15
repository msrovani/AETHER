//! Benchmark da FASE 5 (hardware extremus): BAM, entropia, JIT e SPSC.
//!
//! Mede, sem descarte de amostras (percentis reais, protocolo declarado):
//!
//! 1. micro-operações BAM (D=256): `sin_cos` f32 vs LUT, bind, similaridade;
//! 2. **A** — SINDy `dx = Θ(x)·Ξ`: nativo vs. interpretado vs. JIT (ns/avaliação)
//!    com 4 termos ativos, custo de compilação e `break_even_evals`;
//! 3. **B** — cauda `p50/p99/p99.9` do ciclo reflexivo (`EventDrivenCore`)
//!    com e sem pinning, sob carga nos demais núcleos lógicos;
//! 4. **C** — T12 (RAG holográfico): 50 fatos, K=6, 40 % visível, ±0,25 rad,
//!    p50/p95/p99 por consulta em D = 160 / 256 / 512, contra `FhrrStore` f32
//!    em D = 160 e D = 256;
//! 5. entropia de silício (ns/gaussiana) vs. semente determinística;
//! 6. SPSC: vazão produtor→consumidor entre duas threads (Mmsg/s).
//!
//! `harness = false` (laço único com `Instant`, mesma convenção de
//! `throughput_tps`). Saída: tabela no terminal e
//! `target/aether_bench_hardware.json`.
//!
//! Execução: `cargo bench --bench hardware_extremus`
//! (adicione `--features jit,affinity` para as linhas de JIT e pinning).

use std::fs;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use aether_engine::core::affinity::{core_ids, pin_reflex_core, pin_to_core};
use aether_engine::core::aether_governor::{
    EventDrivenCore, ReflexConfig, ReflexEvent, LATENT_DIM,
};
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

/// Resultado do benchmark A (SINDy RHS + break-even do JIT).
struct SindyBench {
    native_ns: f64,
    interp_ns: f64,
    jit_ns: f64,
    jitted: bool,
    /// Custo da última compilação Cranelift (µs); 0 se a feature `jit` está off.
    compile_us: f64,
    /// Custo da **primeira** compilação (cold start, inclui setup da ISA).
    first_compile_us: f64,
    /// Termos ativos da equação de teste.
    active_terms: usize,
    /// Avaliações para amortizar a primeira compilação; `NaN` se o JIT não ganha.
    break_even_evals: f64,
    /// Avaliações para amortizar uma recompilação em regime.
    steady_break_even_evals: f64,
}

/// Benchmark A: `dx = Θ(x)·Ξ` com 4 termos ativos
/// (`x' = y`; `y' = x − x³ − 0,3y`), interpretado vs. JIT, e break-even.
fn bench_sindy() -> SindyBench {
    let mut xi = [[0.0f64; 10]; 2];
    xi[0][2] = 1.0; // x' = y
    xi[1][1] = 1.0; // y' = x − x³ − 0,3 y
    xi[1][6] = -1.0;
    xi[1][2] = -0.3;
    let active_terms = xi.iter().flatten().filter(|v| **v != 0.0).count();
    let (mut jit, integ) = SindyJit::new();
    let out = jit.publish_coefficients(&xi).expect("publish");
    let first_compile_us = jit.stats().last_compile_us as f64;
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
    let jit_ns = ns_per_iter(2_000_000, || {
        integ.eval(black_box(&x), &mut dx);
        black_box(&dx);
    });

    // Custo de compilação: republica com Ξ levemente diferente para forçar
    // recompilação (publicação idêntica é ignorada por design).
    let mut samples: Vec<f64> = Vec::new();
    let mut varied = xi;
    for i in 0..5 {
        varied[1][2] = -0.3 - 1e-9 * (i as f64 + 1.0);
        if jit.publish_coefficients(&varied).is_err() {
            break;
        }
        let st = jit.stats();
        if st.compiles > 0 {
            samples.push(st.last_compile_us as f64);
        }
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let compile_us = samples.get(samples.len() / 2).copied().unwrap_or(0.0);
    let jitted = matches!(
        out,
        aether_engine::core::sindy_jit::PublishOutcome::Jitted { .. }
    );

    let gain_ns = interp - jit_ns;
    let break_even_evals = if jitted && gain_ns > 0.0 && compile_us > 0.0 {
        compile_us * 1000.0 / gain_ns
    } else {
        f64::NAN
    };

    SindyBench {
        native_ns: native,
        interp_ns: interp,
        jit_ns,
        jitted,
        compile_us,
        active_terms,
        break_even_evals,
    }
}

/// Amostra latente sintética do reflexo (mesma escala do teste do governador).
fn latent_z(k: usize) -> [f32; LATENT_DIM] {
    let t = k as f32 * 0.01;
    let mut z = [0.0f32; LATENT_DIM];
    if LATENT_DIM >= 6 {
        z[0] = 0.5;
        z[1] = 0.2;
        z[2] = 0.2 * (t * 1.3).sin();
        z[3] = 0.2 * (t * 0.7).cos();
        z[4] = 0.1 * (t * 2.1).sin();
        z[5] = 0.1 * (t * 0.3).cos();
    }
    z
}

/// Benchmark B: cauda do ciclo reflexivo sob carga, com e sem pinning.
///
/// Devolve `(p50, p99, p99.9, n, pinned)` em µs.
fn reflex_tail(pin_reflex: bool, load_ids: &[usize], events: usize) -> (f64, f64, f64, usize, bool) {
    let stop = Arc::new(AtomicBool::new(false));
    let mut load = Vec::with_capacity(load_ids.len());
    for &id in load_ids {
        let stop = Arc::clone(&stop);
        load.push(std::thread::spawn(move || {
            let _ = pin_to_core(id);
            let mut x = (id as u64) | 1;
            while !stop.load(Ordering::Relaxed) {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                black_box(x);
            }
        }));
    }

    let pinned = if pin_reflex {
        pin_reflex_core(0).pinned
    } else {
        false
    };

    let (_jit, integ) = SindyJit::new();
    let mut core = EventDrivenCore::new(
        ReflexConfig::default(),
        integ,
        SiliconEntropy::from_seed(3),
    )
    .expect("core reflexivo");
    for k in 0..64 {
        let ev = ReflexEvent {
            t_ns: k as u64,
            z: latent_z(k),
        };
        core.process(&ev).expect("warmup");
    }

    let mut lat = Vec::with_capacity(events);
    for k in 0..events {
        let ev = ReflexEvent {
            t_ns: k as u64 * 1_000_000,
            z: latent_z(k),
        };
        let t = Instant::now();
        core.process(&ev).expect("process");
        lat.push(t.elapsed().as_nanos() as f64 / 1000.0);
    }

    stop.store(true, Ordering::Relaxed);
    for h in load {
        let _ = h.join();
    }
    lat.sort_by(|a, b| a.total_cmp(b));
    (
        pct(&lat, 0.50),
        pct(&lat, 0.99),
        pct(&lat, 0.999),
        lat.len(),
        pinned,
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
    println!(
        "[env] jit_compilado={} | feature affinity={}",
        jit_compiled_in(),
        cfg!(feature = "affinity")
    );

    // ---- C. BAM u16 vs FHRR f32 (T12) -----------------------------------
    println!("\n== C. BAM u16 vs FHRR f32 (T12) ==");
    let micro = bench_bam_micro();
    for (n, v) in micro {
        println!("[bam] {n:<18} {v:>10.1} ns");
    }
    println!("[bam] ganho sin_cos LUT x{:.0}", micro[0].1 / micro[1].1);
    let mut json = String::from("{\n");
    for (n, v) in micro {
        json += &format!("  \"bam_{n}_ns\": {v:.2},\n");
    }
    let (bam160, bam256, bam512, fh160, fh256) = (
        t12::<160>(true),
        t12::<256>(true),
        t12::<512>(true),
        t12::<160>(false),
        t12::<256>(false),
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
        ("FHRR f32 D=256", fh256),
    ] {
        println!(
            "[T12] {n:<15} recall={:.3} p50={:.2} us p95={:.2} us p99={:.2} us",
            r.0, r.1, r.2, r.3
        );
    }
    println!(
        "[T12] speedup p50: D=160 x{:.2} | D=256 x{:.2}",
        fh160.1 / bam160.1,
        fh256.1 / bam256.1
    );
    json += &format!(
        "  \"t12_bam_d160_p50_us\": {:.3},\n  \"t12_bam_d256_p50_us\": {:.3},\n  \"t12_bam_d512_p50_us\": {:.3},\n  \"t12_fhrr_f32_d160_p50_us\": {:.3},\n  \"t12_fhrr_f32_d256_p50_us\": {:.3},\n",
        bam160.1, bam256.1, bam512.1, fh160.1, fh256.1
    );

    // ---- A. SINDy RHS + break-even do JIT --------------------------------
    println!("\n== A. SINDy RHS & break-even (4 termos ativos) ==");
    let s = bench_sindy();
    println!(
        "[sindy] nativo {:.2} ns | interpretado {:.2} ns | {} {:.2} ns (termos ativos={})",
        s.native_ns,
        s.interp_ns,
        if s.jitted { "JIT" } else { "fallback" },
        s.jit_ns,
        s.active_terms
    );
    if s.jitted {
        println!(
            "[sindy] compilacao {:.1} us | ganho {:.2} ns/avaliacao | break_even = {:.0} avaliacoes ({:.1} ms)",
            s.compile_us,
            s.interp_ns - s.jit_ns,
            s.break_even_evals,
            s.break_even_evals * s.jit_ns / 1e6
        );
    } else {
        println!("[sindy] feature `jit` desligada: fallback interpretado, break_even = n/a");
    }
    json += &format!(
        "  \"sindy_native_ns\": {:.3},\n  \"sindy_interp_ns\": {:.3},\n  \"sindy_jit_ns\": {:.3},\n  \"sindy_jit_active\": {},\n  \"sindy_active_terms\": {},\n  \"sindy_compile_us\": {:.1},\n  \"sindy_break_even_evals\": {:.0},\n",
        s.native_ns,
        s.interp_ns,
        s.jit_ns,
        s.jitted,
        s.active_terms,
        s.compile_us,
        if s.break_even_evals.is_finite() { s.break_even_evals } else { -1.0 }
    );

    let (hw, det) = bench_entropy();
    println!("[entropia] gaussiana: silicio {hw:.1} ns | determinista {det:.1} ns");
    let mps = bench_spsc();
    println!("[spsc] produtor->consumidor 2 threads: {mps:.1} Mmsg/s");
    json += &format!(
        "  \"entropy_gauss_hw_ns\": {hw:.2},\n  \"entropy_gauss_seed_ns\": {det:.2},\n  \"spsc_mmsg_s\": {mps:.2},\n"
    );

    // ---- B. Cauda do ciclo reflexivo (pinning) --------------------------
    println!("\n== B. Cauda do ciclo reflexivo (pinning) ==");
    let ids = core_ids();
    let all_load: Vec<usize> = if ids.is_empty() {
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        (0..n).collect()
    } else {
        ids.clone()
    };
    let reflex_core = aether_engine::core::affinity::report_only(0).core;
    let pinned_load: Vec<usize> = match reflex_core {
        Some(c) => all_load.iter().copied().filter(|id| *id != c).collect(),
        None => all_load.clone(),
    };
    let events = 120_000usize;
    let (s50, s99, s999, sn, _) = reflex_tail(false, &all_load, events);
    let (p50, p99, p999, pn, pin_ok) = reflex_tail(true, &pinned_load, events);
    println!(
        "[reflexo] feature affinity={} | carga solta em {} nucleos | carga dedicada em {} | pin_ok={}",
        cfg!(feature = "affinity"),
        all_load.len(),
        pinned_load.len(),
        pin_ok
    );
    println!(
        "{:<8} {:>10} {:>10} {:>10} {:>8}",
        "MODO", "p50 us", "p99 us", "p99.9 us", "n"
    );
    println!(
        "{:<8} {:>10.2} {:>10.2} {:>10.2} {:>8}",
        "solto", s50, s99, s999, sn
    );
    println!(
        "{:<8} {:>10.2} {:>10.2} {:>10.2} {:>8}",
        "fixado", p50, p99, p999, pn
    );
    let reduction = 100.0 * (1.0 - p999 / s999.max(1e-9));
    println!("[reflexo] reducao p99.9 = {reduction:.1}%");
    json += &format!(
        "  \"reflex_p50_solto_us\": {s50:.3},\n  \"reflex_p99_solto_us\": {s99:.3},\n  \"reflex_p999_solto_us\": {s999:.3},\n  \"reflex_p50_fixado_us\": {p50:.3},\n  \"reflex_p99_fixado_us\": {p99:.3},\n  \"reflex_p999_fixado_us\": {p999:.3},\n  \"reflex_p999_reduction_pct\": {reduction:.1},\n  \"affinity_feature\": {},\n  \"pin_ok\": {pin_ok}\n}}\n",
        cfg!(feature = "affinity")
    );

    if let Err(e) = fs::create_dir_all("target")
        .and_then(|_| fs::write("target/aether_bench_hardware.json", &json))
    {
        eprintln!("aviso: nao foi possivel gravar o JSON: {e}");
    }
}
