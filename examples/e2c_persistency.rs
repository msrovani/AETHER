//! E2c / E2c-2 — persistência preserva o ótimo? Até onde ela alcança?
//!
//! Metodologia (idêntica à do E2c v1, para comparação direta):
//! 1. instâncias de Ising `n=14`, `J` ±1 simétrica, `h` com **viés plantado**
//!    em `plant·n` variáveis (emula bits dirigidos de um circuito);
//! 2. ótimo global por **força bruta** (ground truth exato);
//! 3. três braços por instância:
//!    * **dominância** (`persistency_qumo`, E2c v1): `max Δ_i < 0`;
//!    * **roof dual** (`roof_dual_qumo`, E2c-2): posiform quadrática + max-flow
//!      (Dinic) + alcançabilidade residual, com cascata;
//!    * **BSB puro** (sem presolve), como referência de completude;
//! 4. **soundness**: o ótimo do sistema reduzido tem de ser **igual** ao original;
//! 5. resolve o resíduo com BSB, reconstrói e confere a energia.
//!
//! Regimes padrão: `plant ∈ {0%, 25%, 45%}` × 200 instâncias = **600**.
//!
//! ```text
//! cargo run --release --example e2c_persistency
//! cargo run --release --example e2c_persistency -- --plant 0.0     # um regime
//! ```
//!
//! Saída: tabela + `target/aether_e2c.json` (e2c v1) e `target/aether_e2c2.json`.

use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::energy;
use aether_engine::core::presolve::{persistency_qumo, roof_dual_qumo, roof_dual_qumo_z2};
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};

struct Args {
    instances: usize,
    n: usize,
    /// Regimes de viés plantado (fração de variáveis com `h` forte).
    plants: Vec<f64>,
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
            plants: vec![0.0, 0.25, 0.45],
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
                a.plants = vec![v.parse().unwrap_or(0.25)];
                i += 1;
            }
            ("--plants", Some(v)) => {
                a.plants = v
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect::<Vec<f64>>();
                if a.plants.is_empty() {
                    a.plants = Args::default().plants;
                }
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
    let mut s = vec![0i8; n];
    for (i, f) in fixed.iter().enumerate() {
        if let Some(v) = f {
            s[i] = *v;
        }
    }
    let rn = residual.len();
    if rn == 0 {
        return energy(j, h, n, &s);
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

/// Resultado acumulado de um braço de presolve.
#[derive(Default, Clone, Copy)]
struct Arm {
    reduction_sum: f64,
    fixed_sum: usize,
    unsound: usize,
    solved: usize,
    deficit: f64,
}

impl Arm {
    /// Avalia um presolve: soundness por força bruta e reconstrução com BSB.
    #[allow(clippy::too_many_arguments)]
    fn eval(
        &mut self,
        pre: &aether_engine::core::presolve::QuboPresolve,
        n: usize,
        j: &[f64],
        h: &[f64],
        e_star: f64,
        steps: usize,
        restarts: usize,
        seed: u64,
    ) {
        self.reduction_sum += pre.reduction(n);
        self.fixed_sum += pre.fixed_count();
        let e_red = optimum_with_fixed(n, j, h, &pre.fixed, &pre.residual);
        if (e_red - e_star).abs() > 1e-9 {
            self.unsound += 1;
            self.deficit += e_red - e_star;
        }
        let sol = solve_residual(&pre.residual, j, &pre.h_eff, n, steps, restarts, seed);
        let mut s = vec![0i8; n];
        for (i, f) in pre.fixed.iter().enumerate() {
            if let Some(v) = f {
                s[i] = *v;
            }
        }
        for (new, &old) in pre.residual.iter().enumerate() {
            s[old] = sol[new];
        }
        if (energy(j, h, n, &s) - e_star).abs() <= 1e-9 {
            self.solved += 1;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n = args.n;

    println!();
    println!("AETHER · E2c-2 — persistência (dominância × roof dual max-flow)");
    println!("===============================================================");
    println!(
        "instâncias={}/regime n={} regimes={:?} delta={} steps={} restarts={}",
        args.instances, n, args.plants, args.delta, args.steps, args.restarts
    );
    println!();
    println!(
        "{:>7} {:>11} {:>9} {:>9} {:>11} {:>12} {:>10}",
        "regime", "método", "redução", "fixadas", "ótimo mudou", "presolve+BSB", "BSB puro"
    );
    println!("{}", "-".repeat(74));

    struct RegimeRow {
        plant: f64,
        dom: Arm,
        roof: Arm,
        roof_z2: Arm,
        full_solved: usize,
    }
    let mut rows: Vec<RegimeRow> = Vec::new();
    let total_instances = args.instances * args.plants.len();

    for &plant in &args.plants {
        let k = (plant * n as f64).round() as usize;
        let mut dom = Arm::default();
        let mut roof = Arm::default();
        let mut roof_z2 = Arm::default();
        let mut full_solved = 0usize;

        for inst in 0..args.instances {
            let seed = args.seed.wrapping_add(inst as u64);
            let (j, h) = make_instance(n, k, args.delta, seed);
            let e_star = brute_force(n, &j, &h);

            // --- braço 1: dominância (E2c v1) ---
            dom.eval(
                &persistency_qumo(n, &j, &h),
                n,
                &j,
                &h,
                e_star,
                args.steps,
                args.restarts,
                seed,
            );

            // --- braço 2: roof dual (E2c-2) ---
            roof.eval(
                &roof_dual_qumo(n, &j, &h),
                n,
                &j,
                &h,
                e_star,
                args.steps,
                args.restarts,
                seed,
            );

            // --- braço 3: roof dual + quebra de simetria Z2 ---
            roof_z2.eval(
                &roof_dual_qumo_z2(n, &j, &h),
                n,
                &j,
                &h,
                e_star,
                args.steps,
                args.restarts,
                seed,
            );

            // --- braço 4: BSB puro ---
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

        let show = |label: &str, a: &Arm| {
            println!(
                "{:>6.0}% {:>15} {:>8.1}% {:>9.2} {:>11} {:>7}/{:<4} {:>4}/{:<5}",
                plant * 100.0,
                label,
                100.0 * a.reduction_sum / args.instances as f64,
                a.fixed_sum as f64 / args.instances as f64,
                a.unsound,
                a.solved,
                args.instances,
                full_solved,
                args.instances
            );
        };
        show("dominância", &dom);
        show("roof dual", &roof);
        show("roof dual + z2", &roof_z2);
        println!("{}", "-".repeat(78));

        rows.push(RegimeRow {
            plant,
            dom,
            roof,
            roof_z2,
            full_solved,
        });
    }

    // ---------------- gates ----------------
    let total_unsound: usize = rows
        .iter()
        .map(|r| r.roof.unsound + r.roof_z2.unsound)
        .sum();
    let mut reach_ok = true;
    let mut reach_z2_ok = true;
    for r in &rows {
        let rd = r.dom.reduction_sum / args.instances as f64;
        let rr = r.roof.reduction_sum / args.instances as f64;
        let rz = r.roof_z2.reduction_sum / args.instances as f64;
        if rr + 1e-12 < rd {
            reach_ok = false;
        }
        if rz <= rd + 1e-12 {
            reach_z2_ok = false;
        }
    }
    let strict_all = rows
        .iter()
        .all(|r| r.roof.reduction_sum > r.dom.reduction_sum + 1e-12);
    let row45 = rows
        .iter()
        .min_by(|a, b| {
            (a.plant - 0.45)
                .abs()
                .partial_cmp(&(b.plant - 0.45).abs())
                .unwrap()
        })
        .expect("ao menos um regime");
    let row0 = rows
        .iter()
        .min_by(|a, b| {
            (a.plant - 0.0)
                .abs()
                .partial_cmp(&(b.plant - 0.0).abs())
                .unwrap()
        })
        .expect("ao menos um regime");

    println!();
    println!("gates E2c-2");
    println!(
        "  G1 soundness ({} regimes × {} instâncias) : {}  ({} ótimos alterados)",
        args.plants.len(),
        args.instances,
        pass(total_unsound == 0),
        total_unsound
    );
    println!(
        "  G2a alcance ≥ dominância (roof dual)        : {}  (estrito em todos: {})",
        pass(reach_ok),
        pass(strict_all)
    );
    println!(
        "  G2b alcance > dominância (roof + z2)        : {}",
        pass(reach_z2_ok)
    );
    println!(
        "  G3 completude em 45% ≥ 199/200              : {}  ({}/{})",
        pass(row45.roof_z2.solved.max(row45.roof.solved) >= 199.min(args.instances)),
        row45.roof_z2.solved.max(row45.roof.solved),
        args.instances
    );
    println!(
        "  G4 regime sem viés melhora o BSB puro        : {}  (roof {}/{} · roof+z2 {}/{} · BSB {}/{})",
        pass(row0.roof_z2.solved > row0.full_solved || row0.roof.solved > row0.full_solved),
        row0.roof.solved,
        args.instances,
        row0.roof_z2.solved,
        args.instances,
        row0.full_solved,
        args.instances
    );
    println!();

    std::fs::create_dir_all("target").ok();
    let mut json = String::new();
    json.push_str("{\n  \"e2c2\": {\n");
    json.push_str(&format!(
        "    \"instances_per_regime\": {}, \"n\": {n}, \"delta\": {},\n    \"regimes\": [\n",
        args.instances, args.delta
    ));
    for (i, r) in rows.iter().enumerate() {
        let comma = if i + 1 == rows.len() { "" } else { "," };
        let dom_red = r.dom.reduction_sum / args.instances as f64;
        let roof_red = r.roof.reduction_sum / args.instances as f64;
        let z2_red = r.roof_z2.reduction_sum / args.instances as f64;
        json.push_str(&format!(
            "      {{\"plant\": {:.3}, \"dom_reduction\": {dom_red:.4}, \"roof_reduction\": {roof_red:.4}, \"roof_z2_reduction\": {z2_red:.4}, \"dom_unsound\": {}, \"roof_unsound\": {}, \"roof_z2_unsound\": {}, \"dom_solved\": {}, \"roof_solved\": {}, \"roof_z2_solved\": {}, \"bsb_solved\": {}, \"instances\": {}}}{comma}\n",
            r.plant,
            r.dom.unsound,
            r.roof.unsound,
            r.roof_z2.unsound,
            r.dom.solved,
            r.roof.solved,
            r.roof_z2.solved,
            r.full_solved,
            args.instances
        ));
    }
    json.push_str("    ],\n");
    json.push_str(&format!(
        "    \"total_instances\": {total_instances}, \"roof_unsound_total\": {total_unsound}, \"reach_ok\": {reach_ok}, \"reach_z2_ok\": {reach_z2_ok}\n"
    ));
    json.push_str("  }\n}\n");
    let path = "target/aether_e2c2.json";
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
