//! FASE 5 — AETHER Hardware Extremus: invariantes e medições.
//!
//! * BAM holográfico (T12-BAM): recall, SNR, latência `< 5 µs`, 0 alocações.
//! * Banho térmico RDTSC: estatística, ausência de alocação, Langevin (OU).
//! * SINDy JIT: equivalência com o interpretador, *hot-swap* sob carga,
//!   zero alocação no avaliador.
//! * Governador: pinning gracioso, filas SPSC, publicação JIT em voo.
//!
//! A latência é o **mínimo de 64 repetições** por consulta (robusto a
//! contenção de CPU de builds concorrentes), como o T12 da auditoria.

#![cfg(all(feature = "m1", feature = "m5"))]
#![allow(clippy::needless_range_loop)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::{Duration, Instant};

use aether_engine::core::aether_governor::{
    EventDrivenCore, Governor, GovernorConfig, ReflexConfig, ReflexEvent, LATENT_DIM,
};
use aether_engine::core::bam_phase::{
    BamHolographicStore, BamHolographicVector, BamMask, BamPhase,
};
use aether_engine::core::fhrr::FhrrStore;
use aether_engine::core::koopman_sindy::{SindyEquationDiscoverer, SINDY_TERMS};
use aether_engine::core::silicon_entropy::{has_hardware_tsc, LangevinBath, SiliconEntropy};
use aether_engine::core::sindy_jit::{jit_compiled_in, PublishOutcome, SindyJit};

/// Allocador contador por thread (só conta enquanto `ARMED`).
struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(|a| a.get()) {
            ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    let out = f();
    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()))
}

// ---------------------------------------------------------------------------
// BAM holográfico (T12-BAM)
// ---------------------------------------------------------------------------

const FACTS: usize = 50;
const QUERIES: usize = 20;
const REPS: usize = 64;

/// Máscara exata de `visible` posições (embaralhamento determinístico).
fn exact_mask(seed: u64, d: usize, visible: usize) -> Vec<bool> {
    let mut rng = SiliconEntropy::from_seed(seed);
    let mut idx: Vec<usize> = (0..d).collect();
    for i in (1..d).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        idx.swap(i, j);
    }
    let mut m = vec![false; d];
    for &i in idx.iter().take(visible) {
        m[i] = true;
    }
    m
}

struct T12Result {
    recall: f64,
    snr: f64,
    latency_us: f64,
    allocs: usize,
}

fn rand_rad(rng: &mut SiliconEntropy, d: usize) -> Vec<f32> {
    (0..d).map(|_| rng.next_f32() * std::f32::consts::TAU).collect()
}

fn snr_of(sims: &[f32], truth: usize) -> f64 {
    let others: Vec<f64> = sims
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != truth)
        .map(|(_, s)| *s as f64)
        .collect();
    let mean = others.iter().sum::<f64>() / others.len() as f64;
    let var = others.iter().map(|s| (s - mean) * (s - mean)).sum::<f64>() / others.len() as f64;
    (sims[truth] as f64 - mean) / var.sqrt().max(1e-6)
}

/// Mesmo cenário do T12 da auditoria (50 fatos, 40 % visíveis, ±0,25 rad) em BAM.
fn run_bam<const D: usize>(noise_on_entity_only: bool) -> T12Result {
    let mut rng = SiliconEntropy::from_seed(0xFAC7);
    let mut store = BamHolographicStore::<D>::new(6).expect("store");
    let mut atoms: Vec<(Vec<f32>, Vec<f32>)> = Vec::new();
    for _ in 0..FACTS {
        let (e, r, v) = (
            rand_rad(&mut rng, D),
            rand_rad(&mut rng, D),
            rand_rad(&mut rng, D),
        );
        store.add_fact(
            &BamHolographicVector::<D>::from_radians(&e).expect("e"),
            &BamHolographicVector::<D>::from_radians(&r).expect("r"),
            &BamHolographicVector::<D>::from_radians(&v).expect("v"),
        );
        atoms.push((e, r));
    }
    let mut sc = store.scratch(FACTS);
    let (mut hits, mut allocs) = (0usize, 0usize);
    let (mut lat, mut snr) = (0.0f64, 0.0f64);
    for q in 0..QUERIES {
        let truth = (rng.next_u64() % FACTS as u64) as usize;
        let mask_b = exact_mask(0x7000 + q as u64, D, D * 40 / 100);
        let (mut ec, mut rc) = atoms[truth].clone();
        for i in 0..D {
            if mask_b[i] {
                let noise = (rng.next_f32() - 0.5) * 0.5;
                ec[i] += noise;
                if !noise_on_entity_only {
                    rc[i] -= noise; // cancela em e⊙r (como o T12 da auditoria)
                }
            }
        }
        let e = BamHolographicVector::<D>::from_radians(&ec).expect("ec");
        let r = BamHolographicVector::<D>::from_radians(&rc).expect("rc");
        let mask = BamMask::<D>::from_bools(&mask_b).expect("mask");
        let mut best = u128::MAX;
        let mut hit = store.query_into(&e, &r, &mask, &mut sc).expect("q");
        for _ in 0..REPS {
            let t = Instant::now();
            hit = store.query_into(&e, &r, &mask, &mut sc).expect("q");
            best = best.min(t.elapsed().as_nanos());
        }
        lat += best as f64 / 1000.0;
        let (_, a) = count_allocs(|| {
            let _ = store.query_into(&e, &r, &mask, &mut sc);
        });
        allocs += a;
        hits += usize::from(hit.value_index == truth);
        snr += snr_of(sc.sims(), truth);
    }
    T12Result {
        recall: hits as f64 / QUERIES as f64,
        snr: snr / QUERIES as f64,
        latency_us: lat / QUERIES as f64,
        allocs,
    }
}

/// O mesmo cenário no `FhrrStore` f32 (D=160) — baseline do T12 atual.
fn run_fhrr_f32<const D: usize>() -> (f64, f64) {
    let mut rng = SiliconEntropy::from_seed(0xFAC7);
    let mut store = FhrrStore::with_traces(D, 6).expect("fhrr");
    let mut atoms = Vec::new();
    for _ in 0..FACTS {
        let (e, r, v) = (
            rand_rad(&mut rng, D),
            rand_rad(&mut rng, D),
            rand_rad(&mut rng, D),
        );
        store.add_fact(&e, &r, &v).expect("add");
        atoms.push((e, r));
    }
    let mut sc = store.scratch(FACTS);
    let (mut hits, mut lat) = (0usize, 0.0f64);
    for q in 0..QUERIES {
        let truth = (rng.next_u64() % FACTS as u64) as usize;
        let mask = exact_mask(0x7000 + q as u64, D, D * 40 / 100);
        let (mut ec, mut rc) = atoms[truth].clone();
        for i in 0..D {
            if mask[i] {
                let n = (rng.next_f32() - 0.5) * 0.5;
                ec[i] += n;
                rc[i] -= n;
            }
        }
        let mut best = u128::MAX;
        let mut hit = store.query_into(&ec, &rc, &mask, &mut sc).expect("q");
        for _ in 0..REPS {
            let t = Instant::now();
            hit = store.query_into(&ec, &rc, &mask, &mut sc).expect("q");
            best = best.min(t.elapsed().as_nanos());
        }
        lat += best as f64 / 1000.0;
        hits += usize::from(hit.value_index == truth);
    }
    (hits as f64 / QUERIES as f64, lat / QUERIES as f64)
}

#[test]
fn t12_bam_d256_abaixo_de_5us_sem_alocar() {
    let r = run_bam::<256>(false);
    println!(
        "[T12-BAM D=256] Recall@1={:.3} SNR={:.2}sigma latencia={:.3} us allocs={}",
        r.recall, r.snr, r.latency_us, r.allocs
    );
    assert!(r.recall >= 0.90, "recall {}", r.recall);
    assert!(r.snr > 3.0, "snr {}", r.snr);
    assert_eq!(r.allocs, 0, "hot-path deve ter 0 alocacoes");
    assert!(r.latency_us < 5.0, "latencia {:.3} us >= 5 us", r.latency_us);
}

#[test]
fn t12_bam_d160_vs_fhrr_f32() {
    let b = run_bam::<160>(false);
    let (f_recall, f_lat) = run_fhrr_f32::<160>();
    println!(
        "[T12-BAM D=160] BAM recall={:.3} snr={:.2} {:.3} us | FHRR f32 recall={:.3} {:.3} us | speedup x{:.2}",
        b.recall,
        b.snr,
        b.latency_us,
        f_recall,
        f_lat,
        f_lat / b.latency_us
    );
    assert!(b.recall + 0.051 >= f_recall, "BAM nao pode perder recall");
    assert_eq!(b.allocs, 0);
    assert!(b.latency_us < 5.0, "latencia {:.3} us", b.latency_us);
}

#[test]
fn t12_bam_robusto_a_ruido_nao_cancelante() {
    // Ruído ±0,25 rad só na entidade (não cancela em e⊙r): cenário mais duro.
    let r = run_bam::<256>(true);
    println!(
        "[T12-BAM ruido duro] Recall@1={:.3} SNR={:.2}",
        r.recall, r.snr
    );
    assert!(r.recall >= 0.90, "recall {}", r.recall);
}

#[test]
fn bam_bind_unbind_similaridade_sem_alocar() {
    let mut rng = SiliconEntropy::from_seed(5);
    let a = BamHolographicVector::<256>::random(&mut rng);
    let b = BamHolographicVector::<256>::random(&mut rng);
    let mask = BamMask::<256>::all();
    let ((s_ab, s_aa), allocs) = count_allocs(|| {
        let c = a.bind(&b);
        let back = c.unbind(&b);
        (c.similarity_masked(&b, &mask), back.similarity(&a))
    });
    assert_eq!(allocs, 0);
    assert!(s_aa > 0.99 && s_ab.abs() < 0.2);
    assert_eq!(BamPhase::from_radians(std::f32::consts::PI), BamPhase::HALF_TURN);
}

// ---------------------------------------------------------------------------
// Banho térmico RDTSC
// ---------------------------------------------------------------------------

#[test]
fn entropia_de_silicio_sem_alocar_e_gaussiana() {
    let mut e = SiliconEntropy::new();
    assert!(e.is_hardware_seeded());
    let mut buf = vec![0.0f32; 8192];
    let (_, allocs) = count_allocs(|| {
        e.fill_gaussian(&mut buf);
        let _ = e.next_u64();
        let _ = e.next_f32_open();
    });
    assert_eq!(allocs, 0);
    let n = buf.len() as f64;
    let mean = buf.iter().map(|v| *v as f64).sum::<f64>() / n;
    let var = buf.iter().map(|v| (*v as f64 - mean).powi(2)).sum::<f64>() / n;
    println!(
        "[silicon] tsc_real={} media={mean:.4} var={var:.4}",
        has_hardware_tsc()
    );
    assert!(mean.abs() < 0.05 && (var - 1.0).abs() < 0.08);
}

#[test]
fn langevin_ornstein_uhlenbeck_variancia_igual_a_temperatura() {
    // dx = -x dt + sqrt(2 T dt) ξ  ⇒  Var(x∞) = T.
    let t = 0.5f32;
    let mut bath = LangevinBath::new(t);
    let mut x = [0.0f32; 64];
    let mut drift = [0.0f32; 64];
    let dt = 0.01f32;
    let ((), allocs) = count_allocs(|| {
        for step in 0..4000 {
            for (d, xi) in drift.iter_mut().zip(&x) {
                *d = -xi;
            }
            bath.step(&mut x, &drift, dt);
            if step == 100 {
                std::hint::black_box(&x);
            }
        }
    });
    assert_eq!(allocs, 0);
    let var = x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64;
    println!("[langevin] Var(x)={var:.3} (alvo T={t})");
    assert!((var - t as f64).abs() < 0.25, "var {var}");
}

// ---------------------------------------------------------------------------
// SINDy JIT
// ---------------------------------------------------------------------------

/// Duffing `ẋ1 = x2; ẋ2 = -0.2 x2 - x1 - x1³` (grau 3, esparso).
fn duffing(x: [f64; 2]) -> [f64; 2] {
    [x[1], -0.2 * x[1] - x[0] - x[0] * x[0] * x[0]]
}

fn rk4_f64(x: [f64; 2], dt: f64) -> [f64; 2] {
    let k1 = duffing(x);
    let k2 = duffing([x[0] + 0.5 * dt * k1[0], x[1] + 0.5 * dt * k1[1]]);
    let k3 = duffing([x[0] + 0.5 * dt * k2[0], x[1] + 0.5 * dt * k2[1]]);
    let k4 = duffing([x[0] + dt * k3[0], x[1] + dt * k3[1]]);
    [
        x[0] + dt / 6.0 * (k1[0] + 2.0 * k2[0] + 2.0 * k3[0] + k4[0]),
        x[1] + dt / 6.0 * (k1[1] + 2.0 * k2[1] + 2.0 * k3[1] + k4[1]),
    ]
}

#[test]
fn sindy_descobre_duffing_e_jit_publica_sem_interromper() {
    let dt = 0.01f64;
    let mut disc = SindyEquationDiscoverer::streaming(dt as f32, 0.05).expect("sindy");
    let mut x = [1.0f64, 0.0];
    for _ in 0..200 {
        disc.push([x[0] as f32, x[1] as f32]);
        x = rk4_f64(x, dt);
    }
    let err = disc.fit_window().expect("fit");
    assert!(err < 0.05, "erro relativo do ajuste {err}");

    let (mut jit, integ) = SindyJit::new();
    let out = jit.publish(&disc).expect("publish");
    let active = disc
        .coefficients()
        .iter()
        .flatten()
        .filter(|v| **v != 0.0)
        .count();
    println!(
        "[sindy-jit] {out:?} termos_ativos={active} jit_compilado={} compile_us={}",
        jit_compiled_in(),
        jit.stats().last_compile_us
    );
    assert!(matches!(
        out,
        PublishOutcome::Jitted { .. } | PublishOutcome::Interpreted { .. }
    ));
    assert!(active <= 6, "Xi deve ser esparsa ({active} termos)");
    assert_eq!(integ.is_jit_active(), jit_compiled_in());

    // Trajetória integrada pelo kernel ≈ verdade (1 unidade de tempo).
    let mut xf = [0.8f32, 0.3];
    let mut xt = [0.8f64, 0.3];
    for _ in 0..100 {
        integ.rk4_step(&mut xf, dt as f32);
        xt = rk4_f64(xt, dt);
    }
    let e = ((xf[0] as f64 - xt[0]).powi(2) + (xf[1] as f64 - xt[1]).powi(2)).sqrt();
    println!("[sindy-jit] erro de trajetoria apos t=1: {e:.5}");
    assert!(e < 0.05, "trajetoria diverge: {e}");

    // Avaliação sem alocação.
    let ((), allocs) = count_allocs(|| {
        let mut s = [0.5f32, 0.1];
        for _ in 0..1000 {
            integ.rk4_step(&mut s, 0.001);
        }
        std::hint::black_box(s);
    });
    assert_eq!(allocs, 0);
}

#[test]
fn jit_hot_swap_sob_avaliacao_continua_nunca_rasga_estado() {
    // Cada geração g tem ẋ1 = c_g e ẋ2 = −c_g (termos constantes). Um
    // avaliador em outra thread nunca pode ver c ≠ −c' (estado rasgado) nem um
    // valor que não tenha sido publicado.
    let (mut jit, integ) = SindyJit::new();
    let gens = 120usize;
    let mut xi = [[0.0f64; SINDY_TERMS]; 2];
    xi[0][0] = 1.0;
    xi[1][0] = -1.0;
    jit.publish_coefficients(&xi).expect("g0");

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_r = std::sync::Arc::clone(&stop);
    let reader = std::thread::spawn(move || {
        let mut evals = 0u64;
        let mut torn = 0u64;
        let mut bad = 0u64;
        let x = [0.3f32, -0.2];
        while !stop_r.load(std::sync::atomic::Ordering::Acquire) {
            let mut dx = [0.0f32; 2];
            integ.eval(&x, &mut dx);
            evals += 1;
            if dx[0] != -dx[1] {
                torn += 1;
            }
            let c = dx[0];
            if !((1.0..=gens as f32 + 1.0).contains(&c) && c.fract() == 0.0) {
                bad += 1;
            }
        }
        (evals, torn, bad)
    });
    for g in 1..=gens {
        xi[0][0] = (g + 1) as f64;
        xi[1][0] = -((g + 1) as f64);
        jit.publish_coefficients(&xi).expect("publish");
        std::thread::sleep(Duration::from_micros(300));
    }
    stop.store(true, std::sync::atomic::Ordering::Release);
    let (evals, torn, bad) = reader.join().expect("leitor");
    let st = jit.stats().clone();
    println!(
        "[jit-swap] evals={evals} torn={torn} bad={bad} geracoes={} compiles={} liberados={}",
        st.generations, st.compiles, st.retired_freed
    );
    assert_eq!(torn, 0);
    assert_eq!(bad, 0);
    assert!(evals > 1000);
    assert_eq!(st.generations as usize, gens + 1);
    if jit_compiled_in() {
        assert_eq!(st.compiles as usize, gens + 1);
        assert_eq!(st.failures, 0);
    }
}

// ---------------------------------------------------------------------------
// Governador (reflexo + secundários)
// ---------------------------------------------------------------------------

fn latent_sample(k: usize, x: [f64; 2]) -> [f32; LATENT_DIM] {
    let t = k as f32 * 0.01;
    [
        0.5 * x[0] as f32,
        0.5 * x[1] as f32,
        0.2 * (t * 1.3).sin(),
        0.2 * (t * 0.7).cos(),
        0.1 * (t * 2.1).sin(),
        0.1 * (t * 0.3).cos(),
    ]
}

#[test]
fn event_driven_core_nao_aloca_e_respeita_o_cbf() {
    let (_jit, integ) = SindyJit::new();
    let mut core = EventDrivenCore::new(
        ReflexConfig::default(),
        integ,
        SiliconEntropy::from_seed(3),
    )
    .expect("core");
    let ev = ReflexEvent {
        t_ns: 0,
        z: latent_sample(10, [1.0, 0.2]),
    };
    for _ in 0..8 {
        core.process(&ev).expect("warmup");
    }
    let ((), allocs) = count_allocs(|| {
        for k in 0..2000 {
            let ev = ReflexEvent {
                t_ns: k,
                z: latent_sample(k as usize, [1.0, 0.2]),
            };
            let out = core.process(&ev).expect("process");
            assert!(!out.violated);
        }
    });
    assert_eq!(allocs, 0, "reflexo deve ter 0 alocacoes");
    assert_eq!(core.violations(), 0);
}

#[test]
fn governador_publica_jit_em_voo_e_nao_viola_lyapunov() {
    let cfg = GovernorConfig {
        entropy_seed: Some(42),
        refit_every: 32,
        sindy_tau: 0.05,
        ..GovernorConfig::default()
    };
    let mut gov = Governor::spawn(cfg).expect("spawn");
    let plan = gov.plan().clone();
    let dt = 0.01f64;
    let mut x = [1.0f64, 0.0];
    let total = 6000usize;
    let t0 = Instant::now();
    for k in 0..total {
        let ev = ReflexEvent {
            t_ns: k as u64 * 10_000_000,
            z: latent_sample(k, x),
        };
        while !gov.submit(ev) {
            std::thread::yield_now();
        }
        x = rk4_f64(x, dt);
        if k % 64 == 0 {
            // Cadência de sensor: dá tempo aos secundários (não bloqueia o reflexo).
            std::thread::sleep(Duration::from_micros(200));
        }
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while gov.processed() < total as u64 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let elapsed = t0.elapsed();
    let rep = gov.shutdown().expect("shutdown");
    println!(
        "[governor] plano={:?} pinned={:?} eventos={} t={:.0}ms koopman={} fits={} publishes={} jit_gen={} jit={} swarm_steps={} drops(k/s)={}/{}",
        rep.plan,
        rep.pinned,
        rep.reflex_events,
        elapsed.as_secs_f64() * 1e3,
        rep.koopman_updates,
        rep.sindy_fits,
        rep.sindy_publishes,
        rep.jit_generations,
        rep.jit_active,
        rep.swarm_steps,
        rep.dropped_koopman,
        rep.dropped_swarm
    );
    assert_eq!(rep.reflex_events, rep.submitted, "reflexo perdeu eventos");
    assert_eq!(rep.reflex_errors, 0);
    assert_eq!(rep.cbf_violations, 0, "CBF violado");
    assert!(rep.koopman_updates > 0, "Koopman nao recebeu pares");
    assert!(rep.sindy_fits > 0, "SINDy nao ajustou");
    assert!(rep.sindy_publishes >= 1 && rep.jit_generations >= 1);
    assert_eq!(rep.jit_active, jit_compiled_in());
    assert!(rep.swarm_steps > 0, "enxame nao avancou");
    // Pinning gracioso: se reportou sucesso, havia núcleo no plano.
    if rep.pinned.0 {
        assert!(plan.reflex.is_some());
    }
    if rep.pinned.1 {
        assert!(plan.koopman.is_some());
    }
}
