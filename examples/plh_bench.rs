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
    let f = &facts[0];
    let idx = shard_of(f.ids[0] as usize, f.ids[1] as usize);
    let shard = mem.shard(idx);
    let fact = shard.find(f.ids[0], f.ids[1]).unwrap();
    let key = f.key(&cb, &a);
    let mut sc = QueryScratch::new();

    // warmup
    for _ in 0..100 {
        shard.unbind(fact, &key, &mut sc.z_re, &mut sc.z_im);
        shard.normalize_into(&mut sc.z_re, &mut sc.z_im);
        black_box(resonate_pair(&mut sc, &cb, &a));
    }

    let n = 2000;
    let mut unbind = Vec::with_capacity(n);
    let mut res = Vec::with_capacity(n);
    let mut full = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        shard.unbind(fact, &key, &mut sc.z_re, &mut sc.z_im);
        shard.normalize_into(&mut sc.z_re, &mut sc.z_im);
        unbind.push(t.elapsed().as_nanos() as f64 / 1000.0);

        let t = Instant::now();
        black_box(resonate_pair(&mut sc, &cb, &a));
        res.push(t.elapsed().as_nanos() as f64 / 1000.0);

        let t = Instant::now();
        black_box(hot_query(&mem, &cb, &a, f, &mut sc));
        full.push(t.elapsed().as_nanos() as f64 / 1000.0);
    }
    println!(
        "unbind p50={:.2} p99={:.2} | resonate p50={:.2} p99={:.2} | full p50={:.2} p99={:.2}",
        pct(&mut unbind, 0.5),
        pct(&mut unbind, 0.99),
        pct(&mut res, 0.5),
        pct(&mut res, 0.99),
        pct(&mut full, 0.5),
        pct(&mut full, 0.99)
    );
}
