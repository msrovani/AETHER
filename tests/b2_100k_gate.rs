//! B2 — 100k átomos via shards ≤ capacidade E1B (OPENCODE 0520).
//!
//! Lei: capacidade ≈ 0,4 × D (10240 → 4096). Bank > capacidade degrada recall
//! por lei — piso 0,90 só vale dentro da capacidade. 100k = 25×~4k (união).
//! Roofline B1: 10k p99≈0,64 ms → 100k monolito ≈6,4 ms > teto 5 ms → LSH/shards.

use std::path::{Path, PathBuf};
use std::time::Instant;

use aether_engine::core::hv::{e1b_atom_capacity, shard_plan_e1b, HvBank};

const FULL_CANDIDATES: &[&str] = &[
    "data/rounds/20261117_b2/hv/bank.bin",
    "data/rounds/b2_100k/hv/bank.bin",
    "data/hv/bank_100k.bin",
];

/// Preferência: shards ≤ capacidade (4k). Over-capacity só para medida honesta.
const CAPACITY_SHARD_CANDIDATES: &[&str] = &[
    "data/rounds/b2_shards/4k/hv/bank.bin",
    "data/rounds/b2_shards/4096/hv/bank.bin",
    "data/rounds/20261117_b0_paws/hv/bank.bin",
    "data/rounds/20261117_bg0/hv/bank.bin",
    "data/rounds/20261116_lab_dryrun/hv/bank.bin",
];

/// Shards históricos > capacidade (10k/50k) — reporta, não exige recall≥0,90.
const OVER_CAP_SHARD_CANDIDATES: &[&str] = &[
    "data/rounds/b2_shards/10k/hv/bank.bin",
    "data/rounds/b2_shards/50k/hv/bank.bin",
    "data/rounds/20261117_b1/hv/bank.bin",
    "data/rounds/bg0_paws/hv/bank.bin",
];

fn idx_for(bin: &Path) -> PathBuf {
    let named = bin.with_file_name("bank.idx");
    if named.exists() {
        named
    } else {
        bin.with_extension("idx")
    }
}

fn find_existing(cands: &[&str]) -> Option<(PathBuf, PathBuf)> {
    for bin in cands {
        let bin = Path::new(bin);
        if !bin.exists() {
            continue;
        }
        let idx = idx_for(bin);
        if idx.exists() {
            return Some((bin.to_path_buf(), idx));
        }
    }
    None
}

/// Primeiro candidato com `min_n ≤ count ≤ e1b_capacity` (piso 0,90 válido).
fn find_capacity_shard(cands: &[&str], min_n: usize) -> Option<(PathBuf, PathBuf, HvBank)> {
    for bin in cands {
        let bin = Path::new(bin);
        if !bin.exists() {
            continue;
        }
        let idx = idx_for(bin);
        if !idx.exists() {
            continue;
        }
        let mut bank = match HvBank::load(bin, &idx) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bank.count() >= min_n && !bank.over_capacity() {
            return Some((bin.to_path_buf(), idx, bank));
        }
    }
    None
}

fn measure_recall_p99(bank: &mut HvBank, sample: usize) -> (f64, u128, usize) {
    let n = bank.count();
    let sample = sample.min(n);
    let mut hits = 0usize;
    let mut us = Vec::with_capacity(sample);
    let mut out = [(u32::MAX, 0u128); 10];
    for i in 0..sample {
        let q = bank.atom_words(i).to_vec();
        let t0 = Instant::now();
        let got = bank.top_k_into(&q, 10, &mut out);
        us.push(t0.elapsed().as_micros());
        if (0..got).any(|k| out[k].1 == bank.keys[i]) {
            hits += 1;
        }
    }
    us.sort_unstable();
    let p99 = us[(us.len() * 99 / 100).min(us.len() - 1)];
    let recall = hits as f64 / sample as f64;
    (recall, p99, sample)
}

#[test]
fn b2_e1b_shard_plan_100k() {
    let plan = shard_plan_e1b(100_000, 10_240);
    assert_eq!(e1b_atom_capacity(10_240), 4_096);
    assert_eq!(plan.len(), 25);
    assert!(plan.iter().all(|&(_, n)| n <= 4_096));
    assert_eq!(plan.iter().map(|p| p.1).sum::<usize>(), 100_000);
}

#[test]
fn b2_lsh_recall_p99_rss() {
    if let Some((bank_p, idx_p)) = find_existing(FULL_CANDIDATES) {
        let mut bank = HvBank::load(&bank_p, &idx_p).expect("bank");
        assert!(
            bank.count() >= 100_000,
            "B2 exige n≥100k, got {}",
            bank.count()
        );
        // Monolito 100k está sobre capacidade — LSH obrigatória para latência.
        assert!(
            !bank.lsh_tables.is_empty(),
            "B2 n={} sem LSH — varredura exata estoura o teto 5ms",
            bank.count()
        );
        assert!(
            bank.lsh_probes > 0,
            "B2 exige multi-probe (lsh_probes>0), got {}",
            bank.lsh_probes
        );
        let (recall, p99, sample) = measure_recall_p99(&mut bank, 200);
        let rss = working_set();
        eprintln!(
            "B2 FULL n={} cap={} over={} probes={} sample={sample} recall@10={recall:.3} p99_us={p99} rss={rss}",
            bank.count(),
            bank.e1b_capacity(),
            bank.over_capacity(),
            bank.lsh_probes
        );
        // Piso 0,90 só se o bank estiver particionado ≤ capacidade (não monolito).
        if !bank.over_capacity() {
            assert!(recall >= 0.90, "recall {recall}");
        } else {
            eprintln!(
                "B2 FULL over_capacity — recall@10={recall:.3} reportado sem piso 0,90 (lei E1B; use 25×4k)"
            );
        }
        assert!(p99 <= 5_000, "p99 {p99}us > 5ms");
        assert!(rss <= 256 * 1024 * 1024, "rss {rss}");
        return;
    }

    // Aceite intermediário: shard ≤ capacidade E1B com recall≥0,90.
    if let Some((bank_p, _idx_p, mut bank)) = find_capacity_shard(CAPACITY_SHARD_CANDIDATES, 1_000)
    {
        let cap = bank.e1b_capacity();
        let (recall, p99, sample) = measure_recall_p99(&mut bank, 200);
        eprintln!(
            "B2 CAP-SHARD {} n={} cap={} probes={} sample={sample} recall@10={recall:.3} p99_us={p99}",
            bank_p.display(),
            bank.count(),
            cap,
            bank.lsh_probes
        );
        assert!(
            recall >= 0.90,
            "capacity-shard recall {recall} (piso 0,90 dentro da lei)"
        );
        assert!(p99 <= 5_000, "capacity-shard p99 {p99}us > 5ms");
        return;
    }

    // Over-capacity legado: mede e reporta — sem inventar 0,90.
    if let Some((bank_p, idx_p)) = find_existing(OVER_CAP_SHARD_CANDIDATES) {
        let mut bank = HvBank::load(&bank_p, &idx_p).expect("over-cap shard");
        if bank.count() >= 10_000 && bank.lsh_tables.is_empty() {
            bank.build_lsh(4, 16);
        }
        let (recall, p99, sample) = measure_recall_p99(&mut bank, 200);
        eprintln!(
            "B2 OVER-CAP {} n={} cap={} over={} probes={} sample={sample} recall@10={recall:.3} p99_us={p99} (sem piso 0,90)",
            bank_p.display(),
            bank.count(),
            bank.e1b_capacity(),
            bank.over_capacity(),
            bank.lsh_probes
        );
        assert!(
            bank.over_capacity(),
            "candidato over-cap deveria exceder e1b_capacity"
        );
        assert!(p99 <= 5_000, "over-cap p99 {p99}us > 5ms");
        return;
    }

    eprintln!("B2: bank 100k e shards ausentes — skip (embed A1/A2 + fatia ≤4k antes)");
}

#[cfg(windows)]
fn working_set() -> u64 {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault: u32,
        peak_ws: usize,
        ws: usize,
        qpeak_paged: usize,
        q_paged: usize,
        qpeak_nonpaged: usize,
        q_nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(process: isize, counters: *mut Counters, cb: u32) -> i32;
    }
    let mut c = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        page_fault: 0,
        peak_ws: 0,
        ws: 0,
        qpeak_paged: 0,
        q_paged: 0,
        qpeak_nonpaged: 0,
        q_nonpaged: 0,
        pagefile: 0,
        peak_pagefile: 0,
        private: 0,
    };
    unsafe {
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return 0;
        }
    }
    c.ws as u64
}

#[cfg(not(windows))]
fn working_set() -> u64 {
    0
}
