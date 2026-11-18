//! B2 — 100k átomos: LSH obrigatória + multi-probe (MSG 0458 / plano A4).
//!
//! Roofline B1: 10k varredura p99≈0,64 ms → 100k ≈6,4 ms > teto 5 ms.
//! Sem o bank de 100k: aceita shard ≥10k com LSH (medida real, não finge 100k).

use std::path::{Path, PathBuf};
use std::time::Instant;

use aether_engine::core::hv::HvBank;

const FULL_CANDIDATES: &[&str] = &[
    "data/rounds/20261117_b2/hv/bank.bin",
    "data/rounds/b2_100k/hv/bank.bin",
    "data/hv/bank_100k.bin",
];

/// Shards intermediários (≥10k) antes do monolito 100k.
const SHARD_CANDIDATES: &[&str] = &[
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

fn find_full() -> Option<(PathBuf, PathBuf)> {
    for bin in FULL_CANDIDATES {
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

fn find_shard() -> Option<(PathBuf, PathBuf)> {
    for bin in SHARD_CANDIDATES {
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

fn measure_recall_p99(bank: &HvBank, sample: usize) -> (f64, u128, usize) {
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
fn b2_lsh_recall_p99_rss() {
    if let Some((bank_p, idx_p)) = find_full() {
        let bank = HvBank::load(&bank_p, &idx_p).expect("bank");
        assert!(
            bank.count() >= 100_000,
            "B2 exige n≥100k, got {}",
            bank.count()
        );
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
        let (recall, p99, sample) = measure_recall_p99(&bank, 200);
        let rss = working_set();
        eprintln!(
            "B2 FULL n={} probes={} sample={sample} recall@10={recall:.3} p99_us={p99} rss={rss}",
            bank.count(),
            bank.lsh_probes
        );
        assert!(recall >= 0.90, "recall {recall}");
        assert!(p99 <= 5_000, "p99 {p99}us > 5ms");
        assert!(rss <= 256 * 1024 * 1024, "rss {rss}");
        return;
    }

    // Aceite intermediário A4: shard ≥10k com LSH multi-probe (não inventa 100k).
    let Some((bank_p, idx_p)) = find_shard() else {
        eprintln!("B2: bank 100k e shards ≥10k ausentes — skip (embed A1/A2 antes)");
        return;
    };
    let bank = HvBank::load(&bank_p, &idx_p).expect("shard bank");
    if bank.count() < 10_000 {
        eprintln!(
            "B2 shard {}: n={} <10k — skip (ainda sem LSH auto)",
            bank_p.display(),
            bank.count()
        );
        return;
    }
    assert!(
        !bank.lsh_tables.is_empty(),
        "shard n={} sem LSH",
        bank.count()
    );
    assert!(
        bank.lsh_probes > 0,
        "shard exige multi-probe, got {}",
        bank.lsh_probes
    );
    let (recall, p99, sample) = measure_recall_p99(&bank, 200);
    eprintln!(
        "B2 SHARD {} n={} probes={} sample={sample} recall@10={recall:.3} p99_us={p99}",
        bank_p.display(),
        bank.count(),
        bank.lsh_probes
    );
    assert!(recall >= 0.90, "shard recall {recall}");
    assert!(p99 <= 5_000, "shard p99 {p99}us > 5ms");
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
