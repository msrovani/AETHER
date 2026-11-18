//! D1 — orçamentos release do planner. Em debug o teste não mede o motor.
//!
//! MSG 0428 / P1.3: p50/p99 passam a ser percentis de **chamadas individuais**
//! (200 amostras), não mediana/máximo de médias de lotes de 40.

#![cfg(feature = "agent")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use aether_engine::agent::tools::default_tools;
use aether_engine::agent::{AgentConfig, Goal, Planner, SelfModel, ToolRegistry, WorldState};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct CountAlloc;

unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static ALLOC: CountAlloc = CountAlloc;

/// Tetos release neste host (histórico: lote p50≈1µs / p99≈1µs).
/// Com percentis individuais (MSG 0428 OP1) o p99 absoluto sobe para 5µs
/// (OP2) — spike real deixa de ser invisível.
const P50_US: u128 = 2;
const P99_US: u128 = 5;
const ALLOC_PER_CALL: usize = 7;
const RSS_BYTES: u64 = 27_254_375;
const SAMPLES: usize = 200;

#[test]
fn d1_planner_dentro_do_orcamento() {
    if cfg!(debug_assertions) {
        eprintln!("D1 só vale em --release");
        return;
    }
    let cfg = AgentConfig::default();
    let mut registry = ToolRegistry::new(Vec::new());
    for tool in default_tools(&cfg) {
        registry.register(tool).expect("tool");
    }
    let planner = Planner::default();
    let state = WorldState::default();
    let model = SelfModel::default();

    // Goals pré-alocados: allocs medem só o planner, não o format! do subject.
    let goals: Vec<Goal> = (0..SAMPLES)
        .map(|i| Goal::fact(format!("s{}", i % 5), "capital"))
        .collect();
    for goal in goals.iter().take(5) {
        let _ = planner.select(goal, &state, &registry, &model, &[]);
    }

    let before = ALLOCS.load(Ordering::Relaxed);
    let mut samples_ns = Vec::with_capacity(SAMPLES);
    for goal in &goals {
        let t0 = Instant::now();
        let _ = planner.select(goal, &state, &registry, &model, &[]);
        samples_ns.push(t0.elapsed().as_nanos());
    }
    let allocs = ALLOCS.load(Ordering::Relaxed).saturating_sub(before) / SAMPLES;
    samples_ns.sort_unstable();
    let p50_ns = samples_ns[SAMPLES / 2];
    let p99_ns = samples_ns[(SAMPLES * 99 / 100).min(SAMPLES - 1)];
    // Tetos em µs; arredonda para cima só na comparação legível.
    let p50 = p50_ns.div_ceil(1_000);
    let p99 = p99_ns.div_ceil(1_000);
    let rss = working_set();
    eprintln!(
        "D1 p50={p50}us ({p50_ns}ns) p99={p99}us ({p99_ns}ns) allocs/call={allocs} rss={rss} n={SAMPLES}"
    );
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".into());
    let artifact = format!(
        "{{\"host\":{host:?},\"version\":{ver:?},\"mode\":\"per_call\",\"n\":{SAMPLES},\"p50_ns\":{p50_ns},\"p99_ns\":{p99_ns},\"p50_us\":{p50},\"p99_us\":{p99},\"allocs_per_call\":{allocs},\"rss_bytes\":{rss},\"ceiling_p50_us\":{P50_US},\"ceiling_p99_us\":{P99_US},\"ceiling_allocs\":{ALLOC_PER_CALL},\"ceiling_rss_bytes\":{RSS_BYTES},\"ref\":\"MSG_0428_OP1\"}}\n",
        ver = env!("CARGO_PKG_VERSION"),
    );
    let path = std::path::Path::new("target").join("d1_budget.json");
    let _ = std::fs::create_dir_all("target");
    std::fs::write(&path, artifact).expect("artefato D1");
    assert!(p50 <= P50_US, "p50 {p50}us > {P50_US}");
    assert!(p99 <= P99_US, "p99 {p99}us > {P99_US}");
    assert!(allocs <= ALLOC_PER_CALL, "allocs {allocs}");
    // RSS: MSG 0428 R2 — frágil cross-host; no Windows exige teto, fora só loga.
    if cfg!(windows) {
        assert!(rss <= RSS_BYTES, "rss {rss}");
    } else if rss == 0 {
        eprintln!("D1 RSS: working_set=0 fora do Windows (declarado)");
    }
}

#[cfg(windows)]
fn working_set() -> u64 {
    let mut c = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        page_fault: 0,
        peak_ws: 0,
        ws: 0,
        qpeak_paged: 0,
        q_paged: 0,
        qpeak_nonpaged: 0,
        q_nonpaged: 0,
        pagefile: 0,
        peak_pagefile: 0,
    };
    unsafe {
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return 0;
        }
    }
    c.ws as u64
}

#[cfg(not(windows))]
fn working_set() -> u64 {
    0
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
}

#[cfg(windows)]
#[link(name = "psapi")]
extern "system" {
    fn GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
}

#[cfg(windows)]
#[repr(C)]
struct Counters {
    cb: u32,
    page_fault: u32,
    peak_ws: usize,
    ws: usize,
    qpeak_paged: usize,
    q_paged: usize,
    qpeak_nonpaged: usize,
    q_nonpaged: usize,
    pagefile: usize,
    peak_pagefile: usize,
}
