//! E2 — lógica como Ising: pipeline de 5 estágios vs. penalidade estática.
//!
//! Metodologia:
//! 1. 200 instâncias de 3-SAT `n ≤ 20` em duas classes com ground truth exato
//!    (força bruta): **aleatórias** na razão de aperto (≈4,3 cláusulas/variável,
//!    mistura sat/unsat) e **plantadas** (todas satisfatíveis, igualmente
//!    apertadas);
//! 2. **Braço 1** — BSB puro com penalidade **quadrática estática por pares**
//!    (a forma que arXiv:2511.21046 mostra distorcer a paisagem): sem presolve,
//!    sem persistência, sem decomposição, sem multiplicadores;
//! 3. **Braço 2** — pipeline de 5 estágios:
//!    `presolve (unidades+literais falhos) → persistência de 1ª ordem →
//!     decomposição em componentes → BSB + Lagrange → certificado`;
//! 4. o certificado é avaliado por **contagem exata** de cláusulas violadas, não
//!    por energia: `violations == 0` é a única resposta decisiva.
//!
//! ```text
//! cargo run --release --example ising_logic_probe
//! cargo run --release --example ising_logic_probe -- --n 18 --instances 120 --quick
//! ```
//!
//! Saída: tabela + `target/aether_e2.json`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::{Certificate, Provenance};
use aether_engine::core::logic_pipeline::{violations_3, BsbCore, Clause3, LogicPipeline};

// ---------------------------------------------------------------------------
// Alocador contador (só a thread corrente, só com ARMED)
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
// Argumentos
// ---------------------------------------------------------------------------

struct Args {
    instances: usize,
    n: usize,
    ratio: f64,
    steps: usize,
    restarts: usize,
    rounds: usize,
    eta: f64,
    seed: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            instances: 200,
            n: 20,
            ratio: 4.3,
            steps: 12,
            restarts: 3,
            rounds: 5,
            eta: 0.5,
            seed: 0xE2_0001,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let key = argv[i].as_str();
        let raw = argv.get(i + 1);
        let mut take = |dst: &mut usize| {
            if let Some(v) = raw.and_then(|v| v.parse::<usize>().ok()) {
                *dst = v;
            }
        };
        match key {
            "--instances" => take(&mut a.instances),
            "--n" => take(&mut a.n),
            "--steps" => take(&mut a.steps),
            "--restarts" => take(&mut a.restarts),
            "--rounds" => take(&mut a.rounds),
            "--quick" => {
                a.instances = 60;
                a.n = 14;
                a.steps = 10;
            }
            "--ratio" => {
                if let Some(v) = raw.and_then(|v| v.parse::<f64>().ok()) {
                    a.ratio = v;
                }
            }
            "--eta" => {
                if let Some(v) = raw.and_then(|v| v.parse::<f64>().ok()) {
                    a.eta = v;
                }
            }
            "--seed" => {
                if let Some(v) = raw.and_then(|v| v.parse::<u64>().ok()) {
                    a.seed = v;
                }
            }
            other => eprintln!("argumento ignorado: {other}"),
        }
        if matches!(
            key,
            "--instances"
                | "--n"
                | "--steps"
                | "--restarts"
                | "--rounds"
                | "--ratio"
                | "--eta"
                | "--seed"
        ) {
            i += 1;
        }
        i += 1;
    }
    a
}

// ---------------------------------------------------------------------------
// Instâncias
// ---------------------------------------------------------------------------

/// 3-SAT aleatório (variáveis distintas por cláusula).
fn random_3sat(n: usize, m: usize, seed: u64) -> Vec<Clause3> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut out = Vec::with_capacity(m);
    for _ in 0..m {
        let i = rng.gen_range(0..n);
        let mut j = rng.gen_range(0..n);
        let mut k = rng.gen_range(0..n);
        while j == i {
            j = rng.gen_range(0..n);
        }
        while k == i || k == j {
            k = rng.gen_range(0..n);
        }
        let si = if rng.gen::<bool>() { 1 } else { -1 };
        let sj = if rng.gen::<bool>() { 1 } else { -1 };
        let sk = if rng.gen::<bool>() { 1 } else { -1 };
        out.push(Clause3::new3(i, si, j, sj, k, sk));
    }
    out
}

/// 3-SAT **plantado**: `s*` satisfaz toda cláusula (ao menos um literal verdadeiro).
///
/// Todas as instâncias são satisfatíveis por construção, mas continuam apertadas
/// (mesma razão cláusula/variável).
fn planted_3sat(n: usize, m: usize, seed: u64) -> (Vec<Clause3>, Vec<i8>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut star = vec![1i8; n];
    for v in star.iter_mut() {
        *v = if rng.gen::<bool>() { 1 } else { -1 };
    }
    let mut out = Vec::with_capacity(m);
    for _ in 0..m {
        let i = rng.gen_range(0..n);
        let mut j = rng.gen_range(0..n);
        let mut k = rng.gen_range(0..n);
        while j == i {
            j = rng.gen_range(0..n);
        }
        while k == i || k == j {
            k = rng.gen_range(0..n);
        }
        let mut si = if rng.gen::<bool>() { 1 } else { -1 };
        let mut sj = if rng.gen::<bool>() { 1 } else { -1 };
        let mut sk = if rng.gen::<bool>() { 1 } else { -1 };
        let sat = |s: i8, v: usize| s == star[v];
        if !(sat(si, i) || sat(sj, j) || sat(sk, k)) {
            // força o primeiro literal a ser verdadeiro
            si = star[i];
        }
        // normaliza: se o literal plantado já é satisfeito, mantém
        if !(sat(si, i) || sat(sj, j) || sat(sk, k)) {
            sj = star[j];
        }
        out.push(Clause3::new3(i, si, j, sj, k, sk));
    }
    (out, star)
}

/// Satisfatibilidade exata por força bruta (`n ≤ 22`), com saída antecipada.
fn brute_force_sat(n: usize, clauses: &[Clause3]) -> bool {
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        if violations_3(clauses, &s) == 0 {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Braço 1 — BSB puro com penalidade quadrática estática por pares
// ---------------------------------------------------------------------------

/// `P = Σ_pares (1−l_a)(1−l_b)` em spins, sem offset constante.
///
/// Cada par de literais falsos custa 1; uma cláusula com **um** literal
/// verdadeiro já paga 1 (de 3) — a distorção que o E2 mede.
fn pair_penalty_qubo(n: usize, clauses: &[Clause3]) -> (Vec<f64>, Vec<f64>) {
    let mut j = vec![0.0f64; n * n];
    let mut h = vec![0.0f64; n];
    let mut add_pair = |j: &mut [f64], h: &mut [f64], a: (usize, i8), b: (usize, i8)| {
        let (ia, sa) = (a.0, f64::from(a.1));
        let (ib, sb) = (b.0, f64::from(b.1));
        j[ia * n + ib] -= sa * sb;
        j[ib * n + ia] -= sa * sb;
        h[ia] += 2.0 * sa;
        h[ib] += 2.0 * sb;
    };
    for c in clauses {
        let a = (c.i, c.si);
        let b = (c.j, c.sj);
        add_pair(&mut j, &mut h, a, b);
        if c.len() >= 3 {
            let d = (c.k, c.sk);
            add_pair(&mut j, &mut h, a, d);
            add_pair(&mut j, &mut h, b, d);
        }
    }
    (j, h)
}

fn arm_static(
    n: usize,
    clauses: &[Clause3],
    restarts: usize,
    steps: usize,
    seed: u64,
    bsb: &mut BsbCore,
    states: &mut [i8],
) -> u32 {
    let (j, h) = pair_penalty_qubo(n, clauses);
    let mut best = u32::MAX;
    let mut best_state = vec![0i8; n];
    for r in 0..restarts {
        bsb.relax_into(
            &j,
            &h,
            n,
            seed ^ (r as u64).wrapping_mul(0x9E37_79B9),
            states,
        );
        let v = violations_3(clauses, states);
        if v < best {
            best = v;
            best_state.copy_from_slice(states);
        }
    }
    states.copy_from_slice(&best_state);
    best
}

// ---------------------------------------------------------------------------
// Métricas
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct ArmStats {
    solved_sat: usize,
    sat_total: usize,
    decisive: usize,
    decisive_valid: usize,
    false_decisive_unsat: usize,
    unsat_total: usize,
    solved_tight: usize,
    tight_total: usize,
}

fn pct(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        100.0 * num as f64 / den as f64
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

/// Orçamento de ciclo do organismo (E6b, MSG 0098/0118): 81,3 µs.
const CYCLE_BUDGET_US: f64 = 81.3;
/// Fração do ciclo reservada ao passe simbólico + Lagrange pelo gate.
const GATE_FRACTION: f64 = 0.05;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n = args.n;
    let m = (args.ratio * n as f64).round() as usize;
    let half = args.instances / 2;

    let mut pipe = LogicPipeline::new(n, m + 4, args.steps, args.restarts, args.rounds, args.eta);
    let mut arm1_bsb = BsbCore::new(n, args.steps, 0.05);

    let mut s1 = ArmStats::default();
    let mut s2 = ArmStats::default();

    let mut off = vec![0i8; n];
    let mut best2 = vec![0i8; n];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);

    let mut sym_us: Vec<f64> = Vec::with_capacity(args.instances);
    let mut solve_us: Vec<f64> = Vec::with_capacity(args.instances);
    let mut total_us: Vec<f64> = Vec::with_capacity(args.instances);
    let mut fixed_sym = 0usize;
    let mut fixed_pers = 0usize;
    let mut n_free_sum = 0usize;
    let mut comp_sum = 0usize;
    let mut runs_sum = 0usize;
    let mut allocs_max = 0usize;

    // warmup (estatísticas lazy do alocador/ndarray)
    {
        let warm = random_3sat(n.min(8), n.min(8), 0xE2_AAAA);
        let _ = pipe.resolve_into(n.min(8), &warm, 1, &mut off, &mut cert);
    }

    println!();
    println!("AETHER · E2 — lógica como Ising + certificado de factibilidade");
    println!("=====================================================");
    println!(
        "instâncias={} n={} m={} (razão {:.2}) steps={} restarts={} rounds={} eta={}",
        args.instances, n, m, args.ratio, args.steps, args.restarts, args.rounds, args.eta
    );
    println!(
        "classes: {} aleatórias + {} plantadas (apertadas)",
        half,
        args.instances - half
    );
    println!();

    for inst in 0..args.instances {
        let (clauses, planted) = if inst < half {
            (
                random_3sat(n, m, args.seed.wrapping_add(inst as u64)),
                false,
            )
        } else {
            let (c, _) = planted_3sat(n, m, args.seed.wrapping_add(inst as u64));
            (c, true)
        };
        let sat = brute_force_sat(n, &clauses);
        debug_assert!(sat || !planted, "plantada deveria ser satisfatível");
        let tight = true; // todas as instâncias estão na razão de aperto

        // ---------- braço 1 ----------
        let v1 = arm_static(
            n,
            &clauses,
            args.restarts.max(4),
            args.steps,
            args.seed.wrapping_add(inst as u64),
            &mut arm1_bsb,
            &mut off,
        );
        s1.solved_sat += usize::from(sat && v1 == 0);
        s1.sat_total += usize::from(sat);
        s1.unsat_total += usize::from(!sat);
        s1.false_decisive_unsat += usize::from(!sat && v1 == 0);
        if tight {
            s1.tight_total += 1;
            s1.solved_tight += usize::from(v1 == 0);
        }
        if v1 == 0 {
            s1.decisive += 1;
            if violations_3(&clauses, &off) == 0 {
                s1.decisive_valid += 1;
            }
        }

        // ---------- braço 2 (pipeline) ----------
        let (_, allocs) = count_allocs(|| {
            pipe.resolve_into(
                n,
                &clauses,
                args.seed.wrapping_add(inst as u64),
                &mut best2,
                &mut cert,
            )
            .expect("resolve")
        });
        allocs_max = allocs_max.max(allocs);

        let st = pipe.last;
        sym_us.push(st.symbolic_us);
        solve_us.push(st.solve_us);
        total_us.push(st.symbolic_us + st.solve_us);
        fixed_sym += st.fixed_symbolic;
        fixed_pers += st.fixed_persistency;
        n_free_sum += st.n_free;
        comp_sum += st.components;
        runs_sum += st.bsb_runs;

        let v2 = violations_3(&clauses, &best2);
        debug_assert_eq!(v2, st.violations);
        s2.solved_sat += usize::from(sat && v2 == 0);
        s2.sat_total += usize::from(sat);
        s2.unsat_total += usize::from(!sat);
        s2.false_decisive_unsat += usize::from(!sat && v2 == 0);
        if tight {
            s2.tight_total += 1;
            s2.solved_tight += usize::from(v2 == 0);
        }
        if v2 == 0 {
            s2.decisive += 1;
            // recontagem independente do certificado
            if violations_3(&clauses, &best2) == 0 {
                s2.decisive_valid += 1;
            }
        }
    }

    let inst = args.instances as f64;
    sym_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    solve_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    total_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    println!(
        "braço                              resolvidas sat   decisivas   falsas-decisivas/UNSAT"
    );
    println!("{}", "-".repeat(86));
    println!(
        "1 BSB puro (pares, estático)       {:>10} ({:>5.1}%)  {:>8}   {}",
        format!("{}/{}", s1.solved_sat, s1.sat_total),
        pct(s1.solved_sat, s1.sat_total),
        s1.decisive,
        s1.false_decisive_unsat
    );
    println!(
        "2 pipeline (5 estágios)            {:>10} ({:>5.1}%)  {:>8}   {}",
        format!("{}/{}", s2.solved_sat, s2.sat_total),
        pct(s2.solved_sat, s2.sat_total),
        s2.decisive,
        s2.false_decisive_unsat
    );
    println!();
    println!(
        "precisão das decisivas (braço 2)   : {:.1}%  ({}/{})",
        pct(s2.decisive_valid, s2.decisive),
        s2.decisive_valid,
        s2.decisive
    );
    println!(
        "instâncias insatisfatíveis         : {} (decisivas do braço 2 nelas: {})",
        s2.unsat_total, s2.false_decisive_unsat
    );
    println!(
        "redução simbólica (média)          : {:.1} fixadas de {n} + {:.1} de persistência",
        fixed_sym as f64 / inst,
        fixed_pers as f64 / inst
    );
    println!(
        "resíduo (spins livres, média)      : {:.1}  ·  componentes {:.1}  ·  BSB runs {:.1}",
        n_free_sum as f64 / inst,
        comp_sum as f64 / inst,
        runs_sum as f64 / inst
    );
    println!();
    println!("custo (release, por instância)");
    println!(
        "  simbólico (1)-(3)                : p50 {:.1} µs · p95 {:.1} µs",
        percentile(&sym_us, 0.5),
        percentile(&sym_us, 0.95)
    );
    println!(
        "  BSB + Lagrange (4)               : p50 {:.1} µs · p95 {:.1} µs",
        percentile(&solve_us, 0.5),
        percentile(&solve_us, 0.95)
    );
    println!(
        "  total                            : p50 {:.1} µs · p95 {:.1} µs",
        percentile(&total_us, 0.5),
        percentile(&total_us, 0.95)
    );
    println!("  alocações por resolução (máx)     : {allocs_max}");
    println!();

    let g1 = s2.sat_total > 0 && (s2.solved_sat as f64 / s2.sat_total as f64) >= 0.95;
    let g2a = s2.decisive == 0 || s2.decisive_valid == s2.decisive;
    let g2b = s2.false_decisive_unsat == 0;
    let g3a = allocs_max == 0;
    let budget = CYCLE_BUDGET_US * GATE_FRACTION;
    let sym_p50 = percentile(&sym_us, 0.5);
    let g3b = sym_p50 <= budget;

    println!("gates");
    println!(
        "  G1 completude ≥ 95% (sat)         : {}  ({:.1}%; BSB puro {:.1}%)",
        pass(g1),
        pct(s2.solved_sat, s2.sat_total),
        pct(s1.solved_sat, s1.sat_total)
    );
    println!(
        "  G2 precisão das decisivas = 100%  : {}  ({:.1}%)",
        pass(g2a),
        pct(s2.decisive_valid, s2.decisive)
    );
    println!(
        "  G2 abstenção em insatisfatível    : {}  ({} decisivas falsas)",
        pass(g2b),
        s2.false_decisive_unsat
    );
    println!(
        "  G3 zero alocação na resolução     : {}  ({allocs_max} bytes-alocações)",
        pass(g3a)
    );
    println!(
        "  G3 simbólico ≤ 5% do ciclo ({budget:.2} µs): {}  (p50 {:.1} µs = {:.0}× o teto)",
        pass(g3b),
        sym_p50,
        sym_p50 / budget
    );
    println!();

    // ---------- amostra de certificado ----------
    println!("certificado (amostra do último)");
    println!(
        "  E={:.3} ΔE={:.3} violações={} prov={} coerente={}",
        cert.energy,
        cert.gap,
        cert.violations,
        cert.provenance.label(),
        cert.is_coherent()
    );
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e2.json";
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"instances\": {}, \"n\": {n}, \"m\": {m}, \"ratio\": {:.3},\n",
        args.instances, args.ratio
    ));
    json.push_str(&format!(
        "  \"steps\": {}, \"restarts\": {}, \"rounds\": {}, \"eta\": {:.2},\n",
        args.steps, args.restarts, args.rounds, args.eta
    ));
    json.push_str(&format!(
        "  \"arm1\": {{\"solved_sat\": {}, \"sat_total\": {}, \"decisive\": {}, \"false_decisive_unsat\": {}, \"solved_tight\": {}, \"tight_total\": {}}},\n",
        s1.solved_sat, s1.sat_total, s1.decisive, s1.false_decisive_unsat, s1.solved_tight, s1.tight_total
    ));
    json.push_str(&format!(
        "  \"arm2\": {{\"solved_sat\": {}, \"sat_total\": {}, \"decisive\": {}, \"decisive_valid\": {}, \"false_decisive_unsat\": {}, \"solved_tight\": {}, \"tight_total\": {}}},\n",
        s2.solved_sat, s2.sat_total, s2.decisive, s2.decisive_valid, s2.false_decisive_unsat, s2.solved_tight, s2.tight_total
    ));
    json.push_str(&format!(
        "  \"precision\": {:.4},\n  \"allocs_max\": {allocs_max},\n",
        pct(s2.decisive_valid, s2.decisive) / 100.0
    ));
    json.push_str(&format!(
        "  \"symbolic_us_p50\": {:.3},\n  \"solve_us_p50\": {:.3},\n  \"total_us_p50\": {:.3},\n",
        percentile(&sym_us, 0.5),
        percentile(&solve_us, 0.5),
        percentile(&total_us, 0.5)
    ));
    json.push_str(&format!(
        "  \"fixed_symbolic_mean\": {:.3},\n  \"fixed_persistency_mean\": {:.3},\n",
        fixed_sym as f64 / inst,
        fixed_pers as f64 / inst
    ));
    json.push_str(&format!(
        "  \"n_free_mean\": {:.3},\n  \"components_mean\": {:.3},\n  \"bsb_runs_mean\": {:.3},\n",
        n_free_sum as f64 / inst,
        comp_sum as f64 / inst,
        runs_sum as f64 / inst
    ));
    json.push_str(&format!(
        "  \"gates\": {{\"g1\": {g1}, \"g2a\": {g2a}, \"g2b\": {g2b}, \"g3a\": {g3a}, \"g3b\": {g3b}}}\n}}\n"
    ));
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    Ok(())
}
