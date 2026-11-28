//! B1 — 10k átomos reais. Sem o bank da rodada, o teste não finge o número.

use std::path::Path;
use std::time::Instant;

use aether_engine::core::hv::HvBank;

const BANK: &str = "data/rounds/20261117_b1/hv/bank.bin";
const IDX: &str = "data/rounds/20261117_b1/hv/bank.idx";

#[test]
fn b1_recall_p99_rss() {
    if !Path::new(BANK).exists() {
        eprintln!("B1: bank de 10k ausente, sem número sintético");
        return;
    }
    let mut bank = HvBank::load(Path::new(BANK), Path::new(IDX)).expect("bank");
    assert!(bank.count() >= 10_000, "count {}", bank.count());
    // MSG 0440 ACHADO_1: em n≥10k o caminho tem de ser LSH, não varredura exata.
    // T16: LSH lazy — o build acontece na primeira query (`ensure_lsh`).
    bank.ensure_lsh();
    assert!(
        !bank.lsh_tables.is_empty(),
        "B1 n={} sem LSH — limiar deve ser ≥10k",
        bank.count()
    );
    let n = bank.count();
    let mut hits = 0usize;
    let sample = 200.min(n);
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
        "B1 n={n} lsh_tables={} recall@10={recall:.3} p99_us={p99} rss={rss}",
        bank.lsh_tables.len()
    );
    assert!(recall >= 0.95, "recall {recall}");
    assert!(p99 <= 5_000, "p99 {p99}us");
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
