//! E1 — gates do *Phase-Language Head* (PLH) sharded + ressonador ACF.
//!
//! Cinco gates, todos em `D=160`, `K=6`, 24 shards `≤ 48` fatos:
//!
//! 1. **Escala** — 1.000 fatos sharded com `recall@1 ≥ 0,99`; o controle de
//!    superposição plana colapsa (`< 0,10`).
//! 2. **Não-comutatividade** — `A→B` e `B→A` decodificam 100% e códigos
//!    distintos.
//! 3. **Qualidade e profundidade** — SNR `> 5σ` no unbinding e erro `< 5%`
//!    após 4 passos encadeados de `unbind → cleanup_ACF → re_encode`; o torque
//!    top-down da RG (`64 → 16 → 4`) reduz o erro micro.
//! 4. **Abstenção calibrada** — `> 95%` fora do vocabulário, sob ruído
//!    `σ=1,6` e em profundidade excedida; falso-abster limpo `< 5%`.
//! 5. **Zero alocação e latência** — `0` alocações no caminho quente e
//!    `p50 < 20 µs`.
//!
//! Os números completos do probe ficam em `target/aether_e1_plh.json`; este
//! teste é a versão compacta e determinística para CI.

#![deny(unsafe_code)]

use std::f32::consts::TAU;
use std::hint::black_box;
use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::bam_phase::BamHolographicVector;
use aether_engine::core::phase_language::*;

const SEED: u64 = 0xE1_2026;

// ---------------------------------------------------------------------------
// Instrumentação de alocação (única `unsafe` do arquivo, e é do medidor)
// ---------------------------------------------------------------------------

#[allow(unsafe_code)]
mod alloc_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static ALLOCS: Cell<usize> = const { Cell::new(0) };
    }

    struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if ARMED.with(Cell::get) {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if ARMED.with(Cell::get) {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if ARMED.with(Cell::get) {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
            unsafe { System.realloc(ptr, layout, new_size) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    pub fn arm() {
        ALLOCS.with(|c| c.set(0));
        ARMED.with(|a| a.set(true));
    }

    pub fn disarm() -> usize {
        ARMED.with(|a| a.set(false));
        ALLOCS.with(Cell::get)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build(seed: u64) -> (Codebooks, Anchors, Vec<Fact>, PlhMemory) {
    let cb = Codebooks::build(seed);
    let a = Anchors::build(seed);
    let facts = build_world(N_FACTS);
    let mem = build_sharded(&facts, &cb, &a, K_TRACES, seed);
    (cb, a, facts, mem)
}

fn recall(mem: &PlhMemory, facts: &[Fact], cb: &Codebooks, a: &Anchors, n: usize) -> f64 {
    let mut sc = QueryScratch::new();
    let mut hits = 0usize;
    for k in 0..n {
        let pi = (k * facts.len()) / n.max(1);
        let f = &facts[pi];
        if let Some(p) = hot_query(mem, cb, a, f, &mut sc) {
            if p.obj == f.ids[2] as usize && p.modu == f.ids[3] as usize {
                hits += 1;
            }
        }
    }
    hits as f64 / n as f64
}

fn flat_recall(flat: &Shard, facts: &[Fact], cb: &Codebooks, a: &Anchors, n: usize) -> f64 {
    let mut sc = QueryScratch::new();
    let mut hits = 0usize;
    for k in 0..n {
        let pi = (k * facts.len()) / n.max(1);
        let f = &facts[pi];
        if let Some(p) = hot_query_flat(flat, cb, a, f, &mut sc) {
            if p.obj == f.ids[2] as usize && p.modu == f.ids[3] as usize {
                hits += 1;
            }
        }
    }
    hits as f64 / n as f64
}

fn bundle_sim(x: &Bundle, y: &Bundle) -> f64 {
    let mut dot = 0.0f64;
    let mut nx = 0.0f64;
    let mut ny = 0.0f64;
    for j in 0..D {
        dot += (x.re[j] * y.re[j] + x.im[j] * y.im[j]) as f64;
        nx += (x.re[j] * x.re[j] + x.im[j] * x.im[j]) as f64;
        ny += (y.re[j] * y.re[j] + y.im[j] * y.im[j]) as f64;
    }
    dot / (nx.sqrt() * ny.sqrt()).max(1e-12)
}

fn random_atom(rng: &mut ChaCha8Rng) -> BamHolographicVector<D> {
    let p: [f32; D] = std::array::from_fn(|_| rng.gen::<f32>() * TAU);
    BamHolographicVector::from_radians(&p).expect("D")
}

fn snr(signal: &[f32], noise: &[f32]) -> f64 {
    if signal.is_empty() || noise.is_empty() {
        return f64::NAN;
    }
    let ms = signal.iter().map(|&x| x as f64).sum::<f64>() / signal.len() as f64;
    let mn = noise.iter().map(|&x| x as f64).sum::<f64>() / noise.len() as f64;
    let var = noise
        .iter()
        .map(|&x| {
            let d = x as f64 - mn;
            d * d
        })
        .sum::<f64>()
        / noise.len() as f64;
    let sd = var.sqrt();
    if sd <= 1e-12 {
        f64::INFINITY
    } else {
        (ms - mn) / sd
    }
}

// ---------------------------------------------------------------------------
// Gate 1 — escala sharded vs. plana
// ---------------------------------------------------------------------------

#[test]
fn g1_sharding_holds_1000_facts_and_flat_collapses() {
    let (cb, a, facts, mem) = build(SEED);
    assert!(
        mem.max_load() <= SHARD_CAP,
        "shard com {} fatos > {SHARD_CAP}",
        mem.max_load()
    );
    let r = recall(&mem, &facts, &cb, &a, 200);
    assert!(r >= 0.99, "recall sharded {r} < 0.99");

    let flat = build_flat(&facts, &cb, &a, K_TRACES);
    let fr = flat_recall(&flat, &facts, &cb, &a, 80);
    assert!(fr < 0.10, "superposição plana deveria colapsar, veio {fr}");
    println!(
        "E1.G1 recall sharded={r:.4} plano={fr:.4} max_load={} cap={SHARD_CAP}",
        mem.max_load()
    );
}

// ---------------------------------------------------------------------------
// Gate 2 — não-comutatividade estrita
// ---------------------------------------------------------------------------

#[test]
fn g2_order_matters_a_to_b_vs_b_to_a() {
    let cb = Codebooks::build(SEED ^ 2);
    let a = Anchors::build(SEED ^ 2);
    let mut rng = ChaCha8Rng::seed_from_u64(SEED ^ 0xC0_0042);
    let pairs = 60usize;
    let mut separated = 0usize;
    let mut sim_acc = 0.0f64;

    for k in 0..pairs {
        let x = k % N_ENT;
        let mut y = rng.gen_range(0..N_ENT);
        if y == x {
            y = (y + 1) % N_ENT;
        }
        let v = rng.gen_range(0..N_VERB);
        let m = rng.gen_range(0..N_MOD);

        let fwd = encode_sentence(&cb, &a, [x, v, y, m]);
        let rev = encode_sentence(&cb, &a, [y, v, x, m]);
        let sim = bundle_sim(&fwd, &rev);
        sim_acc += sim;

        let df = factorize_sentence(&fwd, &cb, &a);
        let dr = factorize_sentence(&rev, &cb, &a);
        let fwd_ok = df.ids == [x, v, y, m];
        let rev_ok = dr.ids == [y, v, x, m];
        if fwd_ok && rev_ok && sim < 0.75 {
            separated += 1;
        }
    }
    let sep = separated as f64 / pairs as f64;
    assert_eq!(separated, pairs, "separação {sep:.3} < 1.0");
    assert!(
        sim_acc / (pairs as f64) < 0.75,
        "códigos deveriam ser distintos"
    );
    println!(
        "E1.G2 separacao={sep:.3} sim_media={:.3}",
        sim_acc / pairs as f64
    );
}

// ---------------------------------------------------------------------------
// Gate 3 — SNR, profundidade ACF e torque RG
// ---------------------------------------------------------------------------

#[test]
fn g3_snr_depth_and_rg_topdown() {
    let (cb, a, facts, mem) = build(SEED ^ 3);
    let mut sc = QueryScratch::new();
    let n = 120usize;
    let mut sig_obj = Vec::with_capacity(n);
    let mut sig_mod = Vec::with_capacity(n);
    let mut noise_obj: Vec<f32> = Vec::with_capacity(n * (N_ENT - 1));
    let mut noise_mod: Vec<f32> = Vec::with_capacity(n * (N_MOD - 1));
    let mut osims = [0.0f32; N_ENT];
    let mut msims = [0.0f32; N_MOD];

    for k in 0..n {
        let pi = (k * facts.len()) / n;
        let f = &facts[pi];
        let p = hot_query(&mem, &cb, &a, f, &mut sc).expect("query");
        evidence_sims(&mut sc, &cb, &a, &p, &mut osims, &mut msims);
        sig_obj.push(osims[f.ids[2] as usize]);
        sig_mod.push(msims[f.ids[3] as usize]);
        for (i, &s) in osims.iter().enumerate() {
            if i != f.ids[2] as usize {
                noise_obj.push(s);
            }
        }
        for (i, &s) in msims.iter().enumerate() {
            if i != f.ids[3] as usize {
                noise_mod.push(s);
            }
        }
    }
    let snr_obj = snr(&sig_obj, &noise_obj);
    let snr_mod = snr(&sig_mod, &noise_mod);
    assert!(
        snr_obj > 5.0 && snr_mod > 5.0,
        "SNR baixo: OBJ {snr_obj:.2}σ MOD {snr_mod:.2}σ"
    );

    // Profundidade: 4 passos encadeados, ruído fixo σ=0.35 na entrada.
    let mut rng = ChaCha8Rng::seed_from_u64(SEED ^ 0xD3_7048);
    let depth_n = 120usize;
    let mut wrong = 0usize;
    let mut residual_acc = 0.0f64;
    for k in 0..depth_n {
        let ids = [
            k % N_ENT,
            (k * 3) % N_VERB,
            (k * 7 + 5) % N_ENT,
            (k * 5 + 1) % N_MOD,
        ];
        let truth = ids;
        let mut psi = encode_sentence(&cb, &a, ids);
        add_phase_noise(&mut psi, 0.35, &mut rng);
        let mut ids_now = ids;
        for _step in 0..4 {
            let f = factorize_sentence(&psi, &cb, &a);
            ids_now = f.ids;
            psi = encode_sentence(&cb, &a, ids_now);
        }
        if ids_now != truth {
            wrong += 1;
        }
        residual_acc += reencode_residual(&psi, &cb, &a, ids_now) as f64;
    }
    let depth_err = wrong as f64 / depth_n as f64;
    let residual = residual_acc / depth_n as f64;
    assert!(
        depth_err < 0.05,
        "erro de profundidade {depth_err:.3} >= 0.05"
    );

    // RG: o objetivo macro (4) reduz o erro do campo micro (64).
    let clean: [f32; 64] = std::array::from_fn(|i| 0.8 + 0.01 * i as f32);
    let mut noisy = clean;
    for v in &mut noisy {
        *v += rng.gen_range(-0.6..0.6);
    }
    let rg = rg_topdown(&clean, &noisy, 0.35);
    assert!(
        rg.after < rg.before,
        "RG não reduziu erro: {:.3} -> {:.3}",
        rg.before,
        rg.after
    );
    assert!(rg.after < 0.35, "erro micro pós-RG {:.3} >= 0.35", rg.after);

    println!(
        "E1.G3 snr_obj={snr_obj:.2}σ snr_mod={snr_mod:.2}σ depth_err={depth_err:.3} residual={residual:.4} rg={:.3}->{:.3} align={:.3}",
        rg.before, rg.after, rg.alignment
    );
}

// ---------------------------------------------------------------------------
// Gate 4 — abstenção calibrada
// ---------------------------------------------------------------------------

#[test]
fn g4_abstains_on_oov_noise_and_depth() {
    let cb = Codebooks::build(SEED ^ 4);
    let a = Anchors::build(SEED ^ 4);
    let mut rng = ChaCha8Rng::seed_from_u64(SEED ^ 0xAB_57);
    let n = 120usize;

    // Limpo: não abster e acertar.
    let mut clean_abst = 0usize;
    let mut clean_ok = 0usize;
    for _ in 0..n {
        let ids = [
            rng.gen_range(0..N_ENT),
            rng.gen_range(0..N_VERB),
            rng.gen_range(0..N_ENT),
            rng.gen_range(0..N_MOD),
        ];
        let psi = encode_sentence(&cb, &a, ids);
        let f = factorize_sentence(&psi, &cb, &a);
        if should_abstain(&Evidence::from_factors(&f)) {
            clean_abst += 1;
        } else if f.ids == ids {
            clean_ok += 1;
        }
    }

    // OOV: átomos que não existem em nenhum codebook.
    let mut oov_abst = 0usize;
    for _ in 0..n {
        let mut b = Bundle::zero();
        for r in 0..4 {
            let atom = random_atom(&mut rng);
            b.add_term(&role_term(a.of(r), &atom, r));
        }
        let f = factorize_sentence(&b, &cb, &a);
        if should_abstain(&Evidence::from_factors(&f)) {
            oov_abst += 1;
        }
    }

    // Ruído acima do limiar.
    let mut noisy_abst = 0usize;
    for _ in 0..n {
        let ids = [
            rng.gen_range(0..N_ENT),
            rng.gen_range(0..N_VERB),
            rng.gen_range(0..N_ENT),
            rng.gen_range(0..N_MOD),
        ];
        let mut psi = encode_sentence(&cb, &a, ids);
        add_phase_noise(&mut psi, 1.6, &mut rng);
        let f = factorize_sentence(&psi, &cb, &a);
        if should_abstain(&Evidence::from_factors(&f)) {
            noisy_abst += 1;
        }
    }

    // Profundidade excedida: 5 termos sobrepostos nos 4 papéis.
    let mut deep_abst = 0usize;
    for _ in 0..n {
        let mut b = encode_sentence(
            &cb,
            &a,
            [
                rng.gen_range(0..N_ENT),
                rng.gen_range(0..N_VERB),
                rng.gen_range(0..N_ENT),
                rng.gen_range(0..N_MOD),
            ],
        );
        let extra = random_atom(&mut rng);
        b.add_term(&role_term(a.of(ROLE_S), &extra, ROLE_S));
        let f = factorize_sentence(&b, &cb, &a);
        if should_abstain(&Evidence::from_factors(&f)) {
            deep_abst += 1;
        }
    }

    let clean_fa = clean_abst as f64 / n as f64;
    let clean_acc = clean_ok as f64 / n as f64;
    let oov = oov_abst as f64 / n as f64;
    let noisy = noisy_abst as f64 / n as f64;
    let deep = deep_abst as f64 / n as f64;
    assert!(clean_fa < 0.05, "falso-abster limpo {clean_fa:.3}");
    assert!(clean_acc > 0.95, "acerto limpo {clean_acc:.3}");
    assert!(oov > 0.95, "abstenção OOV {oov:.3}");
    assert!(noisy > 0.95, "abstenção ruído {noisy:.3}");
    assert!(deep > 0.95, "abstenção profundidade {deep:.3}");
    println!(
        "E1.G4 falso_abster={clean_fa:.3} acerto={clean_acc:.3} oov={oov:.3} ruido={noisy:.3} profundidade={deep:.3}"
    );
}

// ---------------------------------------------------------------------------
// Gate 5 — zero alocação e latência
// ---------------------------------------------------------------------------

#[test]
fn g5_zero_alloc_and_latency() {
    let (cb, a, facts, mem) = build(SEED ^ 5);
    let mut sc = QueryScratch::new();
    let n = 240usize;
    let mut picks = Vec::with_capacity(n);
    for k in 0..n {
        picks.push((k * facts.len()) / n);
    }

    // warmup fora da contagem (inicializa a LUT e aquece o shard).
    for _ in 0..32 {
        black_box(hot_query(&mem, &cb, &a, &facts[picks[0]], &mut sc));
    }

    let mut lat: Vec<f64> = Vec::with_capacity(n);
    alloc_probe::arm();
    for &pi in &picks {
        let f = &facts[pi];
        let t0 = Instant::now();
        let answer = hot_query(&mem, &cb, &a, f, &mut sc);
        let dt = t0.elapsed().as_nanos() as f64 / 1000.0;
        lat.push(dt);
        black_box(answer);
    }
    let allocs = alloc_probe::disarm();

    lat.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = lat[(lat.len() as f64 * 0.50) as usize];
    let p99 = lat[((lat.len() as f64 * 0.99) as usize).min(lat.len() - 1)];
    assert_eq!(allocs, 0, "hot-path alocou {allocs} vezes");
    assert!(p50 < 20.0, "p50 {p50:.2} µs >= 20 µs");
    println!("E1.G5 allocs={allocs} p50={p50:.2}us p99={p99:.2}us");
}
