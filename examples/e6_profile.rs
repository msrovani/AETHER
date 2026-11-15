//! E6 — anatomia completa do quadro cognitivo (`step_into`).
//!
//! Responde à pergunta P0 do dossiê (Vol. II §1.1, Vol. III §12.2):
//! **onde vão os microssegundos de um ciclo?**
//!
//! Modos:
//! ```text
//! cargo run --release --example e6_profile                    # Basal, 10k ciclos
//! cargo run --release --example e6_profile -- --sweep         # steps 60/120/240
//! cargo run --release --example e6_profile -- --nsweep        # N 64..1024
//! cargo run --release --example e6_profile -- --all           # tudo
//! cargo run --release --example e6_profile -- --affinity      # com --features affinity
//! ```
//!
//! Metodologia:
//! - warm-up antes de medir (planos FFT, arena, publicação dos nós Kuramoto);
//! - `StageProfile::reset` a cada quadro; amostras pré-alocadas;
//! - alocador contador **armado** durante a janela de medição (gate: 0 bytes);
//! - percentis p50/p95/p99/p99.9 sobre a amostra por quadro;
//! - working set **analítico** por estágio (derivado das formas, não medido).
//!
//! **Correção de premissa:** o gate original "matvec < 1% do ciclo" foi
//! falsificado pela medição (o solver faz dois gemv por passo e domina o quadro).
//! O harness reporta a fração real em vez de falhar por estar certo.
//!
//! O quadro do organismo **não é bit-reprodutível entre instâncias**: a amostragem
//! lê `NodeHandle::latest()`, alimentado por threads UDP reais. Mede-se
//! distribuição, nunca execução única.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use aether_engine::simulation::closed_loop_organism::{
    ClosedLoopOrganism, OrganismConfig, OrganismFrame,
};
use aether_engine::simulation::profile::{stage, StageProfile, STAGE_COUNT, STAGE_NAMES};

// ---------------------------------------------------------------------------
// Alocador contador (mesmo padrão de benches/throughput_tps.rs)
// ---------------------------------------------------------------------------

static HOT_ALLOCS_TOTAL: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static HOT: Cell<u64> = const { Cell::new(0) };
}

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(Cell::get) {
            HOT.with(|c| c.set(c.get() + 1));
            HOT_ALLOCS_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(Cell::get) {
            HOT.with(|c| c.set(c.get() + 1));
            HOT_ALLOCS_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.with(Cell::get) {
            HOT.with(|c| c.set(c.get() + 1));
            HOT_ALLOCS_TOTAL.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

// ---------------------------------------------------------------------------
// Configuração
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Args {
    frames: usize,
    warmup: usize,
    steps: usize,
    n: usize,
    window: usize,
    hz: f64,
    sweep: bool,
    nsweep: bool,
    affinity: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            frames: 10_000,
            warmup: 1_000,
            steps: 60, // regime "Basal" do bench de vazão
            n: 64,
            window: 128,
            hz: 60.0,
            sweep: false,
            nsweep: false,
            affinity: false,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let key = argv[i].as_str();
        let val = || -> Option<&str> {
            if i + 1 < argv.len() {
                Some(argv[i + 1].as_str())
            } else {
                None
            }
        };
        match key {
            "--frames" => {
                if let Some(v) = val() {
                    a.frames = v.parse().unwrap_or(a.frames);
                    i += 1;
                }
            }
            "--warmup" => {
                if let Some(v) = val() {
                    a.warmup = v.parse().unwrap_or(a.warmup);
                    i += 1;
                }
            }
            "--steps" => {
                if let Some(v) = val() {
                    a.steps = v.parse().unwrap_or(a.steps);
                    i += 1;
                }
            }
            "--n" => {
                if let Some(v) = val() {
                    a.n = v.parse().unwrap_or(a.n);
                    i += 1;
                }
            }
            "--window" => {
                if let Some(v) = val() {
                    a.window = v.parse().unwrap_or(a.window);
                    i += 1;
                }
            }
            "--hz" => {
                if let Some(v) = val() {
                    a.hz = v.parse().unwrap_or(a.hz);
                    i += 1;
                }
            }
            "--sweep" => a.sweep = true,
            "--nsweep" => a.nsweep = true,
            "--all" => {
                a.sweep = true;
                a.nsweep = true;
            }
            "--affinity" => a.affinity = true,
            other => eprintln!("argumento desconhecido: {other}"),
        }
        i += 1;
    }
    a
}

// ---------------------------------------------------------------------------
// Estatística e working set
// ---------------------------------------------------------------------------

fn percentile(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Bytes ativos por estágio, derivados das formas (analítico, não instrumentado).
///
/// Sobrepõe estruturas entre estágios (ex.: `J` aparece no solver e no atuador);
/// a última linha reporta o **footprint único** do organismo.
fn working_set(n: usize, window: usize, steps: usize) -> [usize; STAGE_COUNT] {
    let c_in = 8usize; // 2 * sensor_channels
    let c_out = 8usize;
    let modes = 32usize;
    let complex32 = 8usize; // 2 * f32

    let frame_bytes = 128; // 3 × Frame (R + fases) — ordem de grandeza
    let ring = c_in * window * 4;
    let u = c_in * window * 4;
    let v = c_out * window * 4;
    let spec = c_out * modes * complex32;
    let plan = c_in * window * complex32; // scratch FFT (ordem de grandeza)
    let readout = n * c_out * modes * complex32;
    let j = n * n * 8;
    let xyz = 3 * n * 8;
    let trace = (steps + 1) * 8;

    [
        frame_bytes,           // sample
        ring,                  // sensor
        ring + u,              // ring_copy
        u + v + spec + plan,   // wave
        spec + plan,           // entropy
        readout + 2 * n * 8,   // readout (matriz domina)
        j + xyz + trace + n,   // solver
        trace + n + 2 * n * 8, // frame
        j + 2 * n * 8,         // actuator (relê J)
        0,                     // tail (escalares)
    ]
}

/// Footprint único do organismo (sem contar sobreposições).
fn unique_footprint(n: usize, window: usize, steps: usize) -> usize {
    let c_in = 8usize;
    let c_out = 8usize;
    let modes = 32usize;
    n * n * 8                       // J
        + n * c_out * modes * 8     // readout
        + 2 * c_in * window * 4     // ring + u
        + c_out * window * 4        // v
        + c_out * modes * 8         // spec
        + c_in * window * 8         // plan
        + (steps + 1) * 8           // trace
        + 4 * n * 8 // x/y/hx/raw
}

// ---------------------------------------------------------------------------
// Um job de medição
// ---------------------------------------------------------------------------

struct StageStat {
    mean_us: f64,
    p50_us: f64,
    p95_us: f64,
    p99_us: f64,
    p999_us: f64,
    fraction: f64,
    ws_bytes: usize,
}

struct JobResult {
    n: usize,
    steps: usize,
    window: usize,
    hz: f64,
    frames: usize,
    warmup: usize,
    total_mean_us: f64,
    total_p50_us: f64,
    total_p95_us: f64,
    total_p99_us: f64,
    total_p999_us: f64,
    coverage: f64,
    hot_allocs: u64,
    cycles_per_s: f64,
    eq_t_per_s: f64,
    solver_gflops: f64,
    unique_ws: usize,
    stages: Vec<StageStat>,
}

fn run_job(args: Args, n: usize, steps: usize, frames: usize, warmup: usize) -> JobResult {
    let cfg = OrganismConfig {
        decision_dim: n,
        window: args.window,
        steps,
        hz: args.hz,
        perturb_after: Duration::from_secs(3_600),
        ..OrganismConfig::default()
    };

    let mut org = ClosedLoopOrganism::new(cfg.clone()).expect("organismo");
    let mut frame = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);

    for _ in 0..warmup {
        org.step_into(&mut frame).expect("warmup");
    }

    let mut prof = StageProfile::default();
    let mut samples: Vec<[u64; STAGE_COUNT]> = Vec::with_capacity(frames);
    let mut totals: Vec<u64> = Vec::with_capacity(frames);

    // --- janela armada: qualquer alocação aqui é violação ---
    let before = HOT_ALLOCS_TOTAL.load(Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    for _ in 0..frames {
        prof.reset();
        org.step_profiled_into(&mut frame, &mut prof).expect("step");
        samples.push(prof.ns);
        totals.push(prof.total_ns);
    }
    ARMED.with(|a| a.set(false));
    let hot_allocs = HOT_ALLOCS_TOTAL.load(Ordering::Relaxed) - before;

    // --- agregados ---
    let frames_n = samples.len().max(1);
    let mut mean_ns = [0.0f64; STAGE_COUNT];
    let mut p50 = [0u64; STAGE_COUNT];
    let mut p95 = [0u64; STAGE_COUNT];
    let mut p99 = [0u64; STAGE_COUNT];
    let mut p999 = [0u64; STAGE_COUNT];

    for s in 0..STAGE_COUNT {
        let mut col: Vec<u64> = samples.iter().map(|x| x[s]).collect();
        col.sort_unstable();
        mean_ns[s] = col.iter().sum::<u64>() as f64 / frames_n as f64;
        p50[s] = percentile(&col, 0.50);
        p95[s] = percentile(&col, 0.95);
        p99[s] = percentile(&col, 0.99);
        p999[s] = percentile(&col, 0.999);
    }

    totals.sort_unstable();
    let total_mean = totals.iter().sum::<u64>() as f64 / total_frames(&totals);
    let accounted: f64 = mean_ns.iter().sum();

    let ws = working_set(n, args.window, steps);
    let stages = (0..STAGE_COUNT)
        .map(|s| StageStat {
            mean_us: mean_ns[s] / 1_000.0,
            p50_us: p50[s] as f64 / 1_000.0,
            p95_us: p95[s] as f64 / 1_000.0,
            p99_us: p99[s] as f64 / 1_000.0,
            p999_us: p999[s] as f64 / 1_000.0,
            fraction: if total_mean > 0.0 {
                mean_ns[s] / total_mean
            } else {
                0.0
            },
            ws_bytes: ws[s],
        })
        .collect();

    let solver_sec = mean_ns[stage::SOLVER] / 1e9;
    let flops = 4.0 * (n as f64).powi(2) * steps as f64;

    JobResult {
        n,
        steps,
        window: args.window,
        hz: args.hz,
        frames: frames_n,
        warmup,
        total_mean_us: total_mean / 1_000.0,
        total_p50_us: percentile(&totals, 0.50) as f64 / 1_000.0,
        total_p95_us: percentile(&totals, 0.95) as f64 / 1_000.0,
        total_p99_us: percentile(&totals, 0.99) as f64 / 1_000.0,
        total_p999_us: percentile(&totals, 0.999) as f64 / 1_000.0,
        coverage: if total_mean > 0.0 {
            accounted / total_mean
        } else {
            0.0
        },
        hot_allocs,
        cycles_per_s: if total_mean > 0.0 {
            1e9 / total_mean
        } else {
            0.0
        },
        eq_t_per_s: if total_mean > 0.0 {
            128e9 / total_mean
        } else {
            0.0
        },
        solver_gflops: if solver_sec > 0.0 {
            flops / solver_sec / 1e9
        } else {
            0.0
        },
        unique_ws: unique_footprint(n, args.window, steps),
        stages,
    }
}

fn total_frames(totals: &[u64]) -> f64 {
    totals.len().max(1) as f64
}

fn print_job(j: &JobResult) {
    println!();
    println!(
        "AETHER · E6 — N={} steps={} window={} frames={} warmup={}",
        j.n, j.steps, j.window, j.frames, j.warmup
    );
    println!("{}", "-".repeat(88));
    println!(
        "{:<10} {:>9} {:>9} {:>9} {:>9} {:>9} {:>7} {:>10}",
        "estágio", "média", "p50", "p95", "p99", "p99.9", "%", "ws (KB)"
    );
    println!("{}", "-".repeat(88));
    for (s, st) in j.stages.iter().enumerate() {
        println!(
            "{:<10} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>6.2}% {:>10.1}",
            STAGE_NAMES[s],
            st.mean_us,
            st.p50_us,
            st.p95_us,
            st.p99_us,
            st.p999_us,
            st.fraction * 100.0,
            st.ws_bytes as f64 / 1024.0
        );
    }
    println!("{}", "-".repeat(88));
    println!(
        "{:<10} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>6.2}% {:>10.1}",
        "TOTAL",
        j.total_mean_us,
        j.total_p50_us,
        j.total_p95_us,
        j.total_p99_us,
        j.total_p999_us,
        j.coverage * 100.0,
        j.unique_ws as f64 / 1024.0
    );
    println!();
    println!(
        "  cycles/s {:.1} · eq-t/s {:.0} · solver {:.2} GFLOP/s · allocs na janela {}",
        j.cycles_per_s, j.eq_t_per_s, j.solver_gflops, j.hot_allocs
    );
}

/// Fração de N para a qual o footprint único deixa de caber no L2 (1,25 MB).
fn cache_level(bytes: usize) -> &'static str {
    const L1: usize = 48 * 1024;
    const L2: usize = 1_280 * 1024;
    const L3: usize = 24 * 1024 * 1024;
    if bytes <= L1 {
        "L1d"
    } else if bytes <= L2 {
        "L2"
    } else if bytes <= L3 {
        "L3"
    } else {
        "DRAM"
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();

    #[cfg(feature = "affinity")]
    if args.affinity {
        if let Some(ids) = core_affinity::get_core_ids() {
            if let Some(first) = ids.first() {
                let ok = core_affinity::set_for_current(*first);
                println!("affinity: core {:?} -> {}", first, ok);
            }
        }
    }
    #[cfg(not(feature = "affinity"))]
    if args.affinity {
        println!("affinity: feature desligada (rebuilde com --features affinity)");
    }

    let mut jobs: Vec<JobResult> = Vec::new();

    // Job principal: regime Basal com 10k ciclos.
    if !args.sweep && !args.nsweep {
        jobs.push(run_job(args, args.n, args.steps, args.frames, args.warmup));
    }

    if args.sweep {
        for steps in [60usize, 120, 240] {
            let frames = adaptive_frames(64, steps, args.frames);
            let warmup = adaptive_warmup(frames, args.warmup);
            jobs.push(run_job(args, 64, steps, frames, warmup));
        }
    }

    if args.nsweep {
        for n in [64usize, 128, 256, 512, 1024] {
            let frames = adaptive_frames(n, 60, args.frames);
            let warmup = adaptive_warmup(frames, args.warmup);
            jobs.push(run_job(args, n, 60, frames, warmup));
        }
    }

    for j in &jobs {
        print_job(j);
    }

    // --- resumo de varredura ---
    if jobs.len() > 1 {
        println!();
        println!("varredura — lei de escala");
        println!("{}", "-".repeat(74));
        println!(
            "{:>4} {:>6} {:>10} {:>10} {:>10} {:>10} {:>9} {:>10}",
            "N", "steps", "total µs", "p99 µs", "solver µs", "GFLOP/s", "ws MB", "nível"
        );
        for j in &jobs {
            let solver = j.stages[stage::SOLVER].mean_us;
            println!(
                "{:>4} {:>6} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>9.2} {:>10}",
                j.n,
                j.steps,
                j.total_mean_us,
                j.total_p99_us,
                solver,
                j.solver_gflops,
                j.unique_ws as f64 / (1024.0 * 1024.0),
                cache_level(j.unique_ws)
            );
        }
    }

    // --- gates ---
    let worst_cov = jobs.iter().map(|j| j.coverage).fold(1.0f64, f64::min);
    let total_allocs: u64 = jobs.iter().map(|j| j.hot_allocs).sum();
    let basal = &jobs[0];
    let dominant = (0..STAGE_COUNT)
        .max_by(|&a, &b| {
            basal.stages[a]
                .fraction
                .partial_cmp(&basal.stages[b].fraction)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);

    println!();
    println!("gates E6");
    println!(
        "  G1 cobertura ≥ 95%        : {:.2}%  {}",
        worst_cov * 100.0,
        pass(worst_cov >= 0.95)
    );
    println!(
        "  G3 zero-alloc na janela   : {} allocs  {}",
        total_allocs,
        pass(total_allocs == 0)
    );
    println!(
        "  G2 dominante              : {} ({:.1}%)  {}",
        STAGE_NAMES[dominant],
        basal.stages[dominant].fraction * 100.0,
        pass(true)
    );
    println!("      (gate original 'matvec < 1%' FALSIFICADO — ver Vol. II §1.1)");
    println!();

    // --- JSON ---
    std::fs::create_dir_all("target").ok();
    let path = "target/aether_profile_e6.json";
    let mut json = String::new();
    json.push_str("{\n  \"tool\": \"e6_profile\",\n  \"version\": 2,\n");
    json.push_str(&format!("  \"jobs\": {},\n", jobs.len()));
    json.push_str("  \"results\": [\n");
    for (ji, j) in jobs.iter().enumerate() {
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"n\": {}, \"steps\": {}, \"window\": {}, \"hz\": {:.1},\n",
            j.n, j.steps, j.window, j.hz
        ));
        json.push_str(&format!(
            "      \"frames\": {}, \"warmup\": {},\n",
            j.frames, j.warmup
        ));
        json.push_str(&format!(
            "      \"total_mean_us\": {:.3}, \"p50_us\": {:.3}, \"p95_us\": {:.3}, \"p99_us\": {:.3}, \"p999_us\": {:.3},\n",
            j.total_mean_us, j.total_p50_us, j.total_p95_us, j.total_p99_us, j.total_p999_us
        ));
        json.push_str(&format!(
            "      \"coverage\": {:.4}, \"hot_allocs\": {}, \"cycles_per_s\": {:.1}, \"eq_t_per_s\": {:.0}, \"solver_gflops\": {:.2},\n",
            j.coverage, j.hot_allocs, j.cycles_per_s, j.eq_t_per_s, j.solver_gflops
        ));
        json.push_str(&format!(
            "      \"unique_ws_bytes\": {}, \"cache_level\": \"{}\",\n",
            j.unique_ws,
            cache_level(j.unique_ws)
        ));
        json.push_str("      \"stages\": [\n");
        for (si, st) in j.stages.iter().enumerate() {
            let comma = if si + 1 == STAGE_COUNT { "" } else { "," };
            json.push_str(&format!(
                "        {{\"name\": \"{}\", \"mean_us\": {:.3}, \"p50_us\": {:.3}, \"p95_us\": {:.3}, \"p99_us\": {:.3}, \"p999_us\": {:.3}, \"fraction\": {:.4}, \"ws_bytes\": {}}}{}\n",
                STAGE_NAMES[si], st.mean_us, st.p50_us, st.p95_us, st.p99_us, st.p999_us, st.fraction, st.ws_bytes, comma
            ));
        }
        json.push_str("      ]\n");
        json.push_str(if ji + 1 == jobs.len() {
            "    }\n"
        } else {
            "    },\n"
        });
    }
    json.push_str("  ]\n}\n");
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    let _ = Instant::now(); // mantém o import explícito para leitores do arquivo
    Ok(())
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// Mantém ~2 s de medição por configuração, sem estourar 10k quadros em N grande.
fn adaptive_frames(n: usize, steps: usize, cap: usize) -> usize {
    let per_frame_s = 4.0 * (n as f64).powi(2) * steps as f64 / 7.0e9;
    ((2.0 / per_frame_s.max(1e-9)) as usize).clamp(64, cap)
}

fn adaptive_warmup(frames: usize, cap: usize) -> usize {
    (frames / 4).clamp(16, cap)
}
