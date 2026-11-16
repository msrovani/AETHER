use std::hint::black_box;
use std::time::Instant;

use aether_engine::core::phase_language::*;

fn pct(v: &mut [f64], q: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 * q) as usize).min(v.len() - 1)]
}

fn main() {
    let seed = 0xE1_2026u64;
    let cb = Codebooks::build(seed);
    let a = Anchors::build(seed);
    let facts = build_world(N_FACTS);
    let mem = build_sharded(&facts, &cb, &a, K_TRACES, seed);
    let mut sc = QueryScratch::new();
    let n = 400usize;
    let mut picks = Vec::with_capacity(n);
    for k in 0..n {
        picks.push((k * facts.len()) / n);
    }
    for _ in 0..64 {
        black_box(hot_query(&mem, &cb, &a, &facts[picks[0]], &mut sc));
    }
    let mut lat = Vec::with_capacity(n);
    for &pi in &picks {
        let f = &facts[pi];
        let t = Instant::now();
        black_box(hot_query(&mem, &cb, &a, f, &mut sc));
        lat.push(t.elapsed().as_nanos() as f64 / 1000.0);
    }
    let p50 = pct(&mut lat, 0.50);
    let p99 = pct(&mut lat, 0.99);
    let mean = lat.iter().sum::<f64>() / lat.len() as f64;
    println!(
        "K={K_TRACES} p50={p50:.2} p99={p99:.2} mean={mean:.2} ws={}KB",
        (cb.memory_bytes()
            + mem.memory_bytes()
            + std::mem::size_of::<QueryScratch>()
            + std::mem::size_of::<Anchors>())
            / 1024
    );
}
