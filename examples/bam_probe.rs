//! Fase 0/1 — perfilamento de hardware e protótipo BAM vs FHRR f32.
//!
//! Execução:
//! `cargo run --release --example bam_probe`
//!
//! Mede:
//! 1. T12: breakdown (pista, cleanup do papel, unbind, cleanup de valores),
//!    comparando `FhrrStore` f32 com `T12Bam` u16 (mesmos fatos/pistas);
//! 2. p50/p99/p99.9 do quadro Basal do organismo, sem e com a thread
//!    reflexiva fixada em um núcleo físico, sob carga nos demais núcleos;
//! 3. roofline de banda de memória para futuros backends wgpu
//!    (N = 512, 1024, 4096).
//!
//! O exemplo é um instrumento de medição: não altera o motor.

#![allow(clippy::needless_range_loop)]

use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use aether_engine::core::affinity::{core_ids, core_plan, pin_reflex_core, pin_to_core};
use aether_engine::core::fhrr::FhrrStore;
use aether_engine::core::fhrr_bam::{T12Bam, T12_D, T12_FACTS, T12_TRACES};
use aether_engine::core::gpu_gate::{decide, roofline_table, GpuVerdict, Workload};
use aether_engine::core::koopman_sindy::SindyEquationDiscoverer;
use aether_engine::simulation::closed_loop_organism::{
    ClosedLoopOrganism, OrganismConfig, OrganismFrame,
};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

struct Fact {
    e: Vec<f32>,
    r: Vec<f32>,
    v: Vec<f32>,
}

struct Query {
    e: Vec<f32>,
    r: Vec<f32>,
    mask: Vec<bool>,
    truth: usize,
}

fn make_facts(n: usize, rng: &mut ChaCha8Rng) -> Vec<Fact> {
    (0..n)
        .map(|_| Fact {
            e: (0..T12_D)
                .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
                .collect(),
            r: (0..T12_D)
                .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
                .collect(),
            v: (0..T12_D)
                .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
                .collect(),
        })
        .collect()
}

fn make_queries(facts: &[Fact], n: usize, rng: &mut ChaCha8Rng) -> Vec<Query> {
    (0..n)
        .map(|_| {
            let truth = (rng.gen::<u32>() as usize) % facts.len();
            let mut mask = vec![true; T12_D];
            let hidden = T12_D * 40 / 100;
            for m in mask.iter_mut().take(hidden) {
                *m = false;
            }
            // Embaralha a máscara sem alocar de novo.
            for i in (1..mask.len()).rev() {
                let j = (rng.gen::<u32>() as usize) % (i + 1);
                mask.swap(i, j);
            }
            let mut e = facts[truth].e.clone();
            let mut r = facts[truth].r.clone();
            for i in 0..T12_D {
                if mask[i] {
                    let noise = (rng.gen::<f32>() - 0.5) * 0.5;
                    e[i] += noise;
                    r[i] -= noise;
                } else {
                    e[i] = 0.0;
                    r[i] = 0.0;
                }
            }
            Query { e, r, mask, truth }
        })
        .collect()
}

fn snr_of(sims: &[f32], truth: usize) -> f64 {
    let correct = sims[truth] as f64;
    let mut mean = 0.0f64;
    let mut count = 0usize;
    for (i, s) in sims.iter().enumerate() {
        if i != truth {
            mean += *s as f64;
            count += 1;
        }
    }
    mean /= count.max(1) as f64;
    let mut var = 0.0f64;
    for (i, s) in sims.iter().enumerate() {
        if i != truth {
            let d = *s as f64 - mean;
            var += d * d;
        }
    }
    var /= count.max(1) as f64;
    (correct - mean) / var.sqrt().max(1e-9)
}

fn min_latency_us<F: FnMut()>(mut f: F, reps: usize) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..reps {
        let t = Instant::now();
        f();
        let us = t.elapsed().as_secs_f64() * 1e6;
        if us < best {
            best = us;
        }
    }
    best
}

fn f32_eval(store: &FhrrStore, queries: &[Query]) -> (f64, f64, f64) {
    let mut hits = 0usize;
    let mut snr = 0.0f64;
    let mut lat = 0.0f64;
    for q in queries {
        let m = store.query_match(&q.e, &q.r, &q.mask).expect("f32 query");
        if m.value_index == q.truth {
            hits += 1;
        }
        snr += snr_of(&m.value_similarities, q.truth);
        lat += min_latency_us(
            || {
                black_box(store.query_match(&q.e, &q.r, &q.mask).expect("f32"));
            },
            3,
        );
    }
    (
        hits as f64 / queries.len() as f64,
        snr / queries.len() as f64,
        lat / queries.len() as f64,
    )
}

fn bam_eval(bam: &mut T12Bam, queries: &[Query]) -> (f64, f64, f64) {
    let mut hits = 0usize;
    let mut snr = 0.0f64;
    let mut lat = 0.0f64;
    for q in queries {
        let hit = bam.query_radians(&q.e, &q.r, &q.mask).expect("bam query");
        if hit.value_index == q.truth {
            hits += 1;
        }
        snr += snr_of(bam.sims(), q.truth);
        lat += min_latency_us(
            || {
                black_box(bam.query_radians(&q.e, &q.r, &q.mask).expect("bam"));
            },
            3,
        );
    }
    (
        hits as f64 / queries.len() as f64,
        snr / queries.len() as f64,
        lat / queries.len() as f64,
    )
}

fn t12_comparison() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xFAC7);
    let facts = make_facts(T12_FACTS, &mut rng);
    let queries = make_queries(&facts, 20, &mut rng);

    let mut f32_store = FhrrStore::with_traces(T12_D, T12_TRACES).expect("f32 store");
    for f in &facts {
        f32_store.add_fact(&f.e, &f.r, &f.v).expect("add f32");
    }
    let mut bam = T12Bam::new(T12_TRACES, T12_FACTS + 8).expect("bam");
    for f in &facts {
        bam.add_fact_radians(&f.e, &f.r, &f.v).expect("add bam");
    }

    let (r32, s32, l32) = f32_eval(&f32_store, &queries);
    let (rb, sb, lb) = bam_eval(&mut bam, &queries);
    let speedup = l32 / lb.max(1e-9);
    let decision = if rb == 1.0 && speedup >= 1.5 {
        "READY_FOR_INTEGRATION"
    } else {
        "ISOLATED_EXPERIMENT"
    };

    println!("\n=== T12: FHRR f32 vs BAM u16 (D={T12_D}, K={T12_TRACES}, {T12_FACTS} fatos) ===");
    println!(
        "{:<12} {:>8} {:>8} {:>12} {:>12} {:>10}",
        "STORAGE", "Recall", "SNR", "lat us", "mem bytes", "status"
    );
    println!(
        "{:<12} {:>8.3} {:>8.2} {:>12.3} {:>12} {:>10}",
        "f32",
        r32,
        s32,
        l32,
        f32_store.memory_bytes(),
        "-"
    );
    println!(
        "{:<12} {:>8.3} {:>8.2} {:>12.3} {:>12} {:>10}",
        "BAM u16",
        rb,
        sb,
        lb,
        bam.memory_bytes(),
        decision
    );
    println!("speedup BAM/f32 na consulta completa = {speedup:.2}x");

    breakdown(&facts, &queries);
    affinity_section(bam.memory_bytes());
}

fn breakdown(facts: &[Fact], queries: &[Query]) {
    const D: usize = T12_D;
    const K: usize = T12_TRACES;
    const P: usize = T12_FACTS;
    let mut roles_re = vec![0.0f32; P * D];
    let mut roles_im = vec![0.0f32; P * D];
    let mut vals_re = vec![0.0f32; P * D];
    let mut vals_im = vec![0.0f32; P * D];
    let mut trace_re = vec![0.0f32; K * D];
    let mut trace_im = vec![0.0f32; K * D];
    for (p, f) in facts.iter().enumerate() {
        for j in 0..D {
            let (se, ce) = f.e[j].sin_cos();
            let (sr, cr) = f.r[j].sin_cos();
            roles_re[p * D + j] = ce * cr - se * sr;
            roles_im[p * D + j] = se * cr + ce * sr;
            let (sv, cv) = f.v[j].sin_cos();
            vals_re[p * D + j] = cv;
            vals_im[p * D + j] = sv;
            let th = f.e[j] + f.r[j] + f.v[j];
            let (s, c) = th.sin_cos();
            for t in 0..K {
                trace_re[t * D + j] += c;
                trace_im[t * D + j] += s;
            }
        }
    }

    let mut cue_ns = 0u128;
    let mut role_ns = 0u128;
    let mut unbind_ns = 0u128;
    let mut val_ns = 0u128;
    let reps = 200usize;
    let mut cue_re = [0.0f32; D];
    let mut cue_im = [0.0f32; D];
    let mut z_re = [0.0f32; D];
    let mut z_im = [0.0f32; D];
    let mut best_role = 0usize;
    let mut sims = vec![0.0f32; P];
    for q in queries {
        let visible: Vec<usize> = (0..D).filter(|j| q.mask[*j]).collect();
        for _ in 0..reps {
            let t = Instant::now();
            for j in 0..D {
                let (s, c) = q.e[j].sin_cos();
                let (s2, c2) = q.r[j].sin_cos();
                cue_re[j] = c * c2 - s * s2;
                cue_im[j] = s * c2 + c * s2;
            }
            cue_ns += t.elapsed().as_nanos();

            let t = Instant::now();
            let mut best = f32::NEG_INFINITY;
            for p in 0..P {
                let mut acc = 0.0f32;
                for &j in &visible {
                    acc += cue_re[j] * roles_re[p * D + j] + cue_im[j] * roles_im[p * D + j];
                }
                if acc > best {
                    best = acc;
                    best_role = p;
                }
            }
            role_ns += t.elapsed().as_nanos();
            black_box(best);

            let t = Instant::now();
            z_re.fill(0.0);
            z_im.fill(0.0);
            for tix in 0..K {
                for j in 0..D {
                    let rr = roles_re[best_role * D + j];
                    let ri = roles_im[best_role * D + j];
                    let tr = trace_re[tix * D + j];
                    let ti = trace_im[tix * D + j];
                    z_re[j] += tr * rr + ti * ri;
                    z_im[j] += ti * rr - tr * ri;
                }
            }
            unbind_ns += t.elapsed().as_nanos();

            let t = Instant::now();
            for p in 0..P {
                let mut acc = 0.0f32;
                for j in 0..D {
                    acc += z_re[j] * vals_re[p * D + j] + z_im[j] * vals_im[p * D + j];
                }
                sims[p] = acc;
            }
            val_ns += t.elapsed().as_nanos();
            black_box(&sims);
        }
    }
    let n = (reps * queries.len()) as f64;
    let cue = cue_ns as f64 / n / 1000.0;
    let role = role_ns as f64 / n / 1000.0;
    let unbind = unbind_ns as f64 / n / 1000.0;
    let val = val_ns as f64 / n / 1000.0;
    println!("\n=== T12 breakdown (cópia de perfil f32, D={D}, K={K}, P={P}) ===");
    println!("pista (2x sin_cos + role cue): {cue:8.3} us");
    println!("cleanup do papel (P x visiveis): {role:8.3} us");
    println!("unbind (K x D):                 {unbind:8.3} us");
    println!("cleanup de valores (P x D):     {val:8.3} us");
    println!(
        "soma dos estagios:              {:8.3} us",
        cue + role + unbind + val
    );
}

fn pct(sorted: &[u64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)] as f64 / 1000.0
}

fn measure_organism(seconds: f64, pin: Option<usize>) -> Vec<u64> {
    if let Some(core) = pin {
        let _ = pin_to_core(core);
    }
    let cfg = OrganismConfig {
        steps: 60,
        perturb_after: Duration::from_secs(3600),
        ..Default::default()
    };
    let mut org = ClosedLoopOrganism::new(cfg).expect("organismo");
    let mut frame = OrganismFrame::with_capacity(60, 64);
    for _ in 0..20 {
        org.step_into(&mut frame).expect("warmup");
    }
    let mut lat = Vec::new();
    let t0 = Instant::now();
    while t0.elapsed().as_secs_f64() < seconds {
        let t = Instant::now();
        org.step_into(&mut frame).expect("step");
        lat.push(t.elapsed().as_nanos() as u64);
    }
    org.shutdown();
    lat
}

fn start_load(
    ids: &[usize],
    exclude: &[usize],
    stop: &Arc<AtomicBool>,
) -> (Vec<thread::JoinHandle<()>>, Vec<usize>) {
    let mut handles = Vec::new();
    let mut used = Vec::new();
    for id in ids {
        if exclude.contains(id) {
            continue;
        }
        let stop = stop.clone();
        let id = *id;
        used.push(id);
        handles.push(thread::spawn(move || {
            let _ = pin_to_core(id);
            while !stop.load(Ordering::Relaxed) {
                black_box((0u64..4096).sum::<u64>());
            }
        }));
    }
    (handles, used)
}

fn affinity_section(active_bytes: usize) {
    let ids = core_ids();
    if ids.is_empty() {
        println!("\n=== Afinidade indisponivel (feature affinity desligada) ===");
        return;
    }
    let plan = core_plan();
    // Núcleo físico dedicado: prefere o último id físico (evita o core 0,
    // que no Windows concentra DPC/interrupções).
    let reflex = ids
        .get(plan.physical.saturating_sub(1))
        .copied()
        .unwrap_or(ids[0]);

    // Baseline: carga em TODOS os núcleos (inclusive o reflexivo), thread solta.
    let stop = Arc::new(AtomicBool::new(false));
    let (load_all, used_all) = start_load(&ids, &[], &stop);
    let base = thread::spawn(move || measure_organism(2.5, None))
        .join()
        .expect("baseline");
    stop.store(true, Ordering::Relaxed);
    for h in load_all {
        let _ = h.join();
    }

    // Fixado: o núcleo reflexivo fica dedicado; a carga vai para os demais.
    let stop = Arc::new(AtomicBool::new(false));
    let (load_ded, used_ded) = start_load(&ids, &[reflex], &stop);
    let pinned = thread::spawn(move || measure_organism(2.5, Some(reflex)))
        .join()
        .expect("pinned");
    stop.store(true, Ordering::Relaxed);
    for h in load_ded {
        let _ = h.join();
    }

    let mut b = base;
    let mut p = pinned;
    b.sort_unstable();
    p.sort_unstable();
    println!(
        "\n=== Basal: afinidade (reflex core {reflex}, carga solta {} nos {} nucleos) ===",
        used_all.len(),
        ids.len()
    );
    println!(
        "{:<10} {:>10} {:>10} {:>10} {:>8}",
        "MODO", "p50 us", "p99 us", "p99.9 us", "n"
    );
    println!(
        "{:<10} {:>10.2} {:>10.2} {:>10.2} {:>8}",
        "solto",
        pct(&b, 0.50),
        pct(&b, 0.99),
        pct(&b, 0.999),
        b.len()
    );
    println!(
        "{:<10} {:>10.2} {:>10.2} {:>10.2} {:>8}",
        "fixado",
        pct(&p, 0.50),
        pct(&p, 0.99),
        pct(&p, 0.999),
        p.len()
    );
    let imp = 100.0 * (1.0 - pct(&p, 0.999) / pct(&b, 0.999).max(1e-9));
    println!(
        "reducao p99.9 = {imp:.1}% (carga dedicada em {} nucleos, reflex isolado)",
        used_ded.len()
    );

    let rep = pin_reflex_core(active_bytes);
    println!(
        "L1d: {} bytes de estado ativo, cabe={} (32 KB), logical={}, physical={}, siblings={:?}",
        rep.l1d_bytes, rep.l1d_fits, rep.logical, rep.physical, rep.reserved_siblings
    );
    if !rep.l1d_fits {
        println!("(o store T12 D=160/K=6 excede L1d; fica em L2 — tradeoff reportado)");
    }
}

fn roofline() {
    println!("\n=== Roofline / portao wgpu (80 passos, alvo 200 us) ===");
    println!(
        "{:>6} {:>12} {:>10} {:>12} {:>12} {:>16}",
        "N", "edges", "MB", "ms CPU", "ms GPU", "veredito"
    );
    for row in roofline_table(80) {
        let verdict = match decide(Workload::new(row.n, 80, 200.0)) {
            GpuVerdict::BelowMinN => "below_min_N",
            GpuVerdict::CpuFaster => "cpu_faster",
            GpuVerdict::TooSlowForTarget { .. } => "too_slow",
            GpuVerdict::WorthIt { .. } => "worth_it",
        };
        println!(
            "{:>6} {:>12} {:>10.1} {:>12.2} {:>12.2} {:>16}",
            row.n, row.edges, row.mb, row.cpu_ms, row.gpu_ms, verdict
        );
    }
    match decide(Workload::new(512, 200_000, 3e6)) {
        GpuVerdict::WorthIt { speedup, gpu_us } => println!(
            "streaming N=512 x 200k passos: worth_it ({speedup:.2}x, {:.1} ms GPU)",
            gpu_us / 1e3
        ),
        other => println!("streaming N=512 x 200k passos: {other:?}"),
    }
    println!("(backend wgpu segue fechado para o laco reflexivo; ver src/core/gpu_gate.rs)");
}

fn sindy_section() {
    let mut series = Vec::with_capacity(4000);
    let (mut x1, mut x2) = (1.0f64, 0.5f64);
    let dt = 0.0005f32;
    for _ in 0..4000 {
        series.push([x1 as f32, x2 as f32]);
        let d1 = -0.5 * x1 + 1.2 * x2 * x2;
        let d2 = 0.8 * x1 - 0.3 * x2 * x2 * x2;
        x1 += d1 * dt as f64;
        x2 += d2 * dt as f64;
    }
    let mut d = SindyEquationDiscoverer::new();
    d.discover(&series, dt, 0.02).expect("sindy");
    let x = [0.37f64, -0.21f64];
    let full = d.rhs(x);
    let act = d.rhs_active(x);
    assert!((full[0] - act[0]).abs() < 1e-12);
    assert!((full[1] - act[1]).abs() < 1e-12);

    let n = 2_000_000u64;
    let t = Instant::now();
    let mut acc = 0.0f64;
    for _ in 0..n {
        let y = d.rhs(black_box(x));
        acc += y[0];
    }
    black_box(acc);
    let full_ns = t.elapsed().as_nanos() as f64 / n as f64;

    let t = Instant::now();
    let mut acc = 0.0f64;
    for _ in 0..n {
        let y = d.rhs_active(black_box(x));
        acc += y[0];
    }
    black_box(acc);
    let act_ns = t.elapsed().as_nanos() as f64 / n as f64;

    println!("\n=== SINDy: avaliador completo vs especializado ===");
    println!("rhs (10 termos): {full_ns:.2} ns/avaliacao");
    println!("rhs_active (termos ativos): {act_ns:.2} ns/avaliacao");
    println!("speedup especializado: {:.2}x", full_ns / act_ns.max(1e-9));
    println!("(JIT medido na Fase 5: 5.24 ns; nativo fixo: 0.82 ns)");
}

fn main() {
    println!("AETHER — Fase 0/1 hardware probe");
    t12_comparison();
    sindy_section();
    roofline();
}
