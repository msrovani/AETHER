//! Benchmark de vazão cognitiva do AETHER (Módulo 4, quadro completo).
//!
//! Para cada regime (`Basal`, `Vigilante`, `Crítico` = 60/120/240 passos de
//! relaxamento por quadro) mede durante 5 s contínuos:
//!
//! 1. latência por ciclo (p50/p95/p99/p99.9, em µs);
//! 2. ciclos cognitivos/s;
//! 3. operações sinápticas/espectrais/s (SOP/s);
//! 4. tokens equivalentes/s (`cycles/s × 128`);
//! 5. pico de heap e RSS do processo; alocações no hot-path.
//!
//! `harness = false`: a medição é um laço único com `Instant`, não criterion —
//! precisamos de percentis por ciclo e de contagem de alocação, que o harness
//! do criterion não expõe. A saída é `target/aether_bench_throughput.json`,
//! mesclada em `target/aether_scorecard.json` (se a suíte de auditoria já
//! rodou), além da tabela no terminal.
//!
//! Execução: `cargo bench --bench throughput_tps`

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use aether_engine::simulation::closed_loop_organism::{
    ClosedLoopOrganism, OrganismConfig, OrganismFrame,
};

// ---------------------------------------------------------------------------
// Alocador instrumentado: contagem no hot-path + pico de heap do processo
// ---------------------------------------------------------------------------

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static HOT_ALLOCS: Cell<usize> = const { Cell::new(0) };
}

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static TOTAL_ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);

fn track(delta: isize) {
    let now = if delta >= 0 {
        LIVE.fetch_add(delta as usize, Ordering::Relaxed) + delta as usize
    } else {
        LIVE.fetch_sub((-delta) as usize, Ordering::Relaxed) - (-delta) as usize
    };
    PEAK.fetch_max(now, Ordering::Relaxed);
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        track(layout.size() as isize);
        TOTAL_ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        if ARMED.with(Cell::get) {
            HOT_ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        track(layout.size() as isize);
        TOTAL_ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        if ARMED.with(Cell::get) {
            HOT_ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        track(new_size as isize - layout.size() as isize);
        TOTAL_ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        if ARMED.with(Cell::get) {
            HOT_ALLOCS.with(|c| c.set(c.get() + 1));
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        track(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

// ---------------------------------------------------------------------------
// RSS (Windows) — PeakWorkingSetSize de PROCESS_MEMORY_COUNTERS
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn peak_rss_mb() -> Option<f64> {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut c = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: `c` é um PROCESS_MEMORY_COUNTERS com `cb` preenchido e o handle
    // é o pseudo-handle do processo atual, sempre válido.
    let ok = unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb)
    };
    if ok != 0 {
        Some(c.peak_working_set_size as f64 / (1024.0 * 1024.0))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn peak_rss_mb() -> Option<f64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: f64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
            return Some(kb / 1024.0);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Medição
// ---------------------------------------------------------------------------

struct Regime {
    name: &'static str,
    steps: usize,
}

struct Stats {
    name: &'static str,
    cycles: usize,
    wall_s: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    p999_us: f64,
    cycles_per_s: f64,
    sop_per_s: f64,
    eq_t_per_s: f64,
    hot_allocs: usize,
    mem_peak_mb: f64,
}

fn percentile_us(sorted_ns: &[u64], q: f64) -> f64 {
    if sorted_ns.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_ns.len() - 1) as f64 * q).round() as usize;
    sorted_ns[idx.min(sorted_ns.len() - 1)] as f64 / 1000.0
}

fn run_regime(regime: &Regime, seconds: f64) -> Stats {
    let cfg = OrganismConfig {
        steps: regime.steps,
        perturb_after: Duration::from_secs(3600),
        ..Default::default()
    };
    let n = cfg.decision_dim;
    let steps = cfg.steps;
    let mut organism = ClosedLoopOrganism::new(cfg).expect("organismo");
    let mut frame = OrganismFrame::with_capacity(steps, n);

    // Warmup: planos FFT, buffers, threads Kuramoto e páginas.
    for _ in 0..32 {
        organism.step_into(&mut frame).expect("warmup");
    }

    let mut latencies: Vec<u64> = Vec::with_capacity(1 << 15);
    let mut hot_allocs = 0usize;
    let wall = Instant::now();

    while wall.elapsed().as_secs_f64() < seconds {
        HOT_ALLOCS.with(|c| c.set(0));
        ARMED.with(|a| a.set(true));
        let t0 = Instant::now();
        organism.step_into(&mut frame).expect("step");
        let dt = t0.elapsed();
        ARMED.with(|a| a.set(false));
        hot_allocs += HOT_ALLOCS.with(Cell::get);
        latencies.push(dt.as_nanos().min(u64::MAX as u128) as u64);
        black_box(frame.states[0]);
        black_box(frame.order_parameter);
    }

    let wall_s = wall.elapsed().as_secs_f64().max(1e-9);
    let cycles = latencies.len();
    latencies.sort_unstable();

    // Operações por ciclo: modulação espectral + FFT + relaxamento bSB.
    // c_in = 2*sensor_channels = 8, c_out = 8, modes = 32, L = window = 128.
    let (c_in, c_out, modes, l) = (8.0f64, 8.0f64, 32.0f64, 128.0f64);
    let spectral = 2.0 * c_in * c_out * modes + c_in * l * l.log2();
    let synaptic = 2.0 * (n as f64) * (n as f64) * (steps as f64);
    let sop_per_cycle = spectral + synaptic;

    let cycles_per_s = cycles as f64 / wall_s;
    let stats = Stats {
        name: regime.name,
        cycles,
        wall_s,
        p50_us: percentile_us(&latencies, 0.50),
        p95_us: percentile_us(&latencies, 0.95),
        p99_us: percentile_us(&latencies, 0.99),
        p999_us: percentile_us(&latencies, 0.999),
        cycles_per_s,
        sop_per_s: cycles_per_s * sop_per_cycle,
        eq_t_per_s: cycles_per_s * 128.0,
        hot_allocs,
        mem_peak_mb: PEAK.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
    };

    // Encerra as threads Kuramoto do organismo antes de retornar.
    organism.shutdown();
    stats
}

fn main() {
    let regimes = [
        Regime {
            name: "Basal",
            steps: 60,
        },
        Regime {
            name: "Vigilante",
            steps: 120,
        },
        Regime {
            name: "Critico",
            steps: 240,
        },
    ];

    println!("AETHER throughput: 5 s continuos por regime (60/120/240 passos)");
    println!(
        "{:<10} {:>8} {:>8} {:>8} {:>8} {:>9} {:>12} {:>12} {:>10}",
        "REGIME", "p50 us", "p95 us", "p99 us", "p99.9", "cycles/s", "SOP/s", "eq-t/s", "allocs"
    );
    println!("{}", "-".repeat(100));

    let mut stats = Vec::new();
    for regime in &regimes {
        let s = run_regime(regime, 5.0);
        println!(
            "{:<10} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>9.0} {:>12.3e} {:>12.3e} {:>10}",
            s.name,
            s.p50_us,
            s.p95_us,
            s.p99_us,
            s.p999_us,
            s.cycles_per_s,
            s.sop_per_s,
            s.eq_t_per_s,
            s.hot_allocs
        );
        stats.push(s);
    }

    let heap_peak_mb = PEAK.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0);
    let rss = peak_rss_mb();
    println!("{}", "-".repeat(100));
    println!(
        "heap pico (alocador) = {heap_peak_mb:.2} MB | RSS pico = {} | total alloc calls = {}",
        rss.map(|v| format!("{v:.2} MB"))
            .unwrap_or_else(|| "indisponivel".into()),
        TOTAL_ALLOC_CALLS.load(Ordering::Relaxed)
    );
    let hot: usize = stats.iter().map(|s| s.hot_allocs).sum();
    println!("alocacoes no hot-path (step_into) = {hot} (esperado 0)");

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"generated_by\": \"benches/throughput_tps.rs\",\n");
    json.push_str(&format!("\"peak_heap_mb\": {heap_peak_mb:.4},\n"));
    json.push_str(&format!(
        "\"peak_rss_mb\": {},\n",
        rss.map(|v| format!("{v:.4}"))
            .unwrap_or_else(|| "null".into())
    ));
    json.push_str(&format!("\"hot_path_allocations\": {hot},\n"));
    json.push_str("\"regimes\": [\n");
    for (i, s) in stats.iter().enumerate() {
        json.push_str(&format!(
            "    {{\"name\": \"{}\", \"steps\": {}, \"cycles\": {}, \"wall_s\": {:.4}, \
             \"p50_us\": {:.4}, \"p95_us\": {:.4}, \"p99_us\": {:.4}, \"p99_9_us\": {:.4}, \
             \"cycles_per_s\": {:.2}, \"sop_per_s\": {:.2}, \"eq_t_per_s\": {:.2}, \
             \"hot_path_allocations\": {}}}",
            s.name,
            regimes[i].steps,
            s.cycles,
            s.wall_s,
            s.p50_us,
            s.p95_us,
            s.p99_us,
            s.p999_us,
            s.cycles_per_s,
            s.sop_per_s,
            s.eq_t_per_s,
            s.hot_allocs
        ));
        if i + 1 < stats.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ]\n}");

    let _ = fs::create_dir_all("target");
    let _ = fs::write("target/aether_bench_throughput.json", &json);

    // Mescla no scorecard (válido se o objeto raiz for JSON).
    let path = "target/aether_scorecard.json";
    let merged = match fs::read_to_string(path) {
        Ok(text) => {
            let trimmed = text.trim_end();
            match trimmed.rfind('}') {
                Some(pos) if trimmed.starts_with('{') => {
                    let head = trimmed[..pos].trim_end();
                    if head.ends_with('{') {
                        format!("{head}\"throughput\": {json}\n}}")
                    } else {
                        format!("{head},\"throughput\": {json}\n}}")
                    }
                }
                _ => format!("{{\"throughput\": {json}}}"),
            }
        }
        Err(_) => format!("{{\"throughput\": {json}}}"),
    };
    let _ = fs::write(path, merged);

    // Gate de engenharia: o caminho quente do organismo já é documentado como
    // zero-alocação; qualquer alocação aqui é regressão, não meta aspiracional.
    assert_eq!(hot, 0, "hot-path alocou {hot} vezes");
    println!("OK: hot-path zero-alocacao; JSON em target/aether_bench_throughput.json");
}
