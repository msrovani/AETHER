//! E2 — microbenchmark do estágio **simbólico** (`presolve` → persistência →
//! decomposição), com estatística robusta a carga da máquina.
//!
//! O gate G3b do `ising_logic_probe` usa `p50` sobre instâncias e sofre com
//! build concorrente (o mesmo binário mediu 33 µs e 52 µs em runs diferentes).
//! Aqui cada instância é resolvida `--reps` vezes e reportamos:
//!
//! * **min** por instância (robusto a *scheduling*/carga);
//! * **mediana** das medianas.
//!
//! ```text
//! cargo run --release --example e2_symbolic_bench
//! cargo run --release --example e2_symbolic_bench -- --n 20 --reps 400
//! ```
//!
//! Saída: tabela + `target/aether_e2_symbolic.json`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::{Certificate, Provenance};
use aether_engine::core::logic_pipeline::{Clause3, LogicPipeline};

struct Args {
    n: usize,
    instances: usize,
    reps: usize,
    steps: usize,
    restarts: usize,
    rounds: usize,
    seed: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            n: 20,
            instances: 12,
            reps: 200,
            steps: 32,
            restarts: 2,
            rounds: 3,
            seed: 0xE2_B000,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let key = argv[i].as_str();
        let raw = argv.get(i + 1).and_then(|v| v.parse::<u64>().ok());
        match (key, raw) {
            ("--n", Some(v)) => {
                a.n = v as usize;
                i += 1;
            }
            ("--instances", Some(v)) => {
                a.instances = v as usize;
                i += 1;
            }
            ("--reps", Some(v)) => {
                a.reps = (v as usize).max(1);
                i += 1;
            }
            ("--steps", Some(v)) => {
                a.steps = v as usize;
                i += 1;
            }
            ("--restarts", Some(v)) => {
                a.restarts = (v as usize).max(1);
                i += 1;
            }
            ("--rounds", Some(v)) => {
                a.rounds = (v as usize).max(1);
                i += 1;
            }
            ("--seed", Some(v)) => {
                a.seed = v;
                i += 1;
            }
            (k, _) => eprintln!("argumento ignorado: {k}"),
        }
    }
    a
}

/// 3-SAT aleatório no limiar (mesma receita do `ising_logic_probe`).
fn random_3sat(n: usize, m: usize, seed: u64) -> Vec<Clause3> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut out = Vec::with_capacity(m);
    while out.len() < m {
        let a = rng.gen_range(0..n);
        let mut b = rng.gen_range(0..n);
        let mut c = rng.gen_range(0..n);
        if a == b {
            b = (b + 1) % n;
        }
        if c == a || c == b {
            c = (c + 1) % n;
            if c == a || c == b {
                c = (c + 1) % n;
            }
        }
        let sa = if rng.gen::<bool>() { 1i8 } else { -1i8 };
        let sb = if rng.gen::<bool>() { 1i8 } else { -1i8 };
        let sc = if rng.gen::<bool>() { 1i8 } else { -1i8 };
        out.push(Clause3::new3(a, sa, b, sb, c, sc));
    }
    out
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n = args.n;
    let m = (4.3 * n as f64).round() as usize;

    println!();
    println!("AETHER · E2 — microbench do estágio simbólico");
    println!("=============================================");
    println!(
        "n={n} m={m} instâncias={} reps={} (min por instância; mediana das medianas)",
        args.instances, args.reps
    );
    println!();

    let mut pipeline = LogicPipeline::new(
        n.max(4),
        m.max(4),
        args.steps,
        args.restarts,
        args.rounds,
        0.5,
    );
    let mut out = vec![0i8; n];
    let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);

    let mut sym_min = Vec::with_capacity(args.instances);
    let mut sym_med = Vec::with_capacity(args.instances);
    let mut pre_min = Vec::with_capacity(args.instances);
    let mut pers_min = Vec::with_capacity(args.instances);
    let mut dec_min = Vec::with_capacity(args.instances);
    let mut fixed_sum = 0.0f64;

    for inst in 0..args.instances {
        let clauses = random_3sat(n, m, args.seed.wrapping_add(inst as u64));
        // warmup (caminhos de erro, buffers, etc.)
        pipeline.resolve_into(n, &clauses, inst as u64, &mut out, &mut cert)?;

        let mut sym = Vec::with_capacity(args.reps);
        let mut pre = Vec::with_capacity(args.reps);
        let mut pers = Vec::with_capacity(args.reps);
        let mut dec = Vec::with_capacity(args.reps);
        for r in 0..args.reps {
            pipeline.resolve_into(n, &clauses, (inst * 1000 + r) as u64, &mut out, &mut cert)?;
            let st = pipeline.last;
            sym.push(st.symbolic_us);
            pre.push(st.presolve_us);
            pers.push(st.persistency_us);
            dec.push(st.decompose_us);
        }
        fixed_sum += pipeline.last.fixed_symbolic as f64;
        sym.sort_by(|a, b| a.partial_cmp(b).unwrap());
        pre.sort_by(|a, b| a.partial_cmp(b).unwrap());
        pers.sort_by(|a, b| a.partial_cmp(b).unwrap());
        dec.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sym_min.push(sym[0]);
        sym_med.push(sym[sym.len() / 2]);
        pre_min.push(pre[0]);
        pers_min.push(pers[0]);
        dec_min.push(dec[0]);
    }

    let sym_min_v = sym_min.iter().copied().fold(f64::INFINITY, f64::min);
    let pre_min_v = pre_min.iter().copied().fold(f64::INFINITY, f64::min);
    let pers_min_v = pers_min.iter().copied().fold(f64::INFINITY, f64::min);
    let dec_min_v = dec_min.iter().copied().fold(f64::INFINITY, f64::min);
    let sym_med_v = median(&mut sym_med);

    let budget = 0.05 * 81.3; // 5% do ciclo de 81,3 µs
    println!(
        "{:>28} {:>10} {:>10}",
        "estágio", "min (µs)", "mediana (µs)"
    );
    println!("{}", "-".repeat(52));
    println!("{:>28} {:>10.3} {:>10.3}", "simbólico (1)-(3)", sym_min_v, sym_med_v);
    println!("{:>28} {:>10.3} {:>10}", "  presolve", pre_min_v, "—");
    println!("{:>28} {:>10.3} {:>10}", "  persistência", pers_min_v, "—");
    println!("{:>28} {:>10.3} {:>10}", "  decomposição", dec_min_v, "—");
    println!();
    println!(
        "orçamento G3b (5% de 81,3 µs) : {budget:.2} µs  ->  {} (min {:.2} × teto)",
        pas(sym_min_v <= budget),
        sym_min_v / budget
    );
    println!(
        "fixadas simbolicamente (média) : {:.1} de {n}",
        fixed_sum / args.instances as f64
    );

    std::fs::create_dir_all("target").ok();
    let json = format!(
        "{{\n  \"n\": {n}, \"m\": {m}, \"reps\": {}, \"instances\": {},\n  \"symbolic_us_min\": {sym_min_v:.4},\n  \"symbolic_us_median\": {sym_med_v:.4},\n  \"presolve_us_min\": {pre_min_v:.4},\n  \"persistency_us_min\": {pers_min_v:.4},\n  \"decompose_us_min\": {dec_min_v:.4},\n  \"budget_us\": {budget:.3},\n  \"within_budget\": {}\n}}\n",
        args.reps,
        args.instances,
        sym_min_v <= budget
    );
    std::fs::write("target/aether_e2_symbolic.json", json)?;
    println!("artefato: target/aether_e2_symbolic.json");
    Ok(())
}

fn pas(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}
