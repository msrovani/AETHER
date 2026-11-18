//! B2 — 100k átomos: LSH obrigatória (MSG 0458).
//!
//! Roofline B1: 10k varredura p99≈0,64 ms → 100k ≈6,4 ms > teto 5 ms.
//! Sem o bank de 100k o teste não finge o número (skip honesto).

use std::path::Path;
use std::time::Instant;

use aether_engine::core::hv::HvBank;

const CANDIDATES: &[&str] = &[
    "data/rounds/20261117_b2/hv/bank.bin",
    "data/rounds/b2_100k/hv/bank.bin",
    "data/hv/bank_100k.bin",
];
const IDX_SUFFIX: &str = "bank.idx";

fn find_bank() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    for bin in CANDIDATES {
        let bin = Path::new(bin);
        if !bin.exists() {
            continue;
        }
        let idx = bin.with_file_name(IDX_SUFFIX);
        let idx = if idx.exists() {
            idx
        } else {
            bin.with_extension("idx")
        };
        if idx.exists() {
            return Some((bin.to_path_buf(), idx));
        }
    }
    None
}

#[test]
fn b2_lsh_recall_p99_rss() {
    let Some((bank_p, idx_p)) = find_bank() else {
        eprintln!("B2: bank 100k ausente — skip (medir embed 1k antes de comprometer)");
        return;
    };
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

    let n = bank.count();
    let sample = 200.min(n);
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
    let rss = working_set();
    eprintln!(
        "B2 n={n} lsh={} recall@10={recall:.3} p99_us={p99} rss={rss}",
        bank.lsh_tables.len()
    );
    // MSG 0458: recall@10 ≥0.90 (mais fraco que 0.95 — carga de colisão LSH).
    assert!(recall >= 0.90, "recall {recall}");
    assert!(p99 <= 5_000, "p99 {p99}us > 5ms");
    assert!(rss <= 256 * 1024 * 1024, "rss {rss}");
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
