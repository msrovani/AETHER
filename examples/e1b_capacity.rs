//! E1b — quantos padrões cabem em D=160?
//!
//! Duas frentes, porque "padrão" tem dois significados no projeto:
//!
//! * **Arm A — store FHRR** (`FhrrStore`): fatos `e ⊗ r ⊗ v` superpostos.
//!   Mede `recall@1` vs número de fatos, para `D ∈ {64,160,320,640}` e
//!   `K ∈ {1,6}` traços. Responde à pergunta do dossiê (Vol. III §12.5 #3).
//! * **Arm B — memória de fase** (`ComplexPhaseMemory`): atratores de fase.
//!   Mede o erro angular de recall com 40% da pista oculta vs nº de padrões.
//!
//! ```text
//! cargo run --release --example e1b_capacity
//! cargo run --release --example e1b_capacity -- --quick
//! ```
//!
//! Saída: tabelas + `target/aether_e1b.json`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

use aether_engine::core::fhrr::FhrrStore;
use aether_engine::phase_memory::ComplexPhaseMemory;

const DIMS: [usize; 4] = [64, 160, 320, 640];
const TRACES: [usize; 2] = [1, 6];
const QUERIES: usize = 200;
const TARGET_RECALL: f64 = 0.99;

struct Args {
    quick: bool,
    dim: usize,
    seed: u64,
}

fn parse_args() -> Args {
    let mut a = Args {
        quick: false,
        dim: 160,
        seed: 0xE1B_0001,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--quick" => a.quick = true,
            "--dim" => {
                if let Some(v) = argv.get(i + 1).and_then(|v| v.parse().ok()) {
                    a.dim = v;
                    i += 1;
                }
            }
            "--seed" => {
                if let Some(v) = argv.get(i + 1).and_then(|v| v.parse().ok()) {
                    a.seed = v;
                    i += 1;
                }
            }
            other => eprintln!("argumento ignorado: {other}"),
        }
        i += 1;
    }
    a
}

fn random_phases(dim: usize, rng: &mut ChaCha8Rng) -> Vec<f32> {
    (0..dim).map(|_| rng.gen::<f32>() * TAU).collect()
}

/// Arm A: `recall@1` para um store com `m` fatos.
fn fhrr_recall(dim: usize, traces: usize, m: usize, seed: u64) -> (f64, f32) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut store = FhrrStore::with_traces(dim, traces).expect("store");
    let relation = random_phases(dim, &mut rng);

    let mut entities = Vec::with_capacity(m);
    for _ in 0..m {
        let e = random_phases(dim, &mut rng);
        let v = random_phases(dim, &mut rng);
        store.add_fact(&e, &relation, &v).expect("add_fact");
        entities.push(e);
    }

    let mask = vec![true; dim];
    let n_q = m.min(QUERIES);
    let mut hits = 0usize;
    let mut sim_sum = 0.0f32;
    for k in 0..n_q {
        let idx = if m <= QUERIES { k } else { (k * m) / n_q };
        let hit = store
            .query_match(&entities[idx], &relation, &mask)
            .expect("query");
        if hit.value_index == idx {
            hits += 1;
        }
        sim_sum += hit.similarity;
    }
    (hits as f64 / n_q as f64, sim_sum / n_q as f32)
}

/// Maior `m` com `recall@1 ≥ TARGET_RECALL`, varrendo potências de dois.
fn fhrr_capacity(dim: usize, traces: usize, seed: u64, cap: usize) -> (usize, Vec<(usize, f64)>) {
    let mut curve = Vec::new();
    let mut best = 0usize;
    let mut m = 8usize;
    while m <= cap {
        let (recall, _) = fhrr_recall(dim, traces, m, seed);
        curve.push((m, recall));
        if recall >= TARGET_RECALL {
            best = m;
        }
        m *= 2;
    }
    (best, curve)
}

fn wrapped_abs(a: f32, b: f32) -> f32 {
    let mut d = (a - b) % TAU;
    if d > std::f32::consts::PI {
        d -= TAU;
    }
    if d < -std::f32::consts::PI {
        d += TAU;
    }
    d.abs()
}

/// Arm B: erro angular de recall com 40% da pista oculta.
fn phase_memory_error(n: usize, patterns: usize, seed: u64) -> f32 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut mem = ComplexPhaseMemory::new(n);
    let mask: Vec<bool> = (0..n).map(|i| i % 5 != 0).collect(); // 20% oculto

    let mut stored: Vec<Vec<f32>> = Vec::with_capacity(patterns);
    for _ in 0..patterns {
        let p: Vec<f32> = (0..n).map(|_| rng.gen::<f32>() * TAU).collect();
        mem.imprint(&p, 40);
        stored.push(p);
    }

    let mut err_sum = 0.0f32;
    let mut count = 0usize;
    for p in &stored {
        let (out, _) = mem.recall(p, &mask, 200, 0.05);
        for i in 0..n {
            if !mask[i] {
                err_sum += wrapped_abs(out[i], p[i]);
                count += 1;
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        err_sum / count as f32
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let dims: &[usize] = if args.quick { &[64, 160] } else { &DIMS };
    let traces_list: &[usize] = if args.quick { &[6] } else { &TRACES };
    let cap = if args.quick { 512 } else { 2048 };

    println!();
    println!("AETHER · E1b — capacidade de memória");
    println!("=====================================================");
    println!(
        "dims={:?} traces={:?} queries={} alvo recall@1 ≥ {:.0}%",
        dims,
        traces_list,
        QUERIES,
        100.0 * TARGET_RECALL
    );
    println!();

    // ---------------- Arm A ----------------
    println!("Arm A — store FHRR: fatos e⊗r⊗v superpostos");
    println!(
        "{:>5} {:>7} {:>12} {:>12} {:>12}",
        "D", "traços", "capacidade", "M/D", "M/D²"
    );
    println!("{}", "-".repeat(54));

    let mut results: Vec<(usize, usize, usize, f64, f64)> = Vec::new();
    for &d in dims {
        for &k in traces_list {
            let (capacity, curve) = fhrr_capacity(d, k, args.seed, cap);
            let ratio_d = capacity as f64 / d as f64;
            let ratio_d2 = capacity as f64 / (d * d) as f64;
            println!(
                "{:>5} {:>7} {:>12} {:>12.3} {:>12.4}",
                d, k, capacity, ratio_d, ratio_d2
            );
            results.push((d, k, capacity, ratio_d, ratio_d2));

            // curva detalhada só para o caso de referência (D=160, K=6)
            if d == 160 && k == 6 {
                println!("  curva (D=160, K=6):");
                for (m, r) in &curve {
                    println!("    M={m:>5}  recall@1={:.3}", r);
                }
            }
        }
    }
    println!();

    // lei de escala: inclinação log-log entre D min e max, para K=6
    let k_ref = 6usize;
    let mut slope = f64::NAN;
    if dims.len() >= 2 {
        let d0 = dims[0];
        let d1 = *dims.last().unwrap();
        let c0 = results.iter().find(|r| r.0 == d0 && r.1 == k_ref).map(|r| r.2);
        let c1 = results.iter().find(|r| r.0 == d1 && r.1 == k_ref).map(|r| r.2);
        if let (Some(c0), Some(c1)) = (c0, c1) {
            if c0 > 0 && c1 > 0 && d1 > d0 {
                slope = ((c1 as f64) / (c0 as f64)).ln() / ((d1 as f64) / (d0 as f64)).ln();
            }
        }
    }
    println!("lei de escala medida (K=6): capacidade ∝ D^{slope:.2}");
    println!();

    // ---------------- Arm B ----------------
    println!("Arm B — memória de fase (M5): erro angular com 20% oculto");
    let n = args.dim;
    let mut mem_ref = ComplexPhaseMemory::new(n);
    let _ = &mut mem_ref;
    println!(
        "{:>5} {:>10} {:>12} {:>12}",
        "n", "padrões", "capacidade*", "erro rad"
    );
    println!("{}", "-".repeat(44));
    let pattern_counts: &[usize] = if args.quick {
        &[4, 8, 16]
    } else {
        &[4, 8, 16, 32, 64]
    };
    let mut phase_errs: Vec<(usize, f32)> = Vec::new();
    for &k in pattern_counts {
        let err = phase_memory_error(n, k, args.seed);
        phase_errs.push((k, err));
        println!("{:>5} {:>10} {:>12} {:>12.3}", n, k, "—", err);
    }
    println!("  (*) `ComplexPhaseMemory::pattern_capacity()` = {}", mem_ref.pattern_capacity());
    println!();

    // ---------------- veredito ----------------
    let cap160 = results
        .iter()
        .find(|r| r.0 == 160 && r.1 == 6)
        .map(|r| r.2);
    println!("veredito");
    match cap160 {
        Some(c) => println!("  capacidade medida em D=160, K=6 : {c} fatos"),
        None => println!("  D=160 não medido neste modo"),
    }
    println!("  lei de escala (K=6)             : D^{slope:.2}");
    let err64 = phase_errs.last().map(|x| x.1);
    if let Some(e) = err64 {
        println!("  memória de fase (último M)      : erro {e:.3} rad");
    }
    println!();

    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e1b.json";
    let mut json = String::new();
    json.push_str("{\n  \"fhrr\": [\n");
    for (i, (d, k, c, rd, rd2)) in results.iter().enumerate() {
        let comma = if i + 1 == results.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{\"dim\": {d}, \"traces\": {k}, \"capacity\": {c}, \"m_over_d\": {rd:.4}, \"m_over_d2\": {rd2:.5}}}{comma}\n"
        ));
    }
    json.push_str("  ],\n");
    json.push_str(&format!("  \"slope_loglog\": {slope:.4},\n"));
    json.push_str("  \"phase_memory\": [\n");
    for (i, (k, e)) in phase_errs.iter().enumerate() {
        let comma = if i + 1 == phase_errs.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{\"n\": {n}, \"patterns\": {k}, \"angular_error\": {e:.5}}}{comma}\n"
        ));
    }
    json.push_str("  ]\n}\n");
    std::fs::write(path, json)?;
    println!("artefato: {path}");
    Ok(())
}

