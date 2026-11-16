use aether_engine::core::bam_phase::{BamLut, BamPhase};
use aether_engine::core::phase_language::*;

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
    shard.unbind(fact, &key, &mut sc.z_re, &mut sc.z_im);
    let raw_norm: f32 = sc.z_re.iter().map(|x| x * x).sum::<f32>().sqrt();
    shard.normalize_into(&mut sc.z_re, &mut sc.z_im);
    let norm: f32 = (sc.z_re.iter().map(|x| x * x).sum::<f32>()
        + sc.z_im.iter().map(|x| x * x).sum::<f32>())
    .sqrt();
    println!(
        "fact={:?} shard={idx} len={} shift? unit={} raw_norm={raw_norm:.2} norm={norm:.2}",
        f.ids,
        shard.len(),
        shard.unit_q15()
    );

    // Similaridade do z normalizado com o valor esperado (2 termos unitários).
    let val = f.value(&cb, &a);
    let lut = BamLut::global();
    let mut dot = 0f32;
    for j in 0..D {
        let (s, c) = lut.sin_cos(BamPhase(val[j]));
        dot += sc.z_re[j] * c + sc.z_im[j] * s;
    }
    println!("dot(z, value)/D = {:.3}", dot / D as f32);

    let p = resonate_pair(&mut sc, &cb, &a);
    println!("pair={p:?} true_obj={} true_mod={}", f.ids[2], f.ids[3]);
    let mut osims = [0f32; N_ENT];
    let mut msims = [0f32; N_MOD];
    evidence_sims(&mut sc, &cb, &a, &p, &mut osims, &mut msims);
    let mut order: Vec<(usize, f32)> = osims.iter().copied().enumerate().collect();
    order.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
    println!("top obj: {:?}", &order[..5]);
    println!(
        "true obj sim={:.3} mod sim={:.3}",
        osims[f.ids[2] as usize], msims[f.ids[3] as usize]
    );
}
