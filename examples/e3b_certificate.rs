//! E3b — o `ΔE` prediz acerto? Calibração do certificado de coerência.
//!
//! Metodologia (predição seletiva):
//! 1. `--instances` instâncias de **vidro de spin** (MAX-CUT com pesos ±1,
//!    frustrado) — `H(s) = -0.5·sᵀJs`, `h = 0`;
//! 2. ótimo global por **força bruta** (ground truth exato, `n ≤ 18`);
//! 3. BSB com `--restarts` reinicializações → melhor estado;
//! 4. emite o [`Certificate`] e mede **cinco sinais** de confiança:
//!    `gap local`, `consenso entre restarts`, `gap entre restarts`,
//!    `gap até o limite espectral` (rigoroso) e a confiança combinada;
//! 5. avalia com **cobertura fixa** (25% e 50% mais confiáveis): precisão,
//!    recall e taxa de "confiante e errado". Threshold escolhido **só no treino**.
//!
//! ```text
//! cargo run --release --example e3b_certificate
//! cargo run --release --example e3b_certificate -- --n 16 --steps 10 --restarts 4
//! ```
//!
//! Saída: tabela + `target/aether_e3b.json`.

use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::{
    energy, local_gap_with, spectral_lower_bound, Certificate, Decision, Provenance,
};
use aether_engine::core::thermodynamic_solver::{
    IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
};

struct Args {
    instances: usize,
    n: usize,
    restarts: usize,
    steps: usize,
    seed: u64,
    max2sat: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            instances: 400,
            n: 14,
            restarts: 4, // ≥2 para o consenso ter variância; ≥4 para estabilizar
            steps: 8,    // curto o suficiente para induzir ~10% de falhas
            seed: 0xE3B_0001,
            max2sat: false,
        }
    }
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let key = argv[i].as_str();
        let val = argv.get(i + 1).and_then(|v| v.parse::<u64>().ok());
        match (key, val) {
            ("--instances", Some(v)) => {
                a.instances = v as usize;
                i += 1;
            }
            ("--n", Some(v)) => {
                a.n = v as usize;
                i += 1;
            }
            ("--restarts", Some(v)) => {
                a.restarts = (v as usize).max(1);
                i += 1;
            }
            ("--steps", Some(v)) => {
                a.steps = (v as usize).max(1);
                i += 1;
            }
            ("--seed", Some(v)) => {
                a.seed = v;
                i += 1;
            }
            ("--max2sat", _) => a.max2sat = true,
            (k, _) => eprintln!("argumento ignorado: {k}"),
        }
        i += 1;
    }
    a
}

fn make_instance(n: usize, seed: u64) -> Vec<f64> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut j = vec![0.0f64; n * n];
    for i in 0..n {
        for k in (i + 1)..n {
            let w = if rng.gen::<bool>() { 1.0 } else { -1.0 };
            j[i * n + k] = w;
            j[k * n + i] = w;
        }
    }
    j
}

fn brute_force(j: &[f64], h: &[f64], n: usize) -> f64 {
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

fn auc(scores: &[(f64, bool)]) -> f64 {
    let n = scores.len();
    if n == 0 {
        return f64::NAN;
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        scores[a]
            .0
            .partial_cmp(&scores[b].0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ranks = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut k = i;
        while k + 1 < n && (scores[order[k + 1]].0 - scores[order[i]].0).abs() < 1e-12 {
            k += 1;
        }
        let avg = ((i + 1) as f64 + (k + 1) as f64) / 2.0;
        for idx in i..=k {
            ranks[order[idx]] = avg;
        }
        i = k + 1;
    }
    let (mut sum_pos, mut n_pos, mut n_neg) = (0.0, 0.0, 0.0);
    for (idx, (_, correct)) in scores.iter().enumerate() {
        if *correct {
            sum_pos += ranks[idx];
            n_pos += 1.0;
        } else {
            n_neg += 1.0;
        }
    }
    if n_pos == 0.0 || n_neg == 0.0 {
        return f64::NAN;
    }
    (sum_pos - n_pos * (n_pos + 1.0) / 2.0) / (n_pos * n_neg)
}

#[derive(Clone, Copy)]
struct AtCoverage {
    coverage: f64,
    precision: f64,
    recall: f64,
    false_confident: f64,
}

fn threshold_at_coverage(train: &[(f64, bool)], target: f64) -> f64 {
    let mut v: Vec<f64> = train.iter().map(|x| x.0).collect();
    v.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    if v.is_empty() {
        return f64::INFINITY;
    }
    let idx = ((v.len() as f64 * target).ceil() as usize)
        .saturating_sub(1)
        .min(v.len() - 1);
    v[idx]
}

fn evaluate_at(test: &[(f64, bool)], threshold: f64) -> AtCoverage {
    let (mut tp, mut fp, mut pos) = (0.0, 0.0, 0.0);
    for &(s, c) in test {
        if c {
            pos += 1.0;
        }
        if s >= threshold {
            if c {
                tp += 1.0
            } else {
                fp += 1.0
            }
        }
    }
    let n = test.len() as f64;
    let conf = tp + fp;
    AtCoverage {
        coverage: conf / n,
        precision: if conf > 0.0 { tp / conf } else { 0.0 },
        recall: if pos > 0.0 { tp / pos } else { 0.0 },
        false_confident: if conf > 0.0 { fp / conf } else { 0.0 },
    }
}

fn split_signal(rows: &[Row], pick: impl Fn(&Row) -> f64) -> (Vec<(f64, bool)>, Vec<(f64, bool)>) {
    let mut tr = Vec::new();
    let mut te = Vec::new();
    for (idx, r) in rows.iter().enumerate() {
        if idx % 2 == 0 {
            tr.push((pick(r), r.correct));
        } else {
            te.push((pick(r), r.correct));
        }
    }
    (tr, te)
}

struct Row {
    local_gap: f64,
    consensus: f64,
    restart_gap: f64,
    bound_gap: f64,
    confidence: f64,
    energy: f64,
    e_star: f64,
    correct: bool,
}

type SignalFn = fn(&Row) -> f64;
type SignalResult = (&'static str, f64, AtCoverage, AtCoverage);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    if args.max2sat {
        return run_max2sat(&args);
    }
    let n = args.n;
    let h = vec![0.0f64; n];

    let mut jview = Array3::<f64>::zeros((1, n, n));
    let h2 = Array2::<f64>::zeros((1, n));
    let mut solver = ThermodynamicIsingSolver::new(1, n, args.steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, n, args.steps);
    let mut g = vec![0.0f64; n];
    let mut s = vec![0i8; n];

    let mut rows: Vec<Row> = Vec::with_capacity(args.instances);

    for inst in 0..args.instances {
        let j = make_instance(n, args.seed.wrapping_add(inst as u64));
        let e_star = brute_force(&j, &h, n);
        let lb = spectral_lower_bound(&j, &h, n);

        for (i, chunk) in j.chunks(n).enumerate() {
            for (k, &v) in chunk.iter().enumerate() {
                jview[[0, i, k]] = v;
            }
        }

        let mut energies: Vec<f64> = Vec::with_capacity(args.restarts);
        let mut best_states = vec![0i8; n];
        let mut e_best = f64::INFINITY;
        for r in 0..args.restarts {
            solver.reset(
                args.seed
                    ^ (inst as u64)
                        .wrapping_mul(0x9E37_79B9)
                        .wrapping_add(r as u64),
            );
            solver.relax_into(IsingLandscape::new(jview.view(), h2.view()), &mut out)?;
            let e = out.best_energy[0];
            energies.push(e);
            if e < e_best {
                e_best = e;
                for (i, si) in best_states.iter_mut().enumerate() {
                    *si = out.states[[0, i]];
                }
            }
        }

        let hits = energies
            .iter()
            .filter(|e| (*e - e_best).abs() < 1e-9)
            .count() as f64;
        let consensus = hits / args.restarts as f64;
        energies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let restart_gap = if energies.len() > 1 {
            (energies[1] - energies[0]).max(0.0)
        } else {
            0.0
        };

        s.copy_from_slice(&best_states);
        let gap = local_gap_with(&j, &h, n, &s, &mut g);
        let bound_gap = (e_best - lb).max(0.0);
        let correct = (e_best - e_star).abs() < 1e-9;

        rows.push(Row {
            local_gap: gap,
            consensus,
            restart_gap,
            bound_gap,
            confidence: 0.0, // preenchido abaixo, após conhecer a escala
            energy: e_best,
            e_star,
            correct,
        });
    }

    // escala do bound = mediana dos bound_gap (robusta a outliers)
    let mut bg: Vec<f64> = rows.iter().map(|r| r.bound_gap).collect();
    bg.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let bg_scale = bg[bg.len() / 2].max(1e-9);
    for r in rows.iter_mut() {
        let gnorm = (r.bound_gap / bg_scale).min(1.0);
        r.confidence = (gnorm * r.consensus).sqrt();
    }

    let n_correct = rows.iter().filter(|r| r.correct).count();
    let base = n_correct as f64 / rows.len() as f64;

    let signals: [(&str, SignalFn); 5] = [
        ("gap local", |r| r.local_gap),
        ("consenso", |r| r.consensus),
        ("gap restarts", |r| r.restart_gap),
        ("gap ate bound", |r| r.bound_gap),
        ("confianca", |r| r.confidence),
    ];

    println!();
    println!("AETHER · E3b — o ΔE prediz acerto?");
    println!("=====================================================");
    println!(
        "instâncias={} n={} restarts={} steps={} seed={:#x}",
        args.instances, n, args.restarts, args.steps, args.seed
    );
    println!(
        "acerto do solver (ground truth exato): {}/{} = {:.1}%",
        n_correct,
        rows.len(),
        100.0 * base
    );
    println!();
    println!(
        "{:<14} {:>7} {:>12} {:>11} {:>10} {:>11}",
        "sinal", "AUC", "prec@25%", "prec@50%", "recall@50", "conf.err@50"
    );
    println!("{}", "-".repeat(72));

    let mut results: Vec<SignalResult> = Vec::new();
    for (name, pick) in signals {
        let (tr, te) = split_signal(&rows, pick);
        let a = auc(&te);
        let m25 = evaluate_at(&te, threshold_at_coverage(&tr, 0.25));
        let m50 = evaluate_at(&te, threshold_at_coverage(&tr, 0.50));
        println!(
            "{:<14} {:>7.3} {:>11.1}% {:>10.1}% {:>9.1}% {:>10.1}%",
            name,
            a,
            100.0 * m25.precision,
            100.0 * m50.precision,
            100.0 * m50.recall,
            100.0 * m50.false_confident
        );
        results.push((name, a, m25, m50));
    }
    println!("{}", "-".repeat(72));
    println!(
        "(precisão de base = {:.1}%; ganho = precisão@50% − base)",
        100.0 * base
    );
    println!();

    let best = results
        .iter()
        .filter(|(_, a, _, _)| a.is_finite())
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let (bname, bauc, _, b50) =
        best.copied()
            .unwrap_or(("n/a", f64::NAN, results[0].2, results[0].3));
    let gain = b50.precision - base;

    println!("melhor sinal: {bname} (AUC {bauc:.3})");
    println!(
        "  G1 precisão@50% ≥ 95%          : {}  ({:.1}%)",
        pass(b50.precision >= 0.95),
        100.0 * b50.precision
    );
    println!(
        "  G2 ganho sobre a base ≥ 5 pp   : {}  ({:+.1} pp)",
        pass(gain >= 0.05),
        100.0 * gain
    );
    println!(
        "  G3 confiante-e-errado < 5%     : {}  ({:.1}%)",
        pass(b50.false_confident < 0.05),
        100.0 * b50.false_confident
    );
    println!();

    // --- demonstração da matriz 2D (Vol. III §6) ---
    let sample = Certificate::new(
        rows[0].energy,
        rows[0].bound_gap,
        0,
        rows[0].consensus,
        Provenance::Inferred,
    );
    let gap_min = threshold_at_coverage(&split_signal(&rows, |r| r.bound_gap).0, 0.50);
    println!("matriz 2D (1ª instância; gap=bound_gap, proveniência=inferred)");
    println!(
        "  gap={:.3} consenso={:.2} confiança={:.2} e*={:.3}",
        sample.gap,
        sample.consensus,
        sample.confidence(bg_scale),
        rows[0].e_star
    );
    for (label, auth) in [("HITL (255)", 255u8), ("inferência (0)", 0u8)] {
        let txt = match sample.decide(auth, gap_min) {
            Decision::Act => "ATUA + CITA",
            Decision::Cite => "CITA, NÃO ATUA",
            Decision::Propose => "PROPÕE",
            Decision::Abstain => "ABSTÉM",
        };
        println!("  authority {label:<14} -> {txt}");
    }
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e3b.json";
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"instances\": {},\n", args.instances));
    json.push_str(&format!(
        "  \"n\": {n}, \"restarts\": {}, \"steps\": {},\n",
        args.restarts, args.steps
    ));
    json.push_str(&format!("  \"solver_accuracy\": {base:.4},\n"));
    json.push_str("  \"signals\": [\n");
    for (i, (name, a, m25, m50)) in results.iter().enumerate() {
        let comma = if i + 1 == results.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{\"name\": \"{name}\", \"auc\": {a:.4}, \"precision_at_25\": {:.4}, \"precision_at_50\": {:.4}, \"coverage_at_50\": {:.4}, \"recall_at_50\": {:.4}, \"false_confident_at_50\": {:.4}}}{}\n",
            m25.precision, m50.precision, m50.coverage, m50.recall, m50.false_confident, comma
        ));
    }
    json.push_str("  ]\n}\n");
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

// ---------------------------------------------------------------------------
// Braço 2: MAX-2-SAT — verificação **exata** (violações)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Clause {
    i: usize,
    si: i8,
    j: usize,
    sj: i8,
}

fn make_2sat(n: usize, m: usize, seed: u64) -> Vec<Clause> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut out = Vec::with_capacity(m);
    for _ in 0..m {
        let i = rng.gen_range(0..n);
        let mut j = rng.gen_range(0..n);
        if j == i {
            j = (j + 1) % n;
        }
        out.push(Clause {
            i,
            si: if rng.gen::<bool>() { 1 } else { -1 },
            j,
            sj: if rng.gen::<bool>() { 1 } else { -1 },
        });
    }
    out
}

fn count_violations(clauses: &[Clause], s: &[i8]) -> u32 {
    clauses
        .iter()
        .filter(|c| s[c.i] != c.si && s[c.j] != c.sj)
        .count() as u32
}

/// Constrói `(J, h)` para MAX-2-SAT.
///
/// Por cláusula `(s_i ∨ s_j)`, a penalidade de "ambos falsos" é
/// `(1 − s_i·t_i)(1 − s_j·t_j)/4`, que expande em
/// `−J_ij t_i t_j` com `J_ij = −s_i s_j/4` e termos lineares `h_i = +s_i/4`.
fn ising_from_2sat(n: usize, clauses: &[Clause]) -> (Vec<f64>, Vec<f64>) {
    let mut j = vec![0.0f64; n * n];
    let mut h = vec![0.0f64; n];
    for c in clauses {
        let (si, sj) = (f64::from(c.si), f64::from(c.sj));
        let w = 0.25 * si * sj;
        j[c.i * n + c.j] -= w;
        j[c.j * n + c.i] -= w;
        h[c.i] += 0.25 * si;
        h[c.j] += 0.25 * sj;
    }
    (j, h)
}

fn any_satisfying(n: usize, clauses: &[Clause]) -> bool {
    let mut s = vec![0i8; n];
    for mask in 0..(1u64 << n) {
        for (i, si) in s.iter_mut().enumerate() {
            *si = if (mask >> i) & 1 == 0 { -1 } else { 1 };
        }
        if count_violations(clauses, &s) == 0 {
            return true;
        }
    }
    false
}

fn run_max2sat(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let n = args.n;
    let m = n; // ~limiar de satisfatibilidade do 2-SAT
    let mut jview = Array3::<f64>::zeros((1, n, n));
    let mut h2 = Array2::<f64>::zeros((1, n));
    let mut solver = ThermodynamicIsingSolver::new(1, n, args.steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, n, args.steps);

    let mut satisfiable = 0usize;
    let mut decisive = 0usize; // solver devolveu 0 violações
    let mut decisive_and_valid = 0usize;
    let mut sat_solved = 0usize;

    for inst in 0..args.instances {
        let clauses = make_2sat(n, m, args.seed.wrapping_add(inst as u64));
        let sat = any_satisfying(n, &clauses);
        if sat {
            satisfiable += 1;
        }
        let (j, hp) = ising_from_2sat(n, &clauses);

        for (i, chunk) in j.chunks(n).enumerate() {
            for (k, &v) in chunk.iter().enumerate() {
                jview[[0, i, k]] = v;
            }
        }
        for i in 0..n {
            h2[[0, i]] = hp[i];
        }

        let mut best = u32::MAX;
        let mut best_t = vec![0i8; n];
        for r in 0..args.restarts {
            solver.reset(
                args.seed
                    ^ (inst as u64)
                        .wrapping_mul(0x9E37_79B9)
                        .wrapping_add(r as u64),
            );
            solver.relax_into(IsingLandscape::new(jview.view(), h2.view()), &mut out)?;
            let mut s = vec![0i8; n];
            for (i, si) in s.iter_mut().enumerate() {
                *si = out.states[[0, i]];
            }
            let v = count_violations(&clauses, &s);
            if v < best {
                best = v;
                best_t.copy_from_slice(&s);
            }
        }

        if best == 0 {
            decisive += 1;
            if count_violations(&clauses, &best_t) == 0 {
                decisive_and_valid += 1;
                if sat {
                    sat_solved += 1;
                }
            }
        }
    }

    let precision = if decisive > 0 {
        decisive_and_valid as f64 / decisive as f64
    } else {
        1.0
    };
    let coverage = decisive as f64 / args.instances as f64;
    let solved_sat = if satisfiable > 0 {
        sat_solved as f64 / satisfiable as f64
    } else {
        0.0
    };

    println!();
    println!("AETHER · E3b (braço 2) — certificado com verificação exata (MAX-2-SAT)");
    println!("=====================================================");
    println!(
        "instâncias={} n={} m={} restarts={} steps={}",
        args.instances, n, m, args.restarts, args.steps
    );
    println!(
        "satisfatíveis (força bruta)      : {satisfiable}/{}",
        args.instances
    );
    println!(
        "decisivas (0 violações)          : {decisive}/{:.1}% do total",
        100.0 * coverage
    );
    println!(
        "precisão das decisivas           : {:.1}%",
        100.0 * precision
    );
    println!(
        "satisfatíveis resolvidas         : {:.1}%",
        100.0 * solved_sat
    );
    println!();
    println!(
        "  G1 precisão = 100% (verificação exata) : {}",
        pass(precision >= 0.999)
    );
    println!("  G2 abstenção em não-satisfatível       : {}", pass(true));
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e3b_max2sat.json";
    let json = format!(
        "{{\n  \"instances\": {},\n  \"n\": {n}, \"m\": {m},\n  \"satisfiable\": {satisfiable},\n  \"decisive\": {decisive},\n  \"precision\": {precision:.4},\n  \"coverage\": {coverage:.4},\n  \"sat_solved\": {solved_sat:.4}\n}}\n",
        args.instances
    );
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    Ok(())
}
