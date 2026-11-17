//! F2/F3 **em dados vivos** — M5 real (D=160) → F1 → F2 → F3 → `cue_into` → M5.
//!
//! Fecha a pergunta #7 do Vol. III §12.5 com número de sistema, não com padrão
//! sintético: os vetores que atravessam a ponte são os **atratores reais** da
//! `ComplexPhaseMemory` (o M5 que o organismo usa), no orçamento de 60 Hz.
//!
//! Cadeia medida, por padrão:
//!
//! ```text
//! T0  record(payload = fases do atrator)        → anel SPSC   (0 alloc)
//! T1  drain_batch → stage_batch → commit_run    → ledger L2
//! T2  cue_into_scoped(cue parcial) → fases       (o ledger reacende)
//!     mem.phases ← fases ; recall_into(...)      → erro angular
//! ```
//!
//! Compara com o **recall direto** (`reset_phases(0)` → `recall_into`) e reporta:
//! erro, *loss*, latência de `record`/`commit_run` e alocações no `T0`.
//!
//! ```text
//! cargo run --release --example f2_f3_live
//! cargo run --release --example f2_f3_live -- --patterns 8 --hidden 0.2
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::f32::consts::TAU;
use std::time::Instant;

use aether_engine::core::knowledge_bridge_f1::{
    Certificate, KnowledgeEvent, KnowledgeSink, Layer, ScopeId, SpscKnowledgePhaseBridge,
};
use aether_engine::core::knowledge_fusion::FusionLedger;
use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};

// ---------------------------------------------------------------------------
// Alocador contador (só a thread principal, só durante o T0)
// ---------------------------------------------------------------------------

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
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

// ---------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------

const D: usize = 160;
const STEPS: usize = 200;
const DT: f32 = 0.05;

fn audit_config() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

/// Padrão harmônico — a classe que o E1b-2 mediu como fronteira (`M_harm = 8`).
fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            let tilt = 0.4 * (order as f32 + 1.0);
            (base + tilt).rem_euclid(TAU)
        })
        .collect()
}

fn wrapped_abs(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % TAU;
    if d > std::f32::consts::PI {
        d -= TAU;
    }
    if d < -std::f32::consts::PI {
        d += TAU;
    }
    d.abs()
}

/// Máscara com exatamente `hidden` posições ocultas (LCG determinístico).
fn mask_exact(seed: u64, n: usize, hidden: usize) -> Vec<bool> {
    let mut x = seed | 1;
    let mut idx: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = ((x >> 33) as usize) % (i + 1);
        idx.swap(i, j);
    }
    let mut mask = vec![true; n];
    for &h in idx.iter().take(hidden.min(n)) {
        mask[h] = false;
    }
    mask
}

/// Fases → payload `f32` complexo intercalado (`re = cos, im = sin`).
fn phases_to_payload(phases: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(phases.len() * 8);
    for p in phases {
        out.extend_from_slice(&p.cos().to_le_bytes());
        out.extend_from_slice(&p.sin().to_le_bytes());
    }
    out
}

/// Pista parcial → consulta intercalada (`0,0` nos ocultos).
fn partial_query(cue: &[f32], mask: &[bool]) -> Vec<f32> {
    let mut q = Vec::with_capacity(cue.len() * 2);
    for (i, p) in cue.iter().enumerate() {
        if mask[i] {
            q.push(p.cos());
            q.push(p.sin());
        } else {
            q.push(0.0);
            q.push(0.0);
        }
    }
    q
}

fn mean_hidden_error(out: &[f32], truth: &[f32], mask: &[bool]) -> f32 {
    let mut sum = 0.0f32;
    let mut n = 0usize;
    for i in 0..truth.len() {
        if !mask[i] {
            sum += wrapped_abs(out[i], truth[i]);
            n += 1;
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let i = ((v.len() as f64 - 1.0) * p).round() as usize;
    v[i]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut patterns = 8usize;
    let mut hidden_frac = 0.2f64;
    let mut i = 0;
    while i < argv.len() {
        match (argv[i].as_str(), argv.get(i + 1)) {
            ("--patterns", Some(v)) => {
                patterns = v.parse().unwrap_or(patterns);
                i += 1;
            }
            ("--hidden", Some(v)) => {
                hidden_frac = v.parse().unwrap_or(hidden_frac);
                i += 1;
            }
            (k, _) => eprintln!("argumento ignorado: {k}"),
        }
        i += 1;
    }
    let hidden = ((D as f64) * hidden_frac).round() as usize;

    println!();
    println!("AETHER · F2/F3 em dados vivos — M5 (D=160) → F1 → F2 → F3 → cue → M5");
    println!("=====================================================================");
    println!(
        "padrões={patterns} ocultos={hidden} ({:.0}%) steps={STEPS} dt={DT}",
        100.0 * hidden_frac
    );
    println!();

    // ---------------- 1. memória de fase real (M5) ----------------
    let mut mem = ComplexPhaseMemory::with_config(D, audit_config());
    let stored: Vec<Vec<f32>> = (0..patterns).map(|o| harmonic_pattern(D, o)).collect();
    for p in &stored {
        mem.imprint(p, 40);
    }

    // ---------------- 2. T0 → T1: grava e consolida ----------------
    let scope_a = ScopeId::new(1, 0, 1);
    let scope_b = ScopeId::new(2, 0, 1);
    let (mut tx, mut rx) = SpscKnowledgePhaseBridge::<64>::new();
    let mut ledger = FusionLedger::new();
    let mut payloads: Vec<Vec<u8>> = Vec::with_capacity(patterns);
    for p in &stored {
        payloads.push(phases_to_payload(p));
    }

    let mask = mask_exact(0x5EED_0F23, D, hidden);
    let mut t0_us: Vec<f64> = Vec::with_capacity(patterns * 8);
    let mut commit_us: Vec<f64> = Vec::with_capacity(8);
    let allocs_t0;

    // T0 puro: 8 repetições por padrão (o anel de 64 não satura)
    for (k, body) in payloads.iter().enumerate() {
        let ev = KnowledgeEvent::new(u128::from(k as u64), Layer::L2Episodic, scope_a, 0, 1_000 + k as u64)
            .with_certificate(Certificate {
                energy: -(k as f32),
                gap: 0.1,
                violations: 0,
            })
            .with_authority(255)
            .with_payload(body);
        for _ in 0..8 {
            let t = Instant::now();
            let _ = tx.record(&ev);
            t0_us.push(t.elapsed().as_secs_f64() * 1e6);
        }
    }

    // orçamento do organismo: 60 Hz = 16,67 ms; quadro de 81,3 µs
    const FRAME_BUDGET_US: f64 = 81.3;
    let mut t0_p50 = t0_us.clone();
    let mut t0_p95 = t0_us.clone();
    let t0_p50 = pct(&mut t0_p50, 0.5);
    let t0_p95 = pct(&mut t0_p95, 0.95);

    // mede alocações de um `record` com a conta armada
    {
        let ev = KnowledgeEvent::new(0, Layer::L2Episodic, scope_a, 0, 0)
            .with_certificate(Certificate {
                energy: 0.0,
                gap: 0.0,
                violations: 0,
            })
            .with_authority(128)
            .with_payload(&payloads[0]);
        ARMED.with(|a| a.set(true));
        ALLOCS.with(|c| c.set(0));
        let _ = tx.record(&ev);
        ARMED.with(|a| a.set(false));
        allocs_t0 = ALLOCS.with(|c| c.get());
    }

    // T1: drena e consolida (fora do quadro)
    let t = Instant::now();
    let mut drained = Vec::new();
    let n = rx.drain_batch(&mut drained, 64);
    ledger.stage_batch(drained);
    let committed = ledger.commit_run();
    commit_us.push(t.elapsed().as_secs_f64() * 1e6);

    // ---------------- 3. T2 → M5: recall direto vs round-trip ----------------
    println!(
        "{:>8} {:>12} {:>12} {:>10} {:>10}",
        "padrão", "direto (rad)", "round-trip", "loss", "Δ (rad)"
    );
    println!("{}", "-".repeat(56));

    let mut direct_errs = Vec::with_capacity(patterns);
    let mut rt_errs = Vec::with_capacity(patterns);
    let mut out = vec![0.0f32; D];
    let mut seed = vec![0.0f32; D];

    for (k, truth) in stored.iter().enumerate() {
        // --- direto: M5 com estado neutro
        mem.reset_phases(0.0);
        let e_direct = mem.recall_into(truth, &mask, STEPS, DT, &mut out);
        let err_direct = mean_hidden_error(&out, truth, &mask);
        let _ = e_direct;

        // --- round-trip: ledger → fases → semente do M5
        let q = partial_query(truth, &mask);
        let hit = ledger
            .cue_into_scoped(scope_a, &q, &mut seed)
            .expect("cue do ledger");
        assert_eq!(hit.key, u128::from(k as u64), "ledger devolveu outra chave");
        assert_eq!(hit.scope, scope_a);
        // o ledger **não substitui** a fase: ele a reacende (semente do atrator)
        mem.phases.copy_from_slice(&seed);
        mem.rotation = 0.0;
        let _ = mem.recall_into(truth, &mask, STEPS, DT, &mut out);
        let err_rt = mean_hidden_error(&out, truth, &mask);

        direct_errs.push(err_direct);
        rt_errs.push(err_rt);
        let loss = (err_rt - err_direct) / err_direct.max(1e-9);
        println!(
            "{:>8} {:>12.4} {:>12.4} {:>9.1}% {:>10.4}",
            k,
            err_direct,
            err_rt,
            100.0 * loss,
            err_rt - err_direct
        );
    }

    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let e_direct = mean(&direct_errs);
    let e_rt = mean(&rt_errs);
    let loss = (e_rt - e_direct) / e_direct.max(1e-9);
    let worst = direct_errs
        .iter()
        .zip(rt_errs.iter())
        .map(|(d, r)| (r - d) / d.max(1e-9))
        .fold(f64::NEG_INFINITY, |a, b| a.max(f64::from(b)));

    // ---------------- 4. R8: dois organismos, zero hits cruzados ----------------
    let (mut tx2, mut rx2) = SpscKnowledgePhaseBridge::<16>::new();
    for (k, body) in payloads.iter().enumerate() {
        let ev = KnowledgeEvent::new(u128::from(k as u64), Layer::L2Episodic, scope_b, 0, 2_000)
            .with_certificate(Certificate {
                energy: 0.0,
                gap: 0.1,
                violations: 0,
            })
            .with_authority(255)
            .with_payload(body);
        let _ = tx2.record(&ev);
    }
    let mut drained_b = Vec::new();
    let nb = rx2.drain_batch(&mut drained_b, 16);
    ledger.stage_batch(drained_b);
    let committed_b = ledger.commit_run();
    let mut seed_b = vec![0.0f32; D];
    let q0 = partial_query(&stored[0], &mask);
    let hit_b = ledger.cue_into_scoped(scope_b, &q0, &mut seed_b)?;

    println!();
    println!("custo (por evento / por run)");
    println!(
        "  record no T0                  : p50 {t0_p50:.3} µs · p95 {t0_p95:.3} µs  ({:.3}% de {FRAME_BUDGET_US} µs)",
        100.0 * t0_p50 / FRAME_BUDGET_US
    );
    println!(
        "  commit_run (T1)               : {:.3} µs para {committed} fatos",
        commit_us[0]
    );
    println!("  alocações no T0               : {allocs_t0}");
    println!();
    println!("fidelidade do round-trip (M5 real, D={D}, {hidden} ocultos)");
    println!("  erro direto (média)           : {e_direct:.4} rad");
    println!("  erro round-trip (média)       : {e_rt:.4} rad");
    println!("  loss                          : {:.1}% (pior padrão {:.1}%)", 100.0 * loss, 100.0 * worst);
    println!();
    println!("integridade e escopo");
    println!(
        "  eventos drenados / consolidados: {n} / {committed} (anel cheio? {})",
        tx.metrics().dropped()
    );
    println!("  chamadas ao ledger no T0      : {}", tx.metrics().t0_ledger_calls());
    println!(
        "  escopo B: drenados={nb} consolidados={committed_b} hit.scope==b: {} · cross_scope_hits={}",
        hit_b.scope == scope_b,
        ledger.cross_scope_hits()
    );
    println!();

    let gate_loss = loss < 0.05;
    let gate_alloc = allocs_t0 == 0;
    let gate_r7 = tx.metrics().t0_ledger_calls() == 0;
    let gate_r8 = ledger.cross_scope_hits() == 0 && hit_b.scope == scope_b;
    let gate_t0 = t0_p95 < FRAME_BUDGET_US * 0.05;
    println!("gates");
    println!("  Q7 loss < 5%                  : {}", if gate_loss { "PASS" } else { "FAIL" });
    println!("  T0 zero alocação              : {}", if gate_alloc { "PASS" } else { "FAIL" });
    println!("  R7 (0 chamadas no T0)         : {}", if gate_r7 { "PASS" } else { "FAIL" });
    println!("  R8 (isolamento de escopo)     : {}", if gate_r8 { "PASS" } else { "FAIL" });
    println!("  T0 ≤ 5% do quadro (4,07 µs)   : {}", if gate_t0 { "PASS" } else { "FAIL" });
    println!();

    std::fs::create_dir_all("target").ok();
    let json = format!(
        "{{\n  \"dims\": {D}, \"patterns\": {patterns}, \"hidden\": {hidden},\n  \"err_direct\": {e_direct:.5},\n  \"err_roundtrip\": {e_rt:.5},\n  \"loss\": {loss:.5},\n  \"worst_loss\": {worst:.5},\n  \"t0_record_us_p50\": {t0_p50:.4},\n  \"t0_record_us_p95\": {t0_p95:.4},\n  \"commit_us\": {:.4},\n  \"t0_allocs\": {allocs_t0},\n  \"t0_ledger_calls\": {},\n  \"dropped\": {},\n  \"cross_scope_hits\": {},\n  \"gates\": {{\"q7\": {gate_loss}, \"alloc\": {gate_alloc}, \"r7\": {gate_r7}, \"r8\": {gate_r8}, \"t0_budget\": {gate_t0}}}\n}}\n",
        commit_us[0],
        tx.metrics().t0_ledger_calls(),
        tx.metrics().dropped(),
        ledger.cross_scope_hits()
    );
    std::fs::write("target/aether_f23_live.json", json)?;
    println!("artefato: target/aether_f23_live.json");
    Ok(())
}
