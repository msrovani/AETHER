//! D1 — orçamentos release do planner. Em debug o teste não mede o motor.

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

/// Tetos da corrida release neste host (p50=1µs, p99=1µs, 6 allocs, RSS 24,8 MB),
/// com folga de 10%. O microsegundo inteiro não representa 10% de 1, então o teto
/// de tempo é o próximo inteiro acima da medida.
const P50_US: u128 = 2;
const P99_US: u128 = 2;
const ALLOC_PER_CALL: usize = 7;
const RSS_BYTES: u64 = 27_254_375;

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
    let mut samples = Vec::new();
    let before = ALLOCS.load(Ordering::Relaxed);
    for seed in 0..5u64 {
        let goal = Goal::fact(format!("s{seed}"), "capital");
        let t0 = Instant::now();
        for _ in 0..40 {
            let _ = planner.select(&goal, &state, &registry, &model, &[]);
        }
        samples.push(t0.elapsed().as_micros() / 40);
    }
    let calls = 5 * 40;
    let allocs = ALLOCS.load(Ordering::Relaxed).saturating_sub(before) / calls;
    samples.sort_unstable();
    let p50 = samples[samples.len() / 2];
    let p99 = samples[samples.len() - 1];
    let rss = working_set();
    eprintln!("D1 p50={p50}us p99={p99}us allocs/call={allocs} rss={rss}");
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "host".into());
    let artifact = format!(
        "{{\"host\":{host:?},\"version\":{ver:?},\"seeds\":[0,1,2,3,4],\"p50_us\":{p50},\"p99_us\":{p99},\"allocs_per_call\":{allocs},\"rss_bytes\":{rss},\"ceiling_p50_us\":{P50_US},\"ceiling_p99_us\":{P99_US},\"ceiling_allocs\":{ALLOC_PER_CALL},\"ceiling_rss_bytes\":{RSS_BYTES}}}\n",
        ver = env!("CARGO_PKG_VERSION"),
    );
    let path = std::path::Path::new("target").join("d1_budget.json");
    let _ = std::fs::create_dir_all("target");
    std::fs::write(&path, artifact).expect("artefato D1");
    assert!(p50 <= P50_US, "p50 {p50} > {P50_US}");
    assert!(p99 <= P99_US, "p99 {p99} > {P99_US}");
    assert!(allocs <= ALLOC_PER_CALL, "allocs {allocs}");
    assert!(rss <= RSS_BYTES, "rss {rss}");
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
