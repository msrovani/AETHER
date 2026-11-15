//! E2c — persistência de primeira ordem preserva o ótimo?
//!
//! Metodologia:
//! 1. instâncias de Ising `n=14`, `J` ±1 simétrica, `h` com **viés plantado**
//!    em `--plant` das variáveis (emula bits dirigidos de um circuito);
//! 2. ótimo global por **força bruta** (ground truth exato);
//! 3. `persistency_qumo` fixa variáveis por dominância (`max Δ_i < 0`);
//! 4. **soundness**: o ótimo do sistema reduzido tem de ser **igual** ao original;
//! 5. resolve o resíduo com BSB, reconstrói e confere a energia.
//!
//! Dois braços: `--plant 0.25` (estruturado) e `--plant 0` (vidro de spin puro),
//! para mostrar onde a regra **não** dispara.
//!
//! ```text
//! cargo run --release --example e2c_persistency
//! cargo run --release --example e2c_persistency -- --plant 0.0
//! ```
//!
//! Saída: tabela + `target/aether_e2c.json`.

use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::persistency_qumo;
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};

struct Args {
    instances: usize,
    n: usize,
    plant: f64,
    delta: f64,
    steps: usize,
    restarts: usize,
    seed: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            instances: 200,
            n: 14,
            plant: 0.25,
            delta: 0.6,
            steps: 20,
            restarts: 4,
            seed: 0xE2C_0001,
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
            ("--delta", Some(v)) => {
                a.delta = v.parse().unwrap_or(a.delta);
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

/// `J` ±1 simétrica (diagonal zero) + viés `h` em `k` variáveis.
fn make_instance(n: usize, k: usize, delta: f64, seed: u64) -> (Vec<f64>, Vec<f64>) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for m in (i + 1)..n {
            let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            j[i * n + m] = w;
            j[m * n + i] = w;
        }
    }

    let mut h = vec![0.0f64; n];
    let mut order: Vec<usize> = (0..n).collect();
    for idx in (1..n).rev() {
        let sw = rng.gen_range(0..=idx);
        order.swap(idx, sw);
    }
    for &v in order.iter().take(k) {
        let row_abs: f64 = j[v * n..(v + 1) * n]
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != v)
            .map(|(_, x)| x.abs())
            .sum();
        let sign = if rng.gen::<bool>() { 1.0 } else { -1.0 };
        h[v] = sign * (row_abs + delta);
    }
    (j, h)
}

fn brute_force(n: usize, j: &[f64], h: &[f64]) -> f64 {
    let mut best = f64::INFINITY;
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        let e = energy(j, h, n, &s);
        if e < best {
            best = e;
        }
    }
    best
}

/// Ótimo com as fixações aplicadas (enumera só o resíduo).
fn optimum_with_fixed(
    n: usize,
    j: &[f64],
    h: &[f64],
    fixed: &[Option<i8>],
    residual: &[usize],
) -> f64 {
    let rn = residual.len();
    let mut s = vec![0i8; n];
    for (i, f) in fixed.iter().enumerate() {
        if let Some(v) = f {
            s[i] = *v;
        }
    }
    let mut best = f64::INFINITY;
    for mask in 0..(1u64 << rn) {
        for (k, &old) in residual.iter().enumerate() {
            s[old] = if (mask >> k) & 1 == 0 { -1 } else { 1 };
        }
        let e = energy(j, h, n, &s);
        if e < best {
            best = e;
        }
    }
    if rn == 0 {
        best = energy(j, h, n, &s);
    }
    best
}

fn solve_residual(
    residual: &[usize],
    j: &[f64],
    h_eff: &[f64],
    n: usize,
    steps: usize,
    restarts: usize,
    seed: u64,
) -> Vec<i8> {
    let rn = residual.len();
    if rn == 0 {
        return Vec::new();
    }
    let mut js = vec![0.0f64; rn * rn];
    let mut hs = vec![0.0f64; rn];
    for (a, &ia) in residual.iter().enumerate() {
        hs[a] = h_eff[ia];
        for (b, &ib) in residual.iter().enumerate() {
            js[a * rn + b] = j[ia * n + ib];
        }
    }

    let mut j3 = Array3::<f64>::zeros((1, rn, rn));
    let mut h2 = Array2::<f64>::zeros((1, rn));
    for (idx, chunk) in js.chunks(rn).enumerate() {
        for (k, &v) in chunk.iter().enumerate() {
            j3[[0, idx, k]] = v;
        }
    }
    for i in 0..rn {
        h2[[0, i]] = hs[i];
    }

    let mut solver = ThermodynamicIsingSolver::new(1, rn, steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, rn, steps);
    let mut best_e = f64::INFINITY;
    let mut best = vec![0i8; rn];

    for r in 0..restarts {
        solver.reset(seed.wrapping_mul(0x9E37_79B9).wrapping_add(r as u64));
        solver
            .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
            .expect("relax");
        let e = out.best_energy[0];
        if e < best_e {
            best_e = e;
            for (i, b) in best.iter_mut().enumerate() {
                *b = out.states[[0, i]];
            }
        }
    }
    best
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n = args.n;
    let k = (args.plant * n as f64).round() as usize;

    let mut reduction_sum = 0.0;
    let mut unsound = 0usize;
    let mut solved = 0usize;
    let mut full_solved = 0usize;
    let mut deficit = 0.0f64;

    for inst in 0..args.instances {
        let seed = args.seed.wrapping_add(inst as u64);
        let (j, h) = make_instance(n, k, args.delta, seed);
        let e_star = brute_force(n, &j, &h);

        let pre = persistency_qumo(n, &j, &h);
        reduction_sum += pre.reduction(n);

        let e_red = optimum_with_fixed(n, &j, &h, &pre.fixed, &pre.residual);
        if (e_red - e_star).abs() > 1e-9 {
            unsound += 1;
            deficit += e_red - e_star;
        }

        // reconstrói com BSB no resíduo
        let sol = solve_residual(
            &pre.residual,
            &j,
            &pre.h_eff,
            n,
            args.steps,
            args.restarts,
            seed,
        );
        let mut s = vec![0i8; n];
        for (i, f) in pre.fixed.iter().enumerate() {
            if let Some(v) = f {
                s[i] = *v;
            }
        }
        for (new, &old) in pre.residual.iter().enumerate() {
            s[old] = sol[new];
        }
        let e_rec = energy(&j, &h, n, &s);
        if (e_rec - e_star).abs() <= 1e-9 {
            solved += 1;
        }

        // BSB puro (sem presolve), para comparação de completude
        let full = solve_residual(
            &(0..n).collect::<Vec<_>>(),
            &j,
            &h,
            n,
            args.steps,
            args.restarts,
            seed,
        );
        let mut s_full = vec![0i8; n];
        s_full.copy_from_slice(&full);
        if (energy(&j, &h, n, &s_full) - e_star).abs() <= 1e-9 {
            full_solved += 1;
        }
    }

    let reduction = reduction_sum / args.instances as f64;

    println!();
    println!("AETHER · E2c — persistência de primeira ordem (QUBO/Ising)");
    println!("=====================================================");
    println!(
        "instâncias={} n={} plant={:.0}% ({} vieses) delta={} steps={} restarts={}",
        args.instances,
        n,
        100.0 * args.plant,
        k,
        args.delta,
        args.steps,
        args.restarts
    );
    println!();
    println!(
        "redução média de variáveis       : {:.1}%",
        100.0 * reduction
    );
    println!("fixações que mudaram o ótimo     : {unsound}");
    if unsound > 0 {
        println!("déficit acumulado no ótimo       : {deficit:.6}");
    }
    println!(
        "reconstruções no ótimo           : {solved}/{}",
        args.instances
    );
    println!(
        "BSB puro no ótimo                : {full_solved}/{}",
        args.instances
    );
    println!();

    println!(
        "  G1 soundness (ótimo preservado)   : {}",
        pass(unsound == 0)
    );
    println!(
        "  G2 redução ≥ 40%                  : {}  ({:.1}%)",
        pass(reduction >= 0.40),
        100.0 * reduction
    );
    println!(
        "  G3 reconstrução no ótimo = 100%   : {}",
        pass(solved == args.instances)
    );
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e2c.json";
    let json = format!(
        "{{\n  \"instances\": {},\n  \"n\": {n}, \"plant\": {:.3}, \"delta\": {:.3},\n  \"reduction\": {reduction:.4},\n  \"unsound\": {unsound},\n  \"solved\": {solved},\n  \"full_bsb_solved\": {full_solved}\n}}\n",
        args.instances, args.plant, args.delta
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
