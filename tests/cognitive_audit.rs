//! Auditoria cognitiva do AETHER — 12 dimensões, notas 0..100.
//!
//! # Proveniência (leia antes de interpretar as notas)
//!
//! O motor implementa 8 módulos físicos (Ising/bSB, onda espectral, Kuramoto
//! UDP, organismo, memória de fase, sonhador termodinâmico, morfogênese,
//! transdutor). A especificação `COGNITIVE_BENCHMARK_SUITE.md` descreve também
//! subsistemas que **não existem neste crate** (`OnlineKoopmanOperator`, FHRR
//! binding/unbinding, SINDy, `ActiveInferenceEngine`, camadas do Grupo de
//! Renormalização, barreira CBF/Lyapunov). Por honestidade de medição, cada
//! dimensão declara sua proveniência:
//!
//! * `MEASURED` — mede diretamente um mecanismo do motor;
//! * `PROXY`    — usa o mecanismo físico mais próximo + instrumento linear no
//!   harness; **não** é a capacidade nomeada na especificação;
//! * `PARTIAL`  — parte do cenário não existe; a nota cobre só o que é real.
//!
//! As notas são calculadas por `score = 100 · clamp(medido/alvo)`. Metas da
//! especificação são aspiracionais: por padrão a suíte **audita** (registra a
//! nota e emite `[AUDIT][WARN]`), sem transformar alvo de capacidade em falha
//! de CI. Invariantes de segurança (NaN/Inf, alocação zero, forma) são sempre
//! `assert!`. Para exigir também as metas de capacidade:
//!
//! ```text
//! $env:AETHER_AUDIT_STRICT=1; cargo test --release --test cognitive_audit -- --nocapture
//! ```
//!
//! Saída: `target/aether_scorecard.json` + tabela no terminal (impressa pelo
//! último teste que registra, com `--nocapture`).

#![cfg(all(
    feature = "m1",
    feature = "m2",
    feature = "m3",
    feature = "m4",
    feature = "m5",
    feature = "m6",
    feature = "m7"
))]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::too_many_arguments)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::f32::consts::{PI, TAU};
use std::fs;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aether_engine::core::kuramoto_field::{KuramotoConfig, KuramotoPhaseNode, NodeHandle};
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};
use aether_engine::core::wave_interference::WaveInterferenceBlock;
use aether_engine::morphogenesis::{MorphogenesisConfig, MorphogeneticField};
use aether_engine::phase_memory::{ComplexPhaseMemory, PhaseMemoryConfig};
use aether_engine::simulation::closed_loop_organism::{
    ClosedLoopOrganism, OrganismConfig, OrganismFrame,
};
use aether_engine::thermodynamic_dreamer::{DreamerConfig, ThermodynamicDreamer};
use ndarray::{Array2, Array3};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use realfft::RealFftPlanner;

// ---------------------------------------------------------------------------
// Contador de alocação (thread-local: os testes correm em paralelo)
// ---------------------------------------------------------------------------

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
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
static GLOBAL: CountingAlloc = CountingAlloc;

fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    ARMED.with(|a| a.set(true));
    ALLOCS.with(|c| c.set(0));
    let out = f();
    ARMED.with(|a| a.set(false));
    (out, ALLOCS.with(|c| c.get()))
}

// ---------------------------------------------------------------------------
// Scorecard thread-safe + JSON + tabela
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Dimension {
    id: &'static str,
    name: &'static str,
    score: f64,
    status: &'static str,
    confidence: f64,
    note: &'static str,
    metrics: Vec<(&'static str, f64)>,
}

impl Dimension {
    fn new(
        id: &'static str,
        name: &'static str,
        score: f64,
        status: &'static str,
        note: &'static str,
    ) -> Self {
        let confidence = match status {
            "MEASURED" => 1.0,
            "PROXY" => 0.6,
            _ => 0.35,
        };
        Self {
            id,
            name,
            score: if score.is_finite() {
                score.clamp(0.0, 100.0)
            } else {
                0.0
            },
            status,
            confidence,
            note,
            metrics: Vec::new(),
        }
    }

    fn m(mut self, key: &'static str, value: f64) -> Self {
        self.metrics
            .push((key, if value.is_finite() { value } else { 0.0 }));
        self
    }
}

static CARD: OnceLock<Mutex<Vec<Dimension>>> = OnceLock::new();

fn card() -> &'static Mutex<Vec<Dimension>> {
    CARD.get_or_init(|| Mutex::new(Vec::new()))
}

fn record(d: Dimension) {
    let mut guard = card().lock().unwrap_or_else(|e| e.into_inner());
    guard.push(d);
    guard.sort_by(|a, b| a.id.cmp(b.id));
    write_scorecard(&guard);
    if guard.len() == 12 {
        print_table(&guard);
    }
}

fn print_table(dims: &[Dimension]) {
    let acq: f64 = dims.iter().map(|d| d.score).sum::<f64>() / dims.len() as f64;
    let weighted: f64 = dims.iter().map(|d| d.score * d.confidence).sum::<f64>()
        / dims.iter().map(|d| d.confidence).sum::<f64>();
    let coverage: f64 = dims.iter().map(|d| d.confidence).sum::<f64>() / dims.len() as f64;

    println!("\n=== AETHER COGNITIVE AUDIT ===");
    println!(
        "{:<4} {:<36} {:>7} {:>9}  PROVENIENCIA",
        "ID", "DIMENSAO", "NOTA", "STATUS"
    );
    let separador = "-".repeat(118);
    println!("{separador}");
    for d in dims {
        println!(
            "{:<4} {:<36} {:>7.2} {:>9}  {}",
            d.id, d.name, d.score, d.status, d.note
        );
        let pairs: Vec<String> = d
            .metrics
            .iter()
            .map(|(k, v)| format!("{k}={v:.4}"))
            .collect();
        println!("     {}", pairs.join("  "));
    }
    let separador = "-".repeat(118);
    println!("{separador}");
    println!("AETHER Cognitive Quotient (media 12 dimensoes) : {acq:.2}");
    println!("Quotient ponderado por confianca              : {weighted:.2}");
    println!("Cobertura de medicao (1.0 = tudo MEASURED)    : {coverage:.2}");
    println!(
        "Strict mode (metas como gate)                 : {}",
        strict_mode()
    );
    println!("Scorecard JSON: target/aether_scorecard.json\n");
}

fn strict_mode() -> bool {
    std::env::var("AETHER_AUDIT_STRICT").as_deref() == Ok("1")
}

/// Gate de capacidade: falha só em strict mode; caso contrário avisa.
fn target_gate(ok: bool, msg: &str) {
    if ok {
        return;
    }
    if strict_mode() {
        panic!("[AETHER_AUDIT_STRICT] meta nao atingida: {msg}");
    }
    println!("[AUDIT][WARN] meta nao atingida: {msg}");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn write_scorecard(dims: &[Dimension]) {
    let _ = fs::create_dir_all("target");
    let acq: f64 = dims.iter().map(|d| d.score).sum::<f64>() / dims.len().max(1) as f64;
    let weighted: f64 = dims.iter().map(|d| d.score * d.confidence).sum::<f64>()
        / dims.iter().map(|d| d.confidence).sum::<f64>().max(1e-9);
    let coverage: f64 = dims.iter().map(|d| d.confidence).sum::<f64>() / dims.len().max(1) as f64;

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"suite\": \"aether_cognitive_audit\",\n");
    out.push_str("  \"crate\": \"aether_engine\",\n");
    out.push_str(&format!(
        "  \"generated_unix_s\": {},\n",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    out.push_str(&format!(
        "  \"strict_mode\": {},\n",
        if strict_mode() { "true" } else { "false" }
    ));
    out.push_str("  \"dimensions\": [\n");
    for (i, d) in dims.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"id\": \"{}\",\n", d.id));
        out.push_str(&format!("      \"name\": \"{}\",\n", json_escape(d.name)));
        out.push_str(&format!("      \"score\": {:.4},\n", d.score));
        out.push_str(&format!("      \"status\": \"{}\",\n", d.status));
        out.push_str(&format!("      \"confidence\": {:.2},\n", d.confidence));
        out.push_str(&format!(
            "      \"provenance\": \"{}\",\n",
            json_escape(d.note)
        ));
        out.push_str("      \"metrics\": {");
        for (k, (name, value)) in d.metrics.iter().enumerate() {
            if k > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{}\": {:.6}", json_escape(name), value));
        }
        out.push_str("}\n    }");
        if i + 1 < dims.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");
    out.push_str(&format!("\"quotient\": {acq:.4},\n"));
    out.push_str(&format!(
        "\"quotient_confidence_weighted\": {weighted:.4},\n"
    ));
    out.push_str(&format!("\"coverage\": {coverage:.4}\n"));
    out.push_str("}\n");
    let _ = fs::write("target/aether_scorecard.json", out);
}

// ---------------------------------------------------------------------------
// Helpers numéricos e de cenário
// ---------------------------------------------------------------------------

#[inline]
fn wrap(x: f32) -> f32 {
    let mut v = x % TAU;
    if v < 0.0 {
        v += TAU;
    }
    v
}

#[inline]
fn ang_err(a: f32, b: f32) -> f32 {
    (a - b + PI).rem_euclid(TAU) - PI
}

fn score_hi(value: f64, target: f64) -> f64 {
    if !value.is_finite() || target <= 0.0 {
        return 0.0;
    }
    (100.0 * value / target).clamp(0.0, 100.0)
}

fn score_lo(value: f64, target: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return 100.0;
    }
    if value <= target {
        100.0
    } else {
        (100.0 * target / value).clamp(0.0, 100.0)
    }
}

/// Padrão harmônico contínuo (mesma família usada pelos testes do crate).
fn harmonic_pattern(n: usize, order: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 / n as f32;
            let base = (TAU * (order as f32 + 1.0) * x).sin() * 2.0;
            (base + 0.4 * (order as f32 + 1.0)).rem_euclid(TAU)
        })
        .collect()
}

/// Máscara com exatamente `hidden` ocultos (amostragem sem reposição).
fn mask_exact(seed: u64, n: usize, hidden: usize) -> Vec<bool> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.shuffle(&mut rng);
    let mut mask = vec![true; n];
    for &i in idx.iter().take(hidden.min(n)) {
        mask[i] = false;
    }
    mask
}

fn mean_err(recovered: &[f32], target: &[f32], mask: &[bool]) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for i in 0..target.len() {
        if mask[i] {
            continue;
        }
        sum += ang_err(recovered[i], target[i]).abs();
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    sum / count as f32
}

fn mean_err_all(recovered: &[f32], target: &[f32]) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..target.len() {
        sum += ang_err(recovered[i], target[i]).abs();
    }
    sum / target.len().max(1) as f32
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..a.len().min(b.len()) {
        sum += ang_err(a[i], b[i]).cos();
    }
    sum / a.len().min(b.len()).max(1) as f32
}

fn circ_mean(phases: &[f32]) -> f32 {
    let mut s = 0.0f32;
    let mut c = 0.0f32;
    for p in phases {
        s += p.sin();
        c += p.cos();
    }
    s.atan2(c)
}

fn inv_cfg() -> PhaseMemoryConfig {
    PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.002,
    }
}

fn memory_with(patterns: &[Vec<f32>], cfg: PhaseMemoryConfig) -> ComplexPhaseMemory {
    let n = patterns.first().map(|p| p.len()).unwrap_or(0);
    let mut memory = ComplexPhaseMemory::with_config(n, cfg);
    for p in patterns {
        memory.imprint(p, 40);
    }
    memory
}

/// Matriz J frustrada determinística (triângulo de impasse + ruído).
fn frustrated_j(n: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let scale = 1.0 / (n as f64).sqrt();
    let mut j = vec![0.0f32; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let base = if (i / 3 + k / 3) % 2 == 0 { -1.0 } else { 1.0 };
            let v = ((base * 0.6 + rng.gen::<f64>() * 0.8 - 0.4) * scale) as f32;
            j[i * n + k] = v;
            j[k * n + i] = v;
        }
    }
    (j, vec![0.0f32; n])
}

/// RNG rápido sem alocação (xorshift), para o laço de 10k ciclos.
#[inline]
fn xorshift(st: &mut u64) -> f32 {
    let mut x = *st;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *st = x;
    ((x >> 40) as f32) / ((1u32 << 24) as f32)
}

// --- Kuramoto -------------------------------------------------------------

fn kuramoto_cfg(id: u32) -> KuramotoConfig {
    KuramotoConfig {
        node_id: id,
        bind: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
        peers: Vec::new(),
        omega: [6.0, 6.02, 5.98, 6.01],
        k_coupling: 8.0,
        hz: 60.0,
        adjacency: None,
        perturb_duration: Duration::from_millis(250),
        multicast: None,
    }
}

fn spawn_kuramoto(count: u32) -> Vec<NodeHandle> {
    let mut nodes: Vec<KuramotoPhaseNode> = (0..count)
        .map(|id| KuramotoPhaseNode::new(kuramoto_cfg(id)).expect("no kuramoto"))
        .collect();
    let addrs: Vec<SocketAddrV4> = nodes.iter().map(|n| n.local_addr()).collect();
    for (i, node) in nodes.iter_mut().enumerate() {
        let peers = addrs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, a)| *a)
            .collect();
        node.set_peers(peers);
    }
    nodes.into_iter().map(|n| n.spawn()).collect()
}

fn mean_order(handles: &[NodeHandle]) -> Option<f64> {
    let mut sum = 0.0;
    let mut count = 0.0;
    for h in handles {
        if let Some(f) = h.latest() {
            sum += f.order_parameter;
            count += 1.0;
        }
    }
    if count == 0.0 {
        None
    } else {
        Some(sum / count)
    }
}

fn wait_order(handles: &[NodeHandle], target: f64, timeout: Duration) -> (f64, bool) {
    let t0 = Instant::now();
    let mut best = 0.0f64;
    while t0.elapsed() < timeout {
        if let Some(r) = mean_order(handles) {
            best = best.max(r);
            if r >= target {
                return (r, true);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    (best, false)
}

// --- Van der Pol + portadora ---------------------------------------------

fn vdp_series(n: usize, dt: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mu = 1.0f64;
    let (mut x, mut y) = (2.0f64, 0.0f64);
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);
    let mut carrier = Vec::with_capacity(n);
    for k in 0..n {
        let f = |x: f64, y: f64| (y, mu * (1.0 - x * x) * y - x);
        let (k1x, k1y) = f(x, y);
        let (k2x, k2y) = f(x + 0.5 * dt * k1x, y + 0.5 * dt * k1y);
        let (k3x, k3y) = f(x + 0.5 * dt * k2x, y + 0.5 * dt * k2y);
        let (k4x, k4y) = f(x + dt * k3x, y + dt * k3y);
        x += dt * (k1x + 2.0 * k2x + 2.0 * k3x + k4x) / 6.0;
        y += dt * (k1y + 2.0 * k2y + 2.0 * k3y + k4y) / 6.0;
        xs.push(x);
        ys.push(y);
        carrier.push((std::f64::consts::TAU * 0.07 * k as f64).sin());
    }
    (xs, ys, carrier)
}

/// Ridge por equações normais + eliminação gaussiana (instrumento do harness).
fn fit_ridge(xs: &[Vec<f32>], ys: &[[f64; 2]], lambda: f64) -> Vec<f64> {
    let d = xs[0].len() + 1; // bias no fim
    let mut a = vec![0.0f64; d * d];
    let mut b = vec![0.0f64; d * 2];
    for (x, y) in xs.iter().zip(ys.iter()) {
        for i in 0..d {
            let ui = if i + 1 < d { x[i] as f64 } else { 1.0 };
            for j in i..d {
                let uj = if j + 1 < d { x[j] as f64 } else { 1.0 };
                a[i * d + j] += ui * uj;
            }
            b[i * 2] += ui * y[0];
            b[i * 2 + 1] += ui * y[1];
        }
    }
    for i in 0..d {
        for j in 0..i {
            a[i * d + j] = a[j * d + i];
        }
        a[i * d + i] += lambda;
    }

    // Matriz aumentada d x (d+2): [A | b0 | b1]
    let mut aug = vec![0.0f64; d * (d + 2)];
    for i in 0..d {
        for j in 0..d {
            aug[i * (d + 2) + j] = a[i * d + j];
        }
        aug[i * (d + 2) + d] = b[i * 2];
        aug[i * (d + 2) + d + 1] = b[i * 2 + 1];
    }
    for col in 0..d {
        let mut piv = col;
        for r in (col + 1)..d {
            if aug[r * (d + 2) + col].abs() > aug[piv * (d + 2) + col].abs() {
                piv = r;
            }
        }
        if piv != col {
            for j in 0..(d + 2) {
                aug.swap(col * (d + 2) + j, piv * (d + 2) + j);
            }
        }
        let p = aug[col * (d + 2) + col];
        if p.abs() < 1e-15 {
            continue;
        }
        for j in col..(d + 2) {
            aug[col * (d + 2) + j] /= p;
        }
        for r in 0..d {
            if r == col {
                continue;
            }
            let f = aug[r * (d + 2) + col];
            if f == 0.0 {
                continue;
            }
            for j in col..(d + 2) {
                aug[r * (d + 2) + j] -= f * aug[col * (d + 2) + j];
            }
        }
    }
    let mut w = vec![0.0f64; d * 2];
    for i in 0..d {
        w[i * 2] = aug[i * (d + 2) + d];
        w[i * 2 + 1] = aug[i * (d + 2) + d + 1];
    }
    w
}

fn predict(w: &[f64], x: &[f32]) -> [f64; 2] {
    let d = x.len() + 1;
    let mut y0 = 0.0f64;
    let mut y1 = 0.0f64;
    for i in 0..x.len() {
        y0 += w[i * 2] * x[i] as f64;
        y1 += w[i * 2 + 1] * x[i] as f64;
    }
    y0 += w[(d - 1) * 2];
    y1 += w[(d - 1) * 2 + 1];
    [y0, y1]
}

// ---------------------------------------------------------------------------
// T01 — Inferência contínua (H = 15)
// ---------------------------------------------------------------------------

#[test]
fn test_01_inference() {
    const L: usize = 32;
    const H: usize = 15;
    let (xs, ys, carrier) = vdp_series(5000, 0.01);

    let mut block = WaveInterferenceBlock::with_max_len(2, 4, 16, L, 0xA37E_0003);
    let mut u = Array2::<f32>::zeros((2, L));
    let mut feats: Vec<Vec<f32>> = Vec::new();
    let mut targets: Vec<[f64; 2]> = Vec::new();
    for t in (L - 1)..(xs.len() - H) {
        for j in 0..L {
            let idx = t + 1 - L + j;
            u[[0, j]] = (xs[idx] / 2.5) as f32;
            u[[1, j]] = (carrier[idx] * 0.5) as f32;
        }
        let v = block.forward(u.view()).expect("forward T01");
        feats.push(v.iter().copied().collect());
        targets.push([xs[t + H] / 2.5, ys[t + H] / 2.5]);
    }

    let split = 3400;
    assert!(feats.len() > split + 200);
    let lambda = 1e-3 * (feats.len() as f64);
    let w = fit_ridge(&feats[..split], &targets[..split], lambda);

    let mut ss_res = [0.0f64; 2];
    let mut ss_tot = [0.0f64; 2];
    let mut mean = [0.0f64; 2];
    for t in split..feats.len() {
        for k in 0..2 {
            mean[k] += targets[t][k];
        }
    }
    let n_test = (feats.len() - split) as f64;
    mean[0] /= n_test;
    mean[1] /= n_test;
    for t in split..feats.len() {
        let p = predict(&w, &feats[t]);
        for k in 0..2 {
            let e = p[k] - targets[t][k];
            ss_res[k] += e * e;
            let d = targets[t][k] - mean[k];
            ss_tot[k] += d * d;
        }
    }
    let r2_0 = 1.0 - ss_res[0] / ss_tot[0].max(1e-12);
    let r2_1 = 1.0 - ss_res[1] / ss_tot[1].max(1e-12);
    let r2 = 0.5 * (r2_0 + r2_1);
    let mse = 0.5 * (ss_res[0] / n_test + ss_res[1] / n_test);
    let var = 0.5 * (ss_tot[0] / n_test + ss_tot[1] / n_test);
    let nrmse = (mse / var.max(1e-12)).sqrt();

    assert!(r2.is_finite() && nrmse.is_finite());
    assert_eq!(u.dim(), (2, L));

    let score = 0.6 * score_hi(r2, 0.95) + 0.4 * score_lo(nrmse, 0.05);
    record(
        Dimension::new(
            "T01",
            "Inferencia continua (VdP + portadora, H=15)",
            score,
            "PROXY",
            "sem OnlineKoopmanOperator; readout ridge do harness sobre features do WaveInterferenceBlock",
        )
        .m("r2", r2)
        .m("nrmse", nrmse)
        .m("horizon_steps", H as f64)
        .m("features", feats[0].len() as f64),
    );
    println!(
        "[T01] R2={r2:.5} NRMSE={nrmse:.5} (H={H}, {split} treino / {} teste)",
        feats.len() - split
    );

    target_gate(r2 >= 0.95 && nrmse < 0.05, "T01 R2/NRMSE");
}

// ---------------------------------------------------------------------------
// T02 — Confiabilidade: 10.000 ciclos hostis, zero alocação
// ---------------------------------------------------------------------------

#[test]
fn test_02_reliability() {
    const N: usize = 64;
    const L: usize = 128;
    const CYCLES: usize = 10_000;

    let mut wave = WaveInterferenceBlock::with_max_len(4, 8, 32, L, 0x51);
    let mut u = Array2::<f32>::zeros((4, L));
    let mut v = Array2::<f32>::zeros((8, L));
    let mut solver = ThermodynamicIsingSolver::new(1, N, 80, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, N, 80);

    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let mut j = Array2::<f64>::zeros((N, N));
    for i in 0..N {
        for k in (i + 1)..N {
            let val = (rng.gen::<f64>() * 2.0 - 1.0) * 0.15;
            j[[i, k]] = val;
            j[[k, i]] = val;
        }
    }
    // Cenário de auditoria exige rho(J) limitado: normaliza pela linha de
    // Gershgorin para 2.0 (o solver espera escala ~1/sqrt(N)).
    let mut row_max = 0.0f64;
    for i in 0..N {
        let mut s = 0.0f64;
        for k in 0..N {
            s += j[[i, k]].abs();
        }
        row_max = row_max.max(s);
    }
    let norm = 2.0 / row_max.max(1e-12);
    for v in j.iter_mut() {
        *v *= norm;
    }
    let mut h2 = Array2::<f64>::zeros((1, N));

    // Warmup: planos FFT, scratch e statics do harness.
    for _ in 0..64 {
        for ch in 0..4 {
            for k in 0..L {
                u[[ch, k]] = ((k as f32) * 0.1).sin();
            }
        }
        wave.forward_into(u.view(), &mut v).expect("warmup");
        let j3w = j.view().into_shape_with_order((1, N, N)).unwrap();
        solver
            .relax_into(IsingLandscape::new(j3w, h2.view()), &mut out)
            .expect("warmup");
    }

    let (result, allocs) = count_allocs(|| {
        let mut st = 0x1234_5678_9ABC_DEF0u64;
        let mut nonfinite = 0usize;
        let mut max_row = 0.0f64;
        let mut phase = 0.0f32;
        let mut last = [0.0f32; 4];

        for _ in 0..CYCLES {
            // Δt hostil: 50 us .. 25 ms.
            let dt = 50e-6f32 + xorshift(&mut st) * (25e-3f32 - 50e-6f32);
            phase = (phase + dt * TAU * 1.3).rem_euclid(TAU);

            for ch in 0..4 {
                for k in 0..(L - 1) {
                    u[[ch, k]] = u[[ch, k + 1]];
                }
                let omega = 0.9 + 0.05 * ch as f32;
                let clean = (phase * omega).sin();
                // Ruído gaussiano-ish com sigma 0.562 => SNR ~5 dB.
                let noise = (xorshift(&mut st) - 0.5) * 3.464 * 0.5623;
                // 30% de pacotes perdidos: mantém a amostra anterior.
                let lost = xorshift(&mut st) < 0.30;
                let sample = if lost { last[ch] } else { clean + noise };
                last[ch] = sample;
                u[[ch, L - 1]] = sample;
            }

            if wave.forward_into(u.view(), &mut v).is_err() {
                nonfinite += 1;
                continue;
            }
            for i in 0..N {
                h2[[0, i]] = v[[i % 8, (i * 11) % L]] as f64 * 0.25;
            }
            if solver
                .relax_into(
                    IsingLandscape::new(
                        j.view().into_shape_with_order((1, N, N)).unwrap(),
                        h2.view(),
                    ),
                    &mut out,
                )
                .is_err()
            {
                nonfinite += 1;
                continue;
            }
            for value in v.iter() {
                if !value.is_finite() {
                    nonfinite += 1;
                }
            }
            for value in out.energy_trace.row(0).iter() {
                if !value.is_finite() {
                    nonfinite += 1;
                }
            }
        }

        for i in 0..N {
            let mut row = 0.0f64;
            for k in 0..N {
                row += j[[i, k]].abs();
            }
            max_row = max_row.max(row);
        }
        (nonfinite, max_row, phase)
    });

    let (nonfinite, max_row, phase) = result;
    assert_eq!(allocs, 0, "laco critico alocou {allocs} vezes");
    assert_eq!(nonfinite, 0, "valores nao finitos no laco: {nonfinite}");
    assert!(
        max_row.is_finite() && max_row <= 4.0,
        "rho(J) estourou: {max_row}"
    );
    assert!(phase.is_finite());

    let score = if nonfinite == 0 && allocs == 0 && max_row <= 4.0 {
        100.0
    } else {
        0.0
    };
    record(
        Dimension::new(
            "T02",
            "Confiabilidade (10k ciclos, SNR 5dB, 30% perda)",
            score,
            "MEASURED",
            "wave+solver em dt hostil; perda de pacote simulada por retencao de amostra",
        )
        .m("cycles", CYCLES as f64)
        .m("allocations", allocs as f64)
        .m("nonfinite", nonfinite as f64)
        .m("gershgorin_rho", max_row),
    );
    println!(
        "[T02] {CYCLES} ciclos: allocs={allocs} nonfinite={nonfinite} rho_gershgorin={max_row:.4}"
    );
}

// ---------------------------------------------------------------------------
// T03 — Aprendizado one-shot (plasticidade local, sem backprop)
// ---------------------------------------------------------------------------

#[test]
fn test_03_online_learning() {
    const N: usize = 64;
    let mut memory = ComplexPhaseMemory::with_config(N, inv_cfg());
    let regime_a = harmonic_pattern(N, 0);
    let regime_b = harmonic_pattern(N, 3);
    let mask = mask_exact(0xB, N, N * 40 / 100);

    memory.imprint(&regime_a, 40);
    memory.reset_phases(0.0);
    let (pre, _) = memory.recall(&regime_b, &mask, 400, 0.05);
    let err_before = mean_err(&pre, &regime_b, &mask);

    // Ruptura de regime: uma unica apresentacao (uma passada de projecao).
    memory.imprint(&regime_b, 40);
    let cycles_to_switch = 1usize;
    memory.reset_phases(0.0);
    let (post, energy) = memory.recall(&regime_b, &mask, 400, 0.05);
    let err_after = mean_err(&post, &regime_b, &mask);

    let reduction = if err_before > 1e-6 {
        1.0 - (err_after / err_before) as f64
    } else {
        0.0
    };
    assert!(energy.is_finite());
    assert!(err_after.is_finite());

    let score = 0.7 * score_hi(reduction, 0.80) + 0.3 * score_lo(cycles_to_switch as f64, 150.0);
    record(
        Dimension::new(
            "T03",
            "Aprendizado online sem backprop (ruptura A->B)",
            score,
            "PROXY",
            "sem Sherman-Morrison/RLS; mede projecao complexa one-shot + plasticidade local do M5",
        )
        .m("err_before_rad", err_before as f64)
        .m("err_after_rad", err_after as f64)
        .m("reduction", reduction)
        .m("cycles_to_switch", cycles_to_switch as f64),
    );
    println!(
        "[T03] erro {err_before:.4} -> {err_after:.4} rad (reducao {:.1}%, 1 passada)",
        reduction * 100.0
    );

    assert!(err_after < 0.15, "recall apos imprint: {err_after:.4} rad");
    target_gate(reduction >= 0.80, "T03 reducao >= 80%");
}

// ---------------------------------------------------------------------------
// T04 — Autoevolucao estrutural (6 nos, 800 ciclos)
// ---------------------------------------------------------------------------

#[test]
fn test_04_self_evolution() {
    const N: usize = 32;
    const NODES: usize = 6;
    const CYCLES: usize = 800;

    let mut rng = ChaCha8Rng::seed_from_u64(0xE0);
    let mut fields: Vec<MorphogeneticField> = Vec::with_capacity(NODES);
    for _ in 0..NODES {
        let mut j0 = vec![0.0f32; N * N];
        for i in 0..N {
            for k in (i + 1)..N {
                let v = rng.gen::<f32>() * 0.12 - 0.06;
                j0[i * N + k] = v;
                j0[k * N + i] = v;
            }
        }
        fields.push(MorphogeneticField::new(
            N,
            &j0,
            MorphogenesisConfig::default(),
        ));
    }

    let mut solver = ThermodynamicIsingSolver::new(1, N, 60, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, N, 60);
    let mut jbuf = Array3::<f64>::zeros((1, N, N));
    let h = Array2::<f64>::zeros((1, N));
    let mut spins = vec![0.0f32; N];

    let mut f_start = 0.0f64;
    let mut f_end = 0.0f64;
    let mut sp_start = 0.0f64;
    let mut sp_end = 0.0f64;
    let mut rho_max = 0.0f32;
    let mut symmetry_error = 0.0f32;
    let mut collapse_cycle: Option<usize> = None;

    for cycle in 0..CYCLES {
        let surprise = if (cycle / 50) % 2 == 0 {
            2.0f32
        } else {
            0.0f32
        };
        for field in fields.iter_mut() {
            {
                let js = jbuf.as_slice_mut().expect("jbuf contiguo");
                let coupling = field.coupling();
                for i in 0..(N * N) {
                    js[i] = coupling[i] as f64;
                }
            }
            solver
                .relax_into(IsingLandscape::new(jbuf.view(), h.view()), &mut out)
                .expect("relax T04");
            for i in 0..N {
                spins[i] = out.states[[0, i]] as f32;
            }
            let free = field.evolve_topology(surprise, &spins, 0.05);
            assert!(free.is_finite());
        }
        if collapse_cycle.is_none() {
            let active: usize = fields.iter().map(|f| f.active_synapses()).sum();
            if active == 0 {
                collapse_cycle = Some(cycle);
            }
        }
        if cycle == 0 {
            for field in &fields {
                f_start += field.last_free_energy() as f64;
                sp_start += field.sparsity_ratio() as f64;
            }
            f_start /= NODES as f64;
            sp_start /= NODES as f64;
        }
    }

    for field in fields.iter_mut() {
        f_end += field.last_free_energy() as f64;
        sp_end += field.sparsity_ratio() as f64;
        let rho = field.spectral_radius();
        rho_max = rho_max.max(rho);
        for i in 0..N {
            for k in (i + 1)..N {
                let d = (field.coupling()[i * N + k] - field.coupling()[k * N + i]).abs();
                symmetry_error = symmetry_error.max(d);
            }
        }
        for v in field.coupling() {
            assert!(v.is_finite(), "J nao finito na autoevolucao");
        }
    }
    f_end /= NODES as f64;
    sp_end /= NODES as f64;

    let f_improvement = if f_start.abs() > 1e-9 {
        ((f_start - f_end) / f_start.abs()).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let sparsity_gain = sp_end - sp_start;

    assert!(symmetry_error < 1e-5, "J assimetrico: {symmetry_error}");
    assert!(rho_max.is_finite() && rho_max <= 1.01, "rho={rho_max}");

    // Poda saudavel preserva funcao. Rede 100% podada (morta) NAO pontua.
    let alive = sp_end < 0.999 && rho_max > 1e-3;
    let roles_emergent = 0.0;
    let structural = 0.45 * score_hi(f_improvement, 0.50)
        + 0.35 * score_hi(sparsity_gain, 0.30)
        + 0.20 * score_lo(rho_max as f64, 1.0);
    let score = if alive { structural } else { 0.0 };

    record(
        Dimension::new(
            "T04",
            "Autoevolucao estrutural (6 nos x 800 ciclos)",
            score,
            "PARTIAL",
            "morfogenese real (F, poda, homeostase); papeis ausentes; colapso topologico zera a nota",
        )
        .m("free_energy_start", f_start)
        .m("free_energy_end", f_end)
        .m("free_energy_improvement", f_improvement)
        .m("sparsity_start", sp_start)
        .m("sparsity_end", sp_end)
        .m("sparsity_gain", sparsity_gain)
        .m("rho_max", rho_max as f64)
        .m("alive", if alive { 1.0 } else { 0.0 })
        .m(
            "collapse_cycle",
            collapse_cycle.map(|c| c as f64).unwrap_or(-1.0),
        )
        .m("roles_emergent", roles_emergent),
    );
    println!(
        "[T04] F {f_start:.4}->{f_end:.4} (melhora {:.1}%), esparsidade {sp_start:.3}->{sp_end:.3}, rho_max={rho_max:.4}, vivo={alive}, colapso={:?}, papeis=0",
        f_improvement * 100.0,
        collapse_cycle
    );

    assert!(
        sp_end > sp_start,
        "poda nao cresceu: {sp_start:.3} -> {sp_end:.3}"
    );
    if !alive {
        println!("[AUDIT][WARN] T04: topologia colapsou (todas as sinapses podadas); nota zerada");
    }
    target_gate(
        alive && sparsity_gain >= 0.30 && f_improvement >= 0.50,
        "T04 metas",
    );
}

// ---------------------------------------------------------------------------
// T05 — Autocorrecao sob falha critica
// ---------------------------------------------------------------------------

#[test]
fn test_05_self_correction() {
    const N: usize = 64;
    let patterns = vec![
        harmonic_pattern(N, 0),
        harmonic_pattern(N, 1),
        harmonic_pattern(N, 2),
        harmonic_pattern(N, 3),
    ];
    let mut memory = memory_with(&patterns, inv_cfg());
    let target = patterns[0].clone();

    // (a) corrupção de 50% das fases + (b) pulso termico (deslocamento global).
    let mut rng = ChaCha8Rng::seed_from_u64(0xFA11);
    for i in (0..N).step_by(2) {
        memory.phases[i] = wrap(memory.phases[i] + PI * (0.5 + rng.gen::<f32>()));
    }
    let err_corrupt = mean_err_all(&memory.phases, &target);
    let mask = mask_exact(5, N, N * 30 / 100);
    let (_rec, energy) = memory.recall(&target, &mask, 400, 0.05);
    let err_recovered = mean_err_all(&memory.phases, &target);
    let memory_recovery = if err_corrupt > 1e-6 {
        (1.0 - err_recovered / err_corrupt).clamp(0.0, 1.0)
    } else {
        0.0
    };
    assert!(energy.is_finite() && err_recovered.is_finite());

    // Kuramoto: coerencia apos perturbacao severa.
    let handles = spawn_kuramoto(3);
    let (r0, converged) = wait_order(&handles, 0.95, Duration::from_secs(4));
    handles[0].perturb([120.0, 120.0, 0.0]).expect("perturb");
    let fault_t0 = Instant::now();
    let mut min_r = 1.0f64;
    let mut dipped = false;
    while fault_t0.elapsed() < Duration::from_millis(500) {
        if let Some(r) = mean_order(&handles) {
            min_r = min_r.min(r);
            if r < 0.85 {
                dipped = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut recovered_r = 0.0f64;
    let mut recovered = false;
    while fault_t0.elapsed() < Duration::from_secs(3) {
        if let Some(r) = mean_order(&handles) {
            if r > 0.92 {
                recovered_r = r;
                recovered = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let recovery_cycles = fault_t0.elapsed().as_secs_f64() * 60.0;
    for h in handles {
        let _ = h.stop();
    }

    assert!(
        r0.is_finite() && converged,
        "nao sincronizou antes da falha"
    );
    assert!(recovered_r.is_finite());
    assert!(min_r.is_finite() && (0.0..=1.0).contains(&min_r));

    // Sem CBF/Lyapunov no motor: a seguranca de V(z) <= Vmax nao e verificavel.
    let cbf_available = 0.0;
    let score =
        0.6 * score_lo(recovery_cycles, 40.0) + 0.4 * score_hi(memory_recovery as f64, 0.50);

    record(
        Dimension::new(
            "T05",
            "Autocorrecao sob falha critica (Kuramoto + M5)",
            score,
            "PARTIAL",
            "sem CBF/Lyapunov; mede ressincronizacao fisica e recuperacao de fases corrompidas",
        )
        .m("r_before_fault", r0)
        .m("r_min_during_fault", min_r)
        .m("fault_effective", if dipped { 1.0 } else { 0.0 })
        .m("recovery_cycles", recovery_cycles)
        .m("r_after_recovery", recovered_r)
        .m("memory_err_corrupt", err_corrupt as f64)
        .m("memory_err_recovered", err_recovered as f64)
        .m("memory_recovery", memory_recovery as f64)
        .m("cbf_available", cbf_available),
    );
    println!(
        "[T05] R={r0:.4} -> min {min_r:.4} -> R={recovered_r:.4} em {recovery_cycles:.1} ciclos (falha efetiva={dipped}); memoria {err_corrupt:.3}->{err_recovered:.3} rad"
    );

    assert!(
        recovered_r <= 1.0 && recovered,
        "Kuramoto nao recuperou R>0.92"
    );
    assert!(
        dipped,
        "perturbacao nao derrubou R<0.85 (falha nao foi severa)"
    );
    target_gate(recovery_cycles < 40.0, "T05 recuperacao < 40 ciclos");
}

// ---------------------------------------------------------------------------
// T06 — Coerencia multi-escala e de enxame
// ---------------------------------------------------------------------------

#[test]
fn test_06_coherence() {
    // 4 nos UDP (o hardware real de enxame).
    let handles = spawn_kuramoto(4);
    let (r_align, converged) = wait_order(&handles, 0.90, Duration::from_secs(4));
    for h in handles {
        let _ = h.stop();
    }

    // Chattering de atuacao do organismo (estado emergente i8 por quadro).
    let cfg = OrganismConfig {
        steps: 120,
        perturb_after: Duration::from_secs(3600),
        ..Default::default()
    };
    let steps = cfg.steps;
    let n = cfg.decision_dim;
    let mut organism = ClosedLoopOrganism::new(cfg).expect("organismo T06");
    let mut frame = OrganismFrame::with_capacity(steps, n);
    // Dá tempo aos 3 nós UDP de publicarem o primeiro quadro.
    std::thread::sleep(Duration::from_millis(300));
    for _ in 0..6 {
        organism.step_into(&mut frame).expect("warmup T06");
    }
    let mut prev = frame.states.clone();
    let mut chatter_sum = 0.0f64;
    let mut frames = 0usize;
    let mut r_org = 0.0f64;
    for _ in 0..20 {
        organism.step_into(&mut frame).expect("step T06");
        let mut d2 = 0.0f64;
        for i in 0..n {
            let d = (frame.states[i] - prev[i]) as f64;
            d2 += d * d;
        }
        chatter_sum += (d2.sqrt()) / (n as f64).sqrt();
        prev.copy_from_slice(&frame.states);
        r_org += frame.order_parameter;
        frames += 1;
        for s in &frame.states {
            assert!(*s == 1 || *s == -1, "estado fora de {{-1,+1}}");
        }
        assert!(frame.entropy_raw.is_finite() && frame.entropy_wave.is_finite());
    }
    organism.shutdown();
    let chatter = chatter_sum / frames as f64;
    r_org /= frames as f64;

    assert!(r_align.is_finite() && r_org.is_finite());
    assert!(converged, "4 nos nao convergiram para R>0.90");

    // Camadas RG (60/15/3.75 Hz) nao existem no motor.
    let rg_layers = 0.0;
    let score = 0.7 * score_hi(r_align, 0.85) + 0.3 * score_lo(chatter, 0.50);

    record(
        Dimension::new(
            "T06",
            "Coerencia de enxame (4 nos) e chattering",
            score,
            "PARTIAL",
            "sem Grupo de Renormalizacao; R_align de 4 nos UDP + suavidade do atuador do organismo",
        )
        .m("r_align", r_align)
        .m("r_organism", r_org)
        .m("chattering", chatter)
        .m("rg_layers", rg_layers),
    );
    println!("[T06] R_align(4 nos)={r_align:.4} R_organismo={r_org:.4} chattering={chatter:.4}");

    target_gate(r_align > 0.85, "T06 R_align > 0.85");
}

// ---------------------------------------------------------------------------
// T07 — Exploracao epistemica (proxy: incerteza do sonhador)
// ---------------------------------------------------------------------------

#[test]
fn test_07_epistemic_search() {
    const N: usize = 32;
    const SAMPLES: usize = 32;
    const STEPS: usize = 200;

    let mut dreamer = ThermodynamicDreamer::new(
        N,
        DreamerConfig {
            a0: 1.0,
            gamma_damp: 0.6,
            beta_chaos: 0.55,
            seed: 0xA37E_0002,
        },
    );

    // Zona A: paisagem ordenada (conhecida -> baixa incerteza).
    let mut j_known = vec![0.0f32; N * N];
    for i in 0..N {
        for k in (i + 1)..N {
            j_known[i * N + k] = 0.05;
            j_known[k * N + i] = 0.05;
        }
    }
    let h_known: Vec<f32> = (0..N)
        .map(|i| if i % 2 == 0 { 0.6 } else { -0.6 })
        .collect();
    let known = dreamer.imagine_futures(&j_known, &h_known, 1.0, SAMPLES, STEPS, 0.05);

    // Zona B: paisagem frustrada (inexplorada -> alta incerteza).
    let (j_frus, h_frus) = frustrated_j(N, 0xD12E);
    let unknown = dreamer.imagine_futures(&j_frus, &h_frus, 1.0, SAMPLES, STEPS, 0.05);
    // Ordem frio->quente (mesma sequência do invariante M6 documentado).
    let cold = dreamer.imagine_futures(&j_frus, &h_frus, 0.001, SAMPLES, STEPS, 0.05);
    let hot = dreamer.imagine_futures(&j_frus, &h_frus, 1.5, SAMPLES, STEPS, 0.05);

    let u_known = known.epistemic_uncertainty;
    let u_unknown = unknown.epistemic_uncertainty;
    let ratio = u_unknown / u_known.max(1e-6);
    assert!(u_unknown.is_finite() && u_known.is_finite());
    assert!((0.0..=1.0).contains(&u_unknown) && (0.0..=1.0).contains(&u_known));
    assert!(
        hot.epistemic_uncertainty > cold.epistemic_uncertainty,
        "U(T) nao monotona"
    );

    // "Explorar ate U < 0.20 e entao reduzir esforco": proxy = U conhecida baixa.
    let mastered = u_known < 0.20;
    let score = 0.5 * score_hi(ratio, 2.0)
        + 0.3 * score_hi(1.0 - u_known, 0.80)
        + 0.2 * if mastered { 100.0 } else { 0.0 };

    record(
        Dimension::new(
            "T07",
            "Exploracao epistemica (U do sonhador)",
            score,
            "PROXY",
            "sem ActiveInferenceEngine; mede separacao de U entre paisagem conhecida e frustrada",
        )
        .m("u_known", u_known)
        .m("u_unknown", u_unknown)
        .m("ratio", ratio)
        .m("u_hot", hot.epistemic_uncertainty)
        .m("u_cold", cold.epistemic_uncertainty)
        .m("mastered_u_below_0_20", if mastered { 1.0 } else { 0.0 }),
    );
    println!(
        "[T07] U(conhecida)={u_known:.4} U(frustrada)={u_unknown:.4} razao={ratio:.3} U(hot)={:.4} U(cold)={:.4}",
        hot.epistemic_uncertainty, cold.epistemic_uncertainty
    );

    target_gate(ratio >= 2.0, "T07 separacao de incerteza");
}

// ---------------------------------------------------------------------------
// T08 — Respostas complexas multi-modais (4 estagios)
// ---------------------------------------------------------------------------

#[test]
fn test_08_complex_responses() {
    // (1) 3 harmonicos ocultos em ruido, via FFT — instrumento montado aqui.
    const L: usize = 256;
    let expected = [7usize, 23, 41];
    let mut rng = ChaCha8Rng::seed_from_u64(0x08);
    let mut signal = vec![0.0f32; L];
    for (k, amp) in [1.0f32, 0.8, 0.6].iter().enumerate() {
        let bin = expected[k] as f32;
        for (i, s) in signal.iter_mut().enumerate() {
            *s += amp * (TAU * bin * i as f32 / L as f32).sin();
        }
    }
    for s in signal.iter_mut() {
        *s += (rng.gen::<f32>() - 0.5) * 0.8;
    }
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(L);
    let mut indata = r2c.make_input_vec();
    let mut spectrum = r2c.make_output_vec();
    // Warmup do plano FFT (fora da janela medida).
    indata.copy_from_slice(&signal);
    r2c.process(&mut indata, &mut spectrum).expect("fft warmup");

    // (2) memoria associativa com 4 padroes (aprendizado fora da janela).
    let n_mem = 64;
    let patterns: Vec<Vec<f32>> = (1..=4).map(|o| harmonic_pattern(n_mem, o)).collect();
    let mut memory = memory_with(&patterns, inv_cfg());
    let mut qrng = ChaCha8Rng::seed_from_u64(0xB2);
    let cues: Vec<(Vec<f32>, Vec<bool>)> = patterns
        .iter()
        .enumerate()
        .map(|(k, p)| {
            let mask = mask_exact(0x100 + k as u64, n_mem, n_mem * 40 / 100);
            let mut cue = vec![0.0f32; n_mem];
            for i in 0..n_mem {
                let noise = (qrng.gen::<f32>() - 0.5) * 0.5;
                cue[i] = if mask[i] { p[i] + noise } else { 0.0 };
            }
            (cue, mask)
        })
        .collect();

    // (3) campo homeostatico ja evoluido (treino fora da janela).
    const NH: usize = 32;
    let mut hrng = ChaCha8Rng::seed_from_u64(0x11);
    let mut j0 = vec![0.0f32; NH * NH];
    for i in 0..NH {
        for k in (i + 1)..NH {
            let v = hrng.gen::<f32>() * 0.16 - 0.08;
            j0[i * NH + k] = v;
            j0[k * NH + i] = v;
        }
    }
    let mut field = MorphogeneticField::new(NH, &j0, MorphogenesisConfig::default());
    let spins: Vec<f32> = (0..NH)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    for _ in 0..100 {
        field.evolve_topology(1.0, &spins, 0.05);
    }

    // (4) sonhador 64-dim, com pool rayon e buffers ja aquecidos.
    const ND: usize = 64;
    let (jd, hd) = frustrated_j(ND, 0xDEED);
    let mut dreamer = ThermodynamicDreamer::new(ND, DreamerConfig::default());
    let _ = dreamer.imagine_futures(&jd, &hd, 1.2, 4, 10, 0.05);
    let reference = dreamer.random_reference_energy(&jd, &hd, 256);

    // ------------------------------------------------------------------
    // Janela medida: UM estimulo composto (inferencia, nao aprendizado).
    // ------------------------------------------------------------------
    let total = Instant::now();

    indata.copy_from_slice(&signal);
    r2c.process(&mut indata, &mut spectrum).expect("fft");
    let mut peaks: Vec<(usize, f32)> = spectrum
        .iter()
        .enumerate()
        .filter(|(k, _)| *k >= 2 && *k < L / 2 - 1)
        .map(|(k, c)| (k, c.norm()))
        .collect();
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let found: Vec<usize> = peaks.iter().take(3).map(|(k, _)| *k).collect();
    let fft_hits = expected
        .iter()
        .filter(|e| found.iter().any(|f| f.abs_diff(**e) <= 1))
        .count();
    let fft_acc = fft_hits as f64 / 3.0;

    let mut recall_hits = 0usize;
    for (k, (cue, mask)) in cues.iter().enumerate() {
        memory.reset_phases(0.0);
        let (rec, _) = memory.recall(cue, mask, 300, 0.05);
        if mean_err(&rec, &patterns[k], mask) < 0.30 {
            recall_hits += 1;
        }
    }
    let recall_acc = recall_hits as f64 / 4.0;

    let rho = field.spectral_radius();
    let homeo_acc = if rho.is_finite() && rho <= 1.001 {
        1.0
    } else {
        0.0
    };

    let ensemble = dreamer.imagine_futures(&jd, &hd, 1.2, 32, 150, 0.05);
    let collapse_acc = if ensemble.mean_energy < reference && ensemble.epistemic_uncertainty > 0.0 {
        1.0
    } else {
        0.0
    };

    let elapsed_ms = total.elapsed().as_secs_f64() * 1e3;
    let composite = (fft_acc + recall_acc + homeo_acc + collapse_acc) / 4.0;

    assert!(rho.is_finite());
    assert!(ensemble.mean_energy.is_finite());
    assert!(elapsed_ms.is_finite());
    assert!(composite > 0.25, "composto degenerado: {composite}");

    let score = 0.8 * (100.0 * composite) + 0.2 * score_lo(elapsed_ms, 1.5);
    record(
        Dimension::new(
            "T08",
            "Respostas complexas multi-modais (4 estagios)",
            score,
            "PARTIAL",
            "FFT+memoria+homeostase+sonhador reais; 'FHRR unbinding' aproximado por recall de fase",
        )
        .m("fft_accuracy", fft_acc)
        .m("recall_accuracy", recall_acc)
        .m("homeostasis_accuracy", homeo_acc)
        .m("collapse_accuracy", collapse_acc)
        .m("composite", composite)
        .m("elapsed_ms", elapsed_ms)
        .m("rho", rho as f64),
    );
    println!(
        "[T08] fft={fft_acc:.2} recall={recall_acc:.2} homeo={homeo_acc:.2} colapso={collapse_acc:.2} composto={composite:.3} tempo={elapsed_ms:.3} ms"
    );

    target_gate(
        composite > 0.90 && elapsed_ms < 1.5,
        "T08 composto > 90% em <1.5ms",
    );
}

// ---------------------------------------------------------------------------
// T09 — Matematica: MAX-CUT via bSB (SINDy ausente)
// ---------------------------------------------------------------------------

#[test]
fn test_09_mathematics() {
    const N: usize = 64;
    const INSTANCES: usize = 20;
    const STEPS: usize = 300;

    let mut solver = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, N, STEPS);
    let h = Array2::<f64>::zeros((1, N));
    let mut j3 = Array3::<f64>::zeros((1, N, N));
    let mut ratios = Vec::with_capacity(INSTANCES);

    for inst in 0..INSTANCES {
        let mut rng = ChaCha8Rng::seed_from_u64(0xC07 + inst as u64);
        let labels: Vec<i8> = (0..N)
            .map(|_| if rng.gen::<bool>() { 1 } else { -1 })
            .collect();
        let mut w = vec![0.0f64; N * N];
        let mut total_positive = 0.0f64;
        for i in 0..N {
            for k in (i + 1)..N {
                let base = if labels[i] != labels[k] { 1.0 } else { -0.3 };
                let val = base + (rng.gen::<f64>() - 0.5) * 0.3;
                w[i * N + k] = val;
                w[k * N + i] = val;
                if val > 0.0 {
                    total_positive += val;
                }
            }
        }
        let reference = cut_value(&w, &labels, N);
        assert!(reference > 0.0, "corte de referencia degenerado");

        for i in 0..N {
            for k in 0..N {
                j3[[0, i, k]] = -w[i * N + k];
            }
        }
        solver.reset(0x9000 + inst as u64);
        solver
            .relax_into(IsingLandscape::new(j3.view(), h.view()), &mut out)
            .expect("relax T09");
        let states: Vec<i8> = out.states.row(0).to_vec();
        let cut = cut_value(&w, &states, N);
        assert!(cut.is_finite() && cut >= 0.0 && cut <= total_positive + 1e-9);
        ratios.push((cut / reference).max(0.0));
    }

    let mean_ratio = ratios.iter().sum::<f64>() / INSTANCES as f64;
    let min_ratio = ratios.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(mean_ratio.is_finite());

    // SINDy nao existe no crate; a nota cobre apenas a otimizacao combinatoria.
    let score = score_hi(mean_ratio, 0.96);
    record(
        Dimension::new(
            "T09",
            "Matematica: MAX-CUT bSB (SINDy ausente)",
            score,
            "PARTIAL",
            "bSB real em 20 grafos N=64; descoberta simbolica SINDy nao implementada",
        )
        .m("maxcut_ratio_mean", mean_ratio)
        .m("maxcut_ratio_min", min_ratio)
        .m("instances", INSTANCES as f64)
        .m("sindy_available", 0.0),
    );
    println!(
        "[T09] MAX-CUT razao media={mean_ratio:.4} (min {min_ratio:.4}) em {INSTANCES} grafos N={N}; SINDy=ausente"
    );

    target_gate(mean_ratio >= 0.96, "T09 MAX-CUT >= 0.96");
}

fn cut_value(w: &[f64], s: &[i8], n: usize) -> f64 {
    let mut cut = 0.0f64;
    for i in 0..n {
        for k in (i + 1)..n {
            if s[i] != s[k] {
                cut += w[i * n + k];
            }
        }
    }
    cut
}

// ---------------------------------------------------------------------------
// T10 — Resistencia a alucinacoes
// ---------------------------------------------------------------------------

#[test]
fn test_10_hallucinations() {
    const N: usize = 32;
    let mut dreamer = ThermodynamicDreamer::new(N, DreamerConfig::default());

    // (a) ruido branco puro: incerteza deve ser alta.
    let (j_noise, h_noise) = frustrated_j(N, 0x0BAD);
    let noise = dreamer.imagine_futures(&j_noise, &h_noise, 1.5, 32, 150, 0.05);
    let u_noise = noise.epistemic_uncertainty;

    // (b) chave inexistente: similaridade deve ser baixa.
    const NM: usize = 64;
    let patterns: Vec<Vec<f32>> = (0..5).map(|o| harmonic_pattern(NM, o)).collect();
    let mut memory = memory_with(&patterns, inv_cfg());
    let mut rng = ChaCha8Rng::seed_from_u64(0x7A17);
    let mut cue = vec![0.0f32; NM];
    for c in cue.iter_mut() {
        *c = rng.gen::<f32>() * TAU;
    }
    let mask = mask_exact(0x99, NM, NM * 40 / 100);
    memory.reset_phases(0.0);
    let (rec, _) = memory.recall(&cue, &mask, 200, 0.05);
    let max_sim = patterns
        .iter()
        .map(|p| cosine_sim(&rec, p))
        .fold(f32::NEG_INFINITY, f32::max);

    // (c) sensores contraditorios: h alternado + paisagem frustrada.
    let (j_contra, _) = frustrated_j(N, 0x0BAD);
    let h_contra: Vec<f32> = (0..N)
        .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
        .collect();
    let contradict = dreamer.imagine_futures(&j_contra, &h_contra, 1.2, 32, 150, 0.05);
    let u_contra = contradict.epistemic_uncertainty;
    let h_order: Vec<f32> = vec![0.5; N];
    let ordered = dreamer.imagine_futures(&j_contra, &h_order, 1.2, 32, 150, 0.05);
    let u_order = ordered.epistemic_uncertainty;
    let contra_ratio = u_contra / u_order.max(1e-6);

    assert!(u_noise.is_finite() && (0.0..=1.0).contains(&u_noise));
    assert!(max_sim.is_finite());
    assert!(u_contra.is_finite());

    let s_a = score_hi(u_noise, 0.75);
    let s_b = score_lo(max_sim as f64, 0.15);
    let s_c = score_hi(contra_ratio, 1.5);
    let score = (s_a + s_b + s_c) / 3.0;

    record(
        Dimension::new(
            "T10",
            "Resistencia a alucinacoes (3 armadilhas)",
            score,
            "PROXY",
            "U de Langevin + similaridade de fase; nao ha gate explicito de abstencao no motor",
        )
        .m("u_white_noise", u_noise)
        .m("max_similarity_absent_key", max_sim as f64)
        .m("u_contradictory", u_contra)
        .m("u_ordered", u_order)
        .m("contradictory_ratio", contra_ratio),
    );
    println!(
        "[T10] U(ruido)={u_noise:.4} sim(chave ausente)={max_sim:.4} U(contraditorio)={u_contra:.4} razao={contra_ratio:.3}"
    );

    target_gate(u_noise > 0.75 && max_sim < 0.15, "T10 armadilhas");
}

// ---------------------------------------------------------------------------
// T11 — Imunidade a desvio de pensamento (2000 ciclos)
// ---------------------------------------------------------------------------

#[test]
fn test_11_thought_drift() {
    const N: usize = 64;
    const CYCLES: usize = 2000;
    let patterns: Vec<Vec<f32>> = (0..5).map(|o| harmonic_pattern(N, o)).collect();
    let mut memory = memory_with(&patterns, inv_cfg());
    let macro_target = circ_mean(&patterns[0]);

    // Bombardeio de distração de alta frequência: kick de fase por ciclo.
    for t in 0..CYCLES {
        for i in 0..N {
            let kick = 0.08 * (0.7 * t as f32 + i as f32).sin();
            memory.phases[i] = wrap(memory.phases[i] + kick);
        }
        memory.free_run(1, 0.05);
    }

    let mut max_err = 0.0f32;
    let mut macro_drift = 0.0f32;
    let mut rng = ChaCha8Rng::seed_from_u64(0x5D);
    for (k, p) in patterns.iter().enumerate() {
        let mask = mask_exact(0x200 + k as u64, N, N * 30 / 100);
        let mut cue = p.clone();
        for i in 0..N {
            if mask[i] {
                cue[i] = wrap(cue[i] + (rng.gen::<f32>() - 0.5) * 0.1);
            }
        }
        let (rec, energy) = memory.recall(&cue, &mask, 200, 0.05);
        assert!(energy.is_finite());
        max_err = max_err.max(mean_err(&rec, p, &mask));
        if k == 0 {
            macro_drift = ang_err(circ_mean(&rec), macro_target).abs();
        }
    }
    assert!(macro_drift.is_finite());

    let score = 0.6 * score_lo(max_err as f64, 0.15) + 0.4 * score_lo(macro_drift as f64, 0.20);
    record(
        Dimension::new(
            "T11",
            "Imunidade a desvio de pensamento (2000 ciclos)",
            score,
            "PARTIAL",
            "sem camada macro RG; mede retencao das 5 memorias e fase macroscopica sob distracao",
        )
        .m("max_recall_error_rad", max_err as f64)
        .m("macro_drift_rad", macro_drift as f64)
        .m("cycles", CYCLES as f64),
    );
    println!(
        "[T11] erro max recall={max_err:.4} rad, desvio macro={macro_drift:.4} rad apos {CYCLES} ciclos"
    );
    target_gate(max_err < 0.15 && macro_drift < 0.20, "T11 retencao");
}

// ---------------------------------------------------------------------------
// T12 — RAG holografico (50 fatos, 40% oclusao, ruido +-0.25 rad)
// ---------------------------------------------------------------------------

#[test]
fn test_12_holographic_rag() {
    const N: usize = 200; // capacidade = min(N/4, 64) = 50 padroes
    const FACTS: usize = 50;
    const QUERIES: usize = 20;
    const STEPS: usize = 80;

    let cfg = PhaseMemoryConfig {
        omega0: 0.0,
        clamp_strength: 8.0,
        eta_w: 0.12,
        eta_delta: 0.08,
        gamma_w: 0.001,
    };
    let mut memory = ComplexPhaseMemory::with_config(N, cfg);
    let mut rng = ChaCha8Rng::seed_from_u64(0xFAC7);
    let mut facts: Vec<Vec<f32>> = Vec::with_capacity(FACTS);
    for _ in 0..FACTS {
        let mut p = vec![0.0f32; N];
        for v in p.iter_mut() {
            *v = rng.gen::<f32>() * TAU;
        }
        facts.push(p);
    }
    assert!(memory.pattern_capacity() >= FACTS);
    for f in &facts {
        memory.imprint(f, 8);
    }
    assert_eq!(memory.stored_patterns(), FACTS);

    let mut hits = 0usize;
    let mut latencies_us = Vec::with_capacity(QUERIES);
    let mut snr_samples = Vec::with_capacity(QUERIES);
    for q in 0..QUERIES {
        let truth = (rng.gen::<u32>() as usize) % FACTS;
        let mask = mask_exact(0x7000 + q as u64, N, N * 40 / 100);
        let mut cue = vec![0.0f32; N];
        for i in 0..N {
            if mask[i] {
                let noise = (rng.gen::<f32>() - 0.5) * 0.5; // +-0.25 rad
                cue[i] = facts[truth][i] + noise;
            }
        }
        memory.reset_phases(0.0);
        let t0 = Instant::now();
        let (rec, _) = memory.recall(&cue, &mask, STEPS, 0.05);
        latencies_us.push(t0.elapsed().as_secs_f64() * 1e6);

        let sims: Vec<f32> = facts.iter().map(|p| cosine_sim(&rec, p)).collect();
        let mut best = 0usize;
        for i in 1..FACTS {
            if sims[i] > sims[best] {
                best = i;
            }
        }
        if best == truth {
            hits += 1;
        }
        let correct = sims[truth] as f64;
        let mut others: Vec<f64> = sims
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != truth)
            .map(|(_, v)| *v as f64)
            .collect();
        let mean_other = others.iter().sum::<f64>() / others.len() as f64;
        let var = others
            .iter()
            .map(|v| (v - mean_other) * (v - mean_other))
            .sum::<f64>()
            / others.len() as f64;
        others.clear();
        snr_samples.push((correct - mean_other) / var.sqrt().max(1e-6));
    }

    let recall_at_1 = hits as f64 / QUERIES as f64;
    let mean_latency = latencies_us.iter().sum::<f64>() / QUERIES as f64;
    let snr = snr_samples.iter().sum::<f64>() / QUERIES as f64;

    assert!(recall_at_1.is_finite() && mean_latency.is_finite() && snr.is_finite());
    let score = 0.5 * (100.0 * recall_at_1)
        + 0.25 * score_hi(snr, 3.0)
        + 0.25 * score_lo(mean_latency, 50.0);

    record(
        Dimension::new(
            "T12",
            "RAG holografico (50 fatos, 40% oclusao)",
            score,
            "PARTIAL",
            "ComplexPhaseMemory sem FHRR bind/unbind nem indice textual; 50 padroes de fase aleatorios",
        )
        .m("recall_at_1", recall_at_1)
        .m("snr_sigma", snr)
        .m("latency_us_mean", mean_latency)
        .m("facts", FACTS as f64)
        .m("occlusion_pct", 40.0),
    );
    println!(
        "[T12] Recall@1={recall_at_1:.3} SNR={snr:.2}sigma latencia={mean_latency:.1} us (N={N}, {FACTS} fatos)"
    );

    target_gate(
        recall_at_1 >= 0.90 && snr > 3.0 && mean_latency < 50.0,
        "T12 RAG",
    );
}
