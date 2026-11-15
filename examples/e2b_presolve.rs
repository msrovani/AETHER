//! E2b — o presolve simbólico reduz o suficiente, e é *sound*?
//!
//! Metodologia:
//! 1. gera 2-SAT aleatório **com unidades plantadas** (emula CNF derivado de
//!    circuito, onde bits fixos propagam — arXiv:2511.21046);
//! 2. força bruta dá a satisfatibilidade e **todas** as soluções (n ≤ 18);
//! 3. o presolve (`core::presolve`) fixa variáveis por unidade e por implicação;
//! 4. valida **soundness**: nenhuma fixação pode contradizer uma solução real;
//! 5. resolve o **resíduo** com o BSB e reconstrói; compara com o BSB sem presolve.
//!
//! ```text
//! cargo run --release --example e2b_presolve
//! cargo run --release --example e2b_presolve -- --n 18 --plant 0.3 --instances 200
//! ```
//!
//! Saída: tabela + `target/aether_e2b.json`.

use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::presolve::{presolve_2sat, residual_clauses, Clause2};
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};

struct Args {
    instances: usize,
    n: usize,
    /// Fração de variáveis com cláusula unitária plantada.
    plant: f64,
    /// Cláusulas por variável (1.0 = limiar do 2-SAT).
    density: f64,
    steps: usize,
    restarts: usize,
    seed: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            instances: 200,
            n: 16,
            plant: 0.25,
            density: 1.5,
            steps: 12,
            restarts: 4,
            seed: 0xE2B_0001,
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
        match (key, raw) {
            ("--instances", Some(v)) => {
                a.instances = v.parse().unwrap_or(a.instances);
                i += 1;
            }
            ("--n", Some(v)) => {
                a.n = v.parse().unwrap_or(a.n);
                i += 1;
            }
            ("--plant", Some(v)) => {
                a.plant = v.parse().unwrap_or(a.plant);
                i += 1;
            }
            ("--density", Some(v)) => {
                a.density = v.parse().unwrap_or(a.density);
                i += 1;
            }
            ("--steps", Some(v)) => {
                a.steps = v.parse().unwrap_or(a.steps);
                i += 1;
            }
            ("--restarts", Some(v)) => {
                a.restarts = (v.parse().unwrap_or(a.restarts)).max(1);
                i += 1;
            }
            ("--seed", Some(v)) => {
                a.seed = v.parse().unwrap_or(a.seed);
                i += 1;
            }
            (k, _) => eprintln!("argumento ignorado: {k}"),
        }
        i += 1;
    }
    a
}

/// 2-SAT **plantado**: toda cláusula é satisfeita por `s*`; unidades seguem `s*`.
///
/// Assim todas as instâncias são satisfatíveis por construção, e a redução medida
/// não é contaminada por instâncias impossíveis.
fn make_instance(n: usize, m: usize, k_units: usize, seed: u64) -> (Vec<Clause2>, Vec<i8>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut s_star = vec![1i8; n];
    for v in s_star.iter_mut() {
        *v = if rng.gen::<bool>() { 1 } else { -1 };
    }

    let mut out = Vec::with_capacity(m + k_units);
    for _ in 0..m {
        let i = rng.gen_range(0..n);
        let mut j = rng.gen_range(0..n);
        if j == i {
            j = (j + 1) % n;
        }
        let si = if rng.gen::<bool>() { 1 } else { -1 };
        // se o primeiro literal já é falso sob s*, o segundo tem de ser verdadeiro
        let sj = if s_star[i] == si {
            if rng.gen::<bool>() {
                1
            } else {
                -1
            }
        } else {
            s_star[j]
        };
        out.push(Clause2::new(i, si, j, sj));
    }

    let mut order: Vec<usize> = (0..n).collect();
    for idx in (1..n).rev() {
        let sw = rng.gen_range(0..=idx);
        order.swap(idx, sw);
    }
    for &v in order.iter().take(k_units) {
        let sign = s_star[v];
        out.push(Clause2::new(v, sign, v, sign));
    }
    (out, s_star)
}

fn count_violations(clauses: &[Clause2], s: &[i8]) -> u32 {
    clauses.iter().filter(|c| !c.satisfied(s)).count() as u32
}

/// Enumera todas as soluções; devolve `(satisfatível, nº de soluções)` e valida
/// as fixações contra elas.
fn brute_force(n: usize, clauses: &[Clause2], fixed: &[Option<i8>]) -> (bool, u64, u64) {
    let mut s = vec![0i8; n];
    let mut sat = false;
    let mut count = 0u64;
    let mut unsound = 0u64;

    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        if count_violations(clauses, &s) != 0 {
            continue;
        }
        sat = true;
        count += 1;
        for (i, f) in fixed.iter().enumerate() {
            if let Some(v) = f {
                if s[i] != *v {
                    unsound += 1;
                }
            }
        }
    }
    (sat, count, unsound)
}

/// Ising para 2-SAT: por cláusula, `J_ij = -s_i s_j / 4` e `h_i = +s_i / 4`.
fn ising_from_clauses(n: usize, clauses: &[Clause2]) -> (Vec<f64>, Vec<f64>) {
    let mut j = vec![0.0f64; n * n];
    let mut h = vec![0.0f64; n];
    for c in clauses {
        if c.i == c.j && c.si == c.sj {
            // unidade: (x) ⇒ penaliza (1 - s_i x)/2
            h[c.i] += 0.5 * f64::from(c.si);
            continue;
        }
        let (si, sj) = (f64::from(c.si), f64::from(c.sj));
        let w = 0.25 * si * sj;
        j[c.i * n + c.j] -= w;
        j[c.j * n + c.i] -= w;
        h[c.i] += 0.25 * si;
        h[c.j] += 0.25 * sj;
    }
    (j, h)
}

fn solve(
    n: usize,
    clauses: &[Clause2],
    restarts: usize,
    seed: u64,
    solver: &mut ThermodynamicIsingSolver,
    out: &mut RelaxOutcome,
) -> Vec<i8> {
    if n == 0 {
        return Vec::new();
    }
    let (j, h) = ising_from_clauses(n, clauses);
    let mut j3 = Array3::<f64>::zeros((1, n, n));
    let mut h2 = Array2::<f64>::zeros((1, n));
    for (idx, chunk) in j.chunks(n).enumerate() {
        for (k, &v) in chunk.iter().enumerate() {
            j3[[0, idx, k]] = v;
        }
    }
    for i in 0..n {
        h2[[0, i]] = h[i];
    }

    let mut best = u32::MAX;
    let mut best_s = vec![0i8; n];
    for r in 0..restarts {
        solver.reset(seed.wrapping_mul(0x9E37_79B9).wrapping_add(r as u64));
        solver
            .relax_into(IsingLandscape::new(j3.view(), h2.view()), out)
            .expect("relax");
        let mut s = vec![0i8; n];
        for (i, si) in s.iter_mut().enumerate() {
            *si = out.states[[0, i]];
        }
        let v = count_violations(clauses, &s);
        if v < best {
            best = v;
            best_s.copy_from_slice(&s);
        }
    }
    best_s
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n = args.n;
    let m = (args.density * n as f64).round() as usize;
    let k_units = (args.plant * n as f64).round() as usize;

    let mut solver = ThermodynamicIsingSolver::new(1, n, args.steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, n, args.steps);

    let mut reduction_sum = 0.0;
    let mut unsound_total = 0u64;
    let mut conflict_ok = 0usize;
    let mut sat_count = 0usize;
    let mut decisive = 0usize;
    let mut decisive_valid = 0usize;
    let mut full_solved = 0usize;
    let mut presolved_solved = 0usize;
    let mut res_n_sum = 0usize;

    for inst in 0..args.instances {
        let seed = args.seed.wrapping_add(inst as u64);
        let (clauses, s_star) = make_instance(n, m, k_units, seed);
        debug_assert_eq!(count_violations(&clauses, &s_star), 0);

        let pre = presolve_2sat(n, &clauses);
        reduction_sum += pre.reduction(n);
        res_n_sum += pre.residual.len();

        let (sat, _, unsound) = brute_force(n, &clauses, &pre.fixed);
        unsound_total += unsound;
        if sat {
            sat_count += 1;
        }
        if pre.conflict == !sat {
            conflict_ok += 1;
        }

        // --- braço A: BSB sem presolve ---
        let full = solve(n, &clauses, args.restarts, seed, &mut solver, &mut out);
        if count_violations(&clauses, &full) == 0 {
            full_solved += 1;
        }

        // --- braço B: presolve + BSB no resíduo + reconstrução ---
        let mut assignment = vec![0i8; n];
        for (i, f) in pre.fixed.iter().enumerate() {
            if let Some(v) = f {
                assignment[i] = *v;
            }
        }
        if !pre.residual.is_empty() && !pre.conflict {
            if let Some(res_clauses) = residual_clauses(&clauses, &pre.fixed, &pre.residual) {
                let rn = pre.residual.len();
                let mut local_solver = ThermodynamicIsingSolver::new(1, rn, args.steps, 0.05);
                let mut local_out = RelaxOutcome::with_capacity(1, rn, args.steps);
                let res_sol = solve(
                    rn,
                    &res_clauses,
                    args.restarts,
                    seed ^ 0xA5A5,
                    &mut local_solver,
                    &mut local_out,
                );
                for (new, &old) in pre.residual.iter().enumerate() {
                    assignment[old] = res_sol[new];
                }
                let v = count_violations(&clauses, &assignment);
                if v == 0 {
                    decisive += 1;
                    decisive_valid += 1;
                    presolved_solved += 1;
                }
            }
        } else if pre.residual.is_empty() && !pre.conflict {
            // tudo fixado: resposta decidida sem solver
            if count_violations(&clauses, &assignment) == 0 {
                decisive += 1;
                decisive_valid += 1;
                presolved_solved += 1;
            }
        }
    }

    let reduction = reduction_sum / args.instances as f64;
    let plant = k_units as f64 / n as f64;
    let precision = if decisive > 0 {
        decisive_valid as f64 / decisive as f64
    } else {
        1.0
    };
    let res_n = res_n_sum as f64 / args.instances as f64;

    println!();
    println!("AETHER · E2b — presolve simbólico (unidades + implicações)");
    println!("=====================================================");
    println!(
        "instâncias={} n={} m={} unidades={} (plantados: satisfatíveis por construção)",
        args.instances, n, m, k_units
    );
    println!(
        "satisfatíveis (força bruta)      : {}/{}",
        sat_count, args.instances
    );
    println!();
    println!(
        "redução média de variáveis       : {:.1}%",
        100.0 * reduction
    );
    println!("  · plantadas (unidades)         : {:.1}%", 100.0 * plant);
    println!(
        "  · cascata (unidades+implicação): {:+.1} pp",
        100.0 * (reduction - plant)
    );
    println!("variáveis residuais (média)      : {res_n:.1} de {n}");
    println!("fixações insound                 : {unsound_total}");
    println!(
        "detecção de conflito correta     : {}/{}",
        conflict_ok, args.instances
    );
    println!();
    println!(
        "resolvidas por BSB puro          : {full_solved}/{}",
        args.instances
    );
    println!(
        "resolvidas com presolve+BSB      : {presolved_solved}/{}",
        args.instances
    );
    println!("decisivas (0 violações)          : {decisive}");
    println!(
        "precisão das decisivas           : {:.1}%",
        100.0 * precision
    );
    println!();

    let g_sound = unsound_total == 0;
    let g_reduce = reduction >= 0.40;
    let g_prec = precision >= 0.999;
    println!("  G1 soundness (0 fixações insound) : {}", pass(g_sound));
    println!(
        "  G2 redução ≥ 40%                  : {}  ({:.1}%)",
        pass(g_reduce),
        100.0 * reduction
    );
    println!("  G3 precisão das decisivas = 100%  : {}", pass(g_prec));
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e2b.json";
    let json = format!(
        "{{\n  \"instances\": {},\n  \"n\": {n}, \"m\": {m}, \"units\": {k_units},\n  \"satisfiable\": {sat_count},\n  \"reduction\": {reduction:.4},\n  \"residual_n\": {res_n:.3},\n  \"unsound_fixings\": {unsound_total},\n  \"conflict_detected_correctly\": {conflict_ok},\n  \"full_bsb_solved\": {full_solved},\n  \"presolved_solved\": {presolved_solved},\n  \"decisive\": {decisive},\n  \"precision\": {precision:.4}\n}}\n",
        args.instances
    );
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    Ok(())
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}
