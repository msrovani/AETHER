//! E3b — o `ΔE` prediz acerto? Calibração do certificado de coerência.
//!
//! Metodologia:
//! 1. gera `--instances` instâncias de **vidro de spin** (MAX-CUT com pesos ±1,
//!    altamente frustrado) — `H(s) = -0.5·sᵀJs`, `h = 0`;
//! 2. determina o **ótimo global por força bruta** (ground truth exato);
//! 3. roda o BSB com `--restarts` reinicializações e coleta o melhor estado;
//! 4. emite o [`Certificate`] — energia, gap local, violações, consenso e proveniência;
//! 5. mede se o atestado **prediz acerto**: AUC, precisão, taxa de "confiante e
//!    errado" e ECE, em split treino/teste (threshold escolhido só no treino).
//!
//! ```text
//! cargo run --release --example e3b_certificate
//! cargo run --release --example e3b_certificate -- --instances 600 --n 16 --steps 20
//! ```
//!
//! Saída: tabela + `target/aether_e3b.json`.

use ndarray::{Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::certificate::{
    energy, local_gap_with, Certificate, Decision, Provenance,
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
}

impl Default for Args {
    fn default() -> Self {
        Self {
            instances: 400,
            n: 14,
            restarts: 8,
            steps: 24, // curto de propósito: induz uma mistura de acertos e erros
            seed: 0xE3B_0001,
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
            (k, _) => {
                eprintln!("argumento ignorado: {k}");
            }
        }
        i += 1;
    }
    a
}

/// Instância: `J` simétrica, diagonal zero, pesos ±1 (spin glass frustrado).
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

/// Ótimo global por força bruta (2^n estados). `h = 0`.
fn brute_force(j: &[f64], h: &[f64], n: usize) -> f64 {
    let mut best = f64::INFINITY;
    let mut s = vec![0i8; n];
    let total: u64 = 1u64 << n;
    for mask in 0..total {
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

/// AUC de um score (maior = mais provável correto), com empates por rank médio.
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
struct Metrics {
    threshold: f64,
    precision: f64,
    coverage: f64,
    recall: f64,
    false_confident: f64,
}

fn choose_threshold(train: &[(f64, bool)]) -> f64 {
    let mut cands: Vec<f64> = train.iter().map(|x| x.0).collect();
    cands.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cands.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
    let mut best_t = cands.first().copied().unwrap_or(f64::INFINITY);
    let mut best_f = -1.0;
    for t in cands {
        let (mut tp, mut fp, mut pos) = (0.0, 0.0, 0.0);
        for &(s, c) in train {
            if c {
                pos += 1.0;
            }
            if s >= t {
                if c {
                    tp += 1.0
                } else {
                    fp += 1.0
                }
            }
        }
        if tp + fp == 0.0 {
            continue;
        }
        let precision = tp / (tp + fp);
        let recall = if pos > 0.0 { tp / pos } else { 0.0 };
        // F0.5: precisão pesa 4× mais que recall (queremos confiança barata e segura)
        let denom = 0.25 * precision + recall;
        let f = if denom > 0.0 {
            1.25 * precision * recall / denom
        } else {
            0.0
        };
        if f > best_f {
            best_f = f;
            best_t = t;
        }
    }
    best_t
}

fn evaluate(test: &[(f64, bool)], threshold: f64) -> Metrics {
    let (mut tp, mut fp, mut pos, mut conf) = (0.0, 0.0, 0.0, 0.0);
    for &(s, c) in test {
        if c {
            pos += 1.0;
        }
        if s >= threshold {
            conf += 1.0;
            if c {
                tp += 1.0
            } else {
                fp += 1.0
            }
        }
    }
    let n = test.len() as f64;
    let precision = if conf > 0.0 { tp / conf } else { 0.0 };
    Metrics {
        threshold,
        precision,
        coverage: conf / n,
        recall: if pos > 0.0 { tp / pos } else { 0.0 },
        false_confident: if conf > 0.0 { fp / conf } else { 0.0 },
    }
}

/// ECE em 10 bins sobre confiança em `0..=1`.
fn ece(pairs: &[(f64, bool)]) -> f64 {
    if pairs.is_empty() {
        return f64::NAN;
    }
    let mut bins = [(0.0f64, 0.0f64); 10];
    for &(c, ok) in pairs {
        let idx = ((c.clamp(0.0, 1.0)) * 9.999) as usize;
        bins[idx].0 += c;
        bins[idx].1 += if ok { 1.0 } else { 0.0 };
    }
    let n = pairs.len() as f64;
    let mut acc = 0.0;
    let mut counts = [0.0f64; 10];
    for &(c, ok) in pairs {
        let idx = ((c.clamp(0.0, 1.0)) * 9.999) as usize;
        counts[idx] += 1.0;
        let _ = (c, ok);
    }
    for b in 0..10 {
        if counts[b] == 0.0 {
            continue;
        }
        let conf = bins[b].0 / counts[b];
        let acc_b = bins[b].1 / counts[b];
        acc += (counts[b] / n) * (conf - acc_b).abs();
    }
    acc
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n = args.n;
    let h = vec![0.0f64; n];

    // --- coleta ---
    let mut rows: Vec<(f64, f64, bool, f64, f64)> = Vec::with_capacity(args.instances);
    // (gap, consensus, correct, energy, best_energy_restarts)

    let j3 = Array3::<f64>::zeros((1, n, n));
    let h2 = Array2::<f64>::zeros((1, n));
    let mut solver = ThermodynamicIsingSolver::new(1, n, args.steps, 0.05);
    let mut out = RelaxOutcome::with_capacity(1, n, args.steps);
    let mut jview = j3;
    let mut g = vec![0.0f64; n];
    let mut s = vec![0i8; n];

    for inst in 0..args.instances {
        let j = make_instance(n, args.seed.wrapping_add(inst as u64));
        let e_star = brute_force(&j, &h, n);

        // copia J para a view usada pelo solver
        for (i, chunk) in j.chunks(n).enumerate() {
            for (k, &v) in chunk.iter().enumerate() {
                jview[[0, i, k]] = v;
            }
        }

        let mut e_best = f64::INFINITY;
        let mut best_states = vec![0i8; n];
        let mut energies: Vec<f64> = Vec::with_capacity(args.restarts);
        for r in 0..args.restarts {
            solver.reset(args.seed ^ (inst as u64).wrapping_mul(0x9E37).wrapping_add(r as u64));
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

        s.copy_from_slice(&best_states);
        let gap = local_gap_with(&j, &h, n, &s, &mut g);
        let correct = (e_best - e_star).abs() < 1e-9;

        rows.push((gap, consensus, correct, e_best, e_star));
    }

    // --- split treino/teste (par/ímpar: determinístico e balanceado) ---
    let mut train_gap = Vec::new();
    let mut test_gap = Vec::new();
    let mut train_cons = Vec::new();
    let mut test_cons = Vec::new();
    let mut train_conf = Vec::new();
    let mut test_conf = Vec::new();

    let gaps: Vec<f64> = rows.iter().map(|r| r.0).collect();
    let mut sorted_gaps = gaps.clone();
    sorted_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let gap_scale = sorted_gaps[sorted_gaps.len() / 2].max(1e-9);

    for (idx, r) in rows.iter().enumerate() {
        let (gap, cons, correct, _, _) = *r;
        let conf = (((gap / gap_scale).min(1.0)) * cons).sqrt();
        if idx % 2 == 0 {
            train_gap.push((gap, correct));
            train_cons.push((cons, correct));
            train_conf.push((conf, correct));
        } else {
            test_gap.push((gap, correct));
            test_cons.push((cons, correct));
            test_conf.push((conf, correct));
        }
    }

    let t_gap = choose_threshold(&train_gap);
    let t_cons = choose_threshold(&train_cons);
    let t_conf = choose_threshold(&train_conf);

    let m_gap = evaluate(&test_gap, t_gap);
    let m_cons = evaluate(&test_cons, t_cons);
    let m_conf = evaluate(&test_conf, t_conf);

    let auc_gap = auc(&test_gap);
    let auc_cons = auc(&test_cons);
    let auc_conf = auc(&test_conf);
    let ece_conf = ece(&test_conf);

    let n_correct = rows.iter().filter(|r| r.2).count();
    let accuracy = n_correct as f64 / rows.len() as f64;

    // --- relatório ---
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
        100.0 * accuracy
    );
    println!();
    println!(
        "{:<12} {:>8} {:>10} {:>10} {:>10} {:>10}",
        "sinal", "AUC", "limiar", "precisão", "cobertura", "conf.err"
    );
    println!("{}", "-".repeat(66));
    for (name, a, m) in [
        ("gap local", auc_gap, m_gap),
        ("consenso", auc_cons, m_cons),
        ("confiança", auc_conf, m_conf),
    ] {
        println!(
            "{:<12} {:>8.3} {:>10.3} {:>9.1}% {:>9.1}% {:>9.1}%",
            name,
            a,
            m.threshold,
            100.0 * m.precision,
            100.0 * m.coverage,
            100.0 * m.false_confident
        );
    }
    println!("{}", "-".repeat(66));
    println!();
    println!("ECE (confiança combinada): {:.4}", ece_conf);
    println!();

    // --- veredito ---
    let best = [
        ("gap local", auc_gap, m_gap),
        ("consenso", auc_cons, m_cons),
        ("confiança", auc_conf, m_conf),
    ]
    .into_iter()
    .filter(|(_, a, _)| a.is_finite())
    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let (name, a, m) = best.unwrap_or(("n/a", f64::NAN, m_gap));
    let gate_auc = a >= 0.85;
    let gate_prec = m.precision >= 0.95;
    let gate_false = m.false_confident < 0.05;
    println!("melhor sinal: {name} (AUC {a:.3})");
    println!("  G-AUC  (AUC ≥ 0,85)                 : {}", pass(gate_auc));
    println!("  G-PREC (precisão ≥ 95%)             : {}", pass(gate_prec));
    println!("  G-FC   (confiante-e-errado < 5%)    : {}", pass(gate_false));
    println!();

    // --- demonstração da matriz 2D (Vol. III §6) ---
    let sample = Certificate::new(
        rows[0].3,
        rows[0].0,
        0,
        rows[0].1,
        Provenance::Inferred,
    );
    let gap_min = t_gap.max(1e-9);
    println!("matriz 2D (exemplo da 1ª instância, proveniência=inferred)");
    println!("  gap={:.3} consenso={:.2} confiança={:.2}", sample.gap, sample.consensus, sample.confidence(gap_scale));
    for (label, auth) in [("HITL (255)", 255u8), ("inferência (0)", 0u8)] {
        let d = sample.decide(auth, gap_min);
        let txt = match d {
            Decision::Act => "ATUA + CITA",
            Decision::Cite => "CITA, NÃO ATUA",
            Decision::Propose => "PROPÕE",
            Decision::Abstain => "ABSTÉM",
        };
        println!("  authority {label:<14} -> {txt}");
    }
    println!();

    // --- JSON ---
    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e3b.json";
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"instances\": {},\n", args.instances));
    json.push_str(&format!("  \"n\": {n}, \"restarts\": {}, \"steps\": {},\n", args.restarts, args.steps));
    json.push_str(&format!("  \"solver_accuracy\": {accuracy:.4},\n"));
    json.push_str(&format!("  \"gap_scale\": {gap_scale:.6},\n"));
    json.push_str(&format!(
        "  \"gap\": {{\"auc\": {auc_gap:.4}, \"threshold\": {:.6}, \"precision\": {:.4}, \"coverage\": {:.4}, \"recall\": {:.4}, \"false_confident\": {:.4}}},\n",
        m_gap.threshold, m_gap.precision, m_gap.coverage, m_gap.recall, m_gap.false_confident
    ));
    json.push_str(&format!(
        "  \"consensus\": {{\"auc\": {auc_cons:.4}, \"threshold\": {:.6}, \"precision\": {:.4}, \"coverage\": {:.4}, \"recall\": {:.4}, \"false_confident\": {:.4}}},\n",
        m_cons.threshold, m_cons.precision, m_cons.coverage, m_cons.recall, m_cons.false_confident
    ));
    json.push_str(&format!(
        "  \"confidence\": {{\"auc\": {auc_conf:.4}, \"threshold\": {:.6}, \"precision\": {:.4}, \"coverage\": {:.4}, \"recall\": {:.4}, \"false_confident\": {:.4}, \"ece\": {ece_conf:.4}}}\n",
        m_conf.threshold, m_conf.precision, m_conf.coverage, m_conf.recall, m_conf.false_confident
    ));
    json.push_str("}\n");
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
