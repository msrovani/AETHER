//! T12 auditavel: latencia do RAG holografico em percentis reais.
//!
//! Protocolo fixo e declarado (para o numero ser reproduzivel):
//! * `FACTS = 50` fatos, cada um `(e, r, v)` com fases aleatorias uniformes;
//! * `K` tracos por fato (`FhrrStore::with_traces(D, K)`);
//! * `40%` da dimensao oculta (`mask`), ruido `+-0.25 rad` nas posicoes visiveis
//!   (metade em `e`, metade em `r`, oposto em sinal, como na auditoria);
//! * `QUERIES` consultas por configuracao, cronometradas **todas** com
//!   `query_into` sobre workspace pre-alocado (nenhum descarte de amostra).
//!
//! Alem dos percentis, tambem reproduz o criterio da auditoria
//! (`melhor de 6 x 20 consultas`) para explicar a divergencia entre os
//! relatorios: 5-7 us, 13.5 us e 15.7 us saem do **mesmo** codigo, de
//! estatisticas diferentes.
//!
//! Execucao: `cargo bench --bench t12_probe`

use std::time::Instant;

use aether_engine::core::fhrr::FhrrStore;

const FACTS: usize = 50;
const QUERIES: usize = 2000;

/// xorshift64* : PRNG deterministico, sem dependencia.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn phases(&mut self, n: usize) -> Vec<f32> {
        (0..n)
            .map(|_| self.unit() * std::f32::consts::TAU)
            .collect()
    }

    fn mask(&mut self, d: usize, hidden: usize) -> Vec<bool> {
        let mut mask = vec![true; d];
        let mut hidden_left = hidden;
        while hidden_left > 0 {
            let i = (self.next_u64() as usize) % d;
            if mask[i] {
                mask[i] = false;
                hidden_left -= 1;
            }
        }
        mask
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn run(d: usize, k: usize, label: &str) {
    let mut rng = Rng::new(0xFAC7_0000 ^ (d as u64) << 8 | k as u64);
    let mut store = FhrrStore::with_traces(d, k).expect("fhrr");
    let mut atoms: Vec<(Vec<f32>, Vec<f32>, Vec<f32>)> = Vec::with_capacity(FACTS);
    for _ in 0..FACTS {
        let e = rng.phases(d);
        let r = rng.phases(d);
        let v = rng.phases(d);
        store.add_fact(&e, &r, &v).expect("add_fact");
        atoms.push((e, r, v));
    }
    let mut scratch = store.scratch(FACTS);
    let hidden = d * 40 / 100;

    // Corpos de prova preparados antes da medicao.
    let mut cues: Vec<(usize, Vec<f32>, Vec<f32>, Vec<bool>)> = Vec::with_capacity(QUERIES);
    for _ in 0..QUERIES {
        let truth = (rng.next_u64() as usize) % FACTS;
        let mask = rng.mask(d, hidden);
        let mut ec = atoms[truth].0.clone();
        let mut rc = atoms[truth].1.clone();
        for i in 0..d {
            if mask[i] {
                let noise = (rng.unit() - 0.5) * 0.5;
                ec[i] += noise;
                rc[i] -= noise;
            }
        }
        cues.push((truth, ec, rc, mask));
    }

    // Aquecimento (cache de instrucao + tabelas de traco).
    for _ in 0..2048 {
        for (_, ec, rc, mask) in cues.iter().take(16) {
            let _ = store.query_into(ec, rc, mask, &mut scratch).expect("warm");
        }
    }

    let mut lat_us = Vec::with_capacity(QUERIES);
    let mut hits = 0usize;
    for (truth, ec, rc, mask) in &cues {
        let t0 = Instant::now();
        let hit = store.query_into(ec, rc, mask, &mut scratch).expect("query");
        lat_us.push(t0.elapsed().as_nanos() as f64 / 1000.0);
        if hit.value_index == *truth {
            hits += 1;
        }
    }
    let mut sorted = lat_us.clone();
    sorted.sort_by(f64::total_cmp);
    let mean = lat_us.iter().sum::<f64>() / lat_us.len() as f64;

    // Criterio da auditoria: melhor de 6, em 20 consultas.
    let mut best_of_6: Vec<f64> = Vec::with_capacity(20);
    for (_, ec, rc, mask) in cues.iter().take(20) {
        let mut best = f64::INFINITY;
        for _ in 0..6 {
            let t = Instant::now();
            let _ = store.query_into(ec, rc, mask, &mut scratch).expect("query");
            best = best.min(t.elapsed().as_nanos() as f64 / 1000.0);
        }
        best_of_6.push(best);
    }
    let audit_mean = best_of_6.iter().sum::<f64>() / best_of_6.len() as f64;
    best_of_6.sort_by(f64::total_cmp);

    println!(
        "{label}: D={d} K={k} fatos={FACTS} ocultos={hidden} consultas={QUERIES} \
recall@1={:.3}\n  media={mean:.2}us p50={:.2} p95={:.2} p99={:.2} max={:.2} | \
auditoria(melhor_de_6x20)={audit_mean:.2}us (p50 {:.2})",
        hits as f64 / QUERIES as f64,
        percentile(&sorted, 0.50),
        percentile(&sorted, 0.95),
        percentile(&sorted, 0.99),
        percentile(&sorted, 1.0),
        percentile(&best_of_6, 0.50),
    );
}

fn main() {
    println!("T12 — RAG holografico, percentis reais (sem descarte de amostra)");
    run(160, 6, "T12a");
    run(256, 6, "T12b");
    run(256, 12, "T12c");
    run(512, 6, "T12d");
}
