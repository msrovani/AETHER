//! E2d — aquisição CRAM-style de restrições → Logos (`Certificate`).
//!
//! Metodologia:
//! 1. Soluções 4×4 sintéticas (Shidoku) com seed fixa; pistas por máscara;
//! 2. **Braço A** — energia hand-set (custos axiomaticos de unicidade);
//! 3. **Braço B** — treino pseudolikelihood de `K` compartilhado (sem BPTT);
//! 4. Ambos resolvem via `pairwise_to_qubo` → `BsbCore` → oráculo de violações;
//! 5. Gates G1–G3; artefato JSON em `target/aether_cram_logos.json`.
//!
//! ```text
//! cargo run --release --example cram_logos_probe
//! cargo run --release --example cram_logos_probe -- --quick
//! cargo run --release --example cram_logos_probe -- --full
//! ```
//!
//! Ver `docs/CRAM_LOGOS.md`.

use std::env;
use std::fs;
use std::time::Instant;

use aether_engine::core::certificate::Provenance;
use aether_engine::core::constraint_acquisition::{
    solve_with_certificate, train_pseudolikelihood, PairwiseEnergy,
};
use aether_engine::core::logic_pipeline::BsbCore;
use aether_engine::core::sudoku_csp::{
    shidoku_instances, shidoku_solutions, SudokuShape,
};

#[derive(Clone, Copy)]
struct Args {
    instances: usize,
    clues: usize,
    restarts: usize,
    pl_steps: usize,
    quick: bool,
    full: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        instances: 24,
        clues: 8,
        restarts: 8,
        pl_steps: 400,
        quick: false,
        full: false,
    };
    let mut it = env::args().skip(1);
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--quick" => {
                a.quick = true;
                a.instances = 8;
                a.restarts = 4;
                a.pl_steps = 120;
            }
            "--full" => {
                a.full = true;
                a.instances = 40;
                a.clues = 6;
                a.restarts = 12;
                a.pl_steps = 800;
            }
            "--instances" => {
                if let Some(v) = it.next() {
                    a.instances = v.parse().unwrap_or(a.instances);
                }
            }
            "--clues" => {
                if let Some(v) = it.next() {
                    a.clues = v.parse().unwrap_or(a.clues);
                }
            }
            "--help" | "-h" => {
                eprintln!(
                    "cram_logos_probe [--quick|--full] [--instances N] [--clues K]"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }
    a
}

struct ArmStats {
    name: &'static str,
    feasible: usize,
    total: usize,
    coherent: usize,
    mean_viol: f64,
    fit_ms: f64,
    params: usize,
}

impl ArmStats {
    fn feasible_pct(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            100.0 * self.feasible as f64 / self.total as f64
        }
    }
}

fn eval_arm(
    name: &'static str,
    energy: &PairwiseEnergy,
    instances: &[(aether_engine::core::sudoku_csp::SudokuBoard, aether_engine::core::sudoku_csp::SudokuBoard)],
    restarts: usize,
    provenance: Provenance,
    fit_ms: f64,
) -> ArmStats {
    let n_spins = energy.shape.n_spins();
    let mut core = BsbCore::new(n_spins, 200, 0.05);
    let mut feasible = 0usize;
    let mut coherent = 0usize;
    let mut viol_sum = 0u64;
    for (k, (puzzle, _sol)) in instances.iter().enumerate() {
        let (_board, cert, _) = solve_with_certificate(
            energy,
            puzzle,
            &mut core,
            restarts,
            0xC2AMu64.wrapping_add(k as u64),
            provenance,
        );
        viol_sum += u64::from(cert.violations);
        if cert.violations == 0 {
            feasible += 1;
        }
        if cert.is_coherent() {
            coherent += 1;
        }
    }
    ArmStats {
        name,
        feasible,
        total: instances.len(),
        coherent,
        mean_viol: if instances.is_empty() {
            0.0
        } else {
            viol_sum as f64 / instances.len() as f64
        },
        fit_ms,
        params: energy.n_params(),
    }
}

fn main() {
    let args = parse_args();
    let shape = SudokuShape::new(4).expect("4×4");
    let instances = shidoku_instances(args.instances, args.clues, 0xAETHER);
    let train_sols = shidoku_solutions();

    println!("=== E2d CRAM→Logos (Shidoku 4×4) ===");
    println!(
        "instances={} clues={} restarts={} pl_steps={} quick={} full={}",
        args.instances, args.clues, args.restarts, args.pl_steps, args.quick, args.full
    );
    println!("train_solutions={} n_spins={}", train_sols.len(), shape.n_spins());

    // Braço A — hand-set
    let hand = PairwiseEnergy::hand_set(shape, 2.5);
    let arm_a = eval_arm(
        "hand_set",
        &hand,
        &instances,
        args.restarts,
        Provenance::Axiomatic,
        0.0,
    );

    // Braço B — PL
    let mut learned = PairwiseEnergy::zeros(shape);
    let t0 = Instant::now();
    train_pseudolikelihood(&mut learned, &train_sols, args.pl_steps, 0.08);
    let fit_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let arm_b = eval_arm(
        "pl_learned",
        &learned,
        &instances,
        args.restarts,
        Provenance::Inferred,
        fit_ms,
    );

    println!();
    println!(
        "{:<12} {:>8} {:>10} {:>10} {:>10} {:>8}",
        "arm", "feas%", "coherent", "mean_viol", "fit_ms", "params"
    );
    for arm in [&arm_a, &arm_b] {
        println!(
            "{:<12} {:>7.1}% {:>10} {:>10.3} {:>10.2} {:>8}",
            arm.name,
            arm.feasible_pct(),
            arm.coherent,
            arm.mean_viol,
            arm.fit_ms,
            arm.params
        );
    }

    // Gates
    // G1: hand-set ≥ 95% factível
    let g1 = arm_a.feasible_pct() >= 95.0;
    // G2: learned ≥ hand-set − 15 pp (ou ≥ 80%)
    let g2 = arm_b.feasible_pct() + 15.0 >= arm_a.feasible_pct()
        || arm_b.feasible_pct() >= 80.0;
    // G3: quando factível, coerência do certificado não deve ser pior que factível
    // (is_coherent exige gap>0; toleramos coherent ≥ 50% dos factíveis)
    let g3 = if arm_b.feasible == 0 {
        false
    } else {
        arm_b.coherent as f64 >= 0.5 * arm_b.feasible as f64
    };

    println!();
    println!("G1 hand_set feas≥95%:     {}", if g1 { "PASS" } else { "FAIL" });
    println!("G2 learned vs hand_set:   {}", if g2 { "PASS" } else { "FAIL" });
    println!("G3 cert coherent@feas:    {}", if g3 { "PASS" } else { "FAIL" });

    let json = format!(
        r#"{{
  "experiment": "E2d",
  "arxiv": "2611.17309v1",
  "shape": "4x4",
  "instances": {},
  "clues": {},
  "restarts": {},
  "pl_steps": {},
  "hand_set": {{
    "feasible_pct": {:.4},
    "feasible": {},
    "total": {},
    "coherent": {},
    "mean_violations": {:.6},
    "params": {}
  }},
  "pl_learned": {{
    "feasible_pct": {:.4},
    "feasible": {},
    "total": {},
    "coherent": {},
    "mean_violations": {:.6},
    "fit_ms": {:.4},
    "params": {}
  }},
  "gates": {{
    "G1_hand_set_feas": {},
    "G2_learned_vs_hand": {},
    "G3_cert_coherent": {}
  }},
  "pass": {}
}}
"#,
        args.instances,
        args.clues,
        args.restarts,
        args.pl_steps,
        arm_a.feasible_pct(),
        arm_a.feasible,
        arm_a.total,
        arm_a.coherent,
        arm_a.mean_viol,
        arm_a.params,
        arm_b.feasible_pct(),
        arm_b.feasible,
        arm_b.total,
        arm_b.coherent,
        arm_b.mean_viol,
        arm_b.fit_ms,
        arm_b.params,
        g1,
        g2,
        g3,
        g1 && g2 && g3
    );

    let _ = fs::create_dir_all("target");
    let out = "target/aether_cram_logos.json";
    if let Err(e) = fs::write(out, &json) {
        eprintln!("warn: não escreveu {out}: {e}");
    } else {
        println!("\nJSON → {out}");
    }

    if !(g1 && g2 && g3) {
        std::process::exit(1);
    }
}
