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
    let cdot = |zr: &[f32; D], zi: &[f32; D], ph: &[u16; D]| {
        let mut dot = 0f32;
        for j in 0..D {
            let (s, c) = lut.sin_cos(BamPhase(ph[j]));
            dot += zr[j] * c + zi[j] * s;
        }
        dot / D as f32
    };
    let term_o = role_term(a.of(ROLE_O), &cb.ent.clean[f.ids[2] as usize], ROLE_O);
    let term_m0 = role_term(a.of(ROLE_M), &cb.modu.clean[f.ids[3] as usize], ROLE_M);
    let mut to_re = [0f32; D];
    let mut to_im = [0f32; D];
    for j in 0..D {
        let (s, c) = lut.sin_cos(BamPhase(term_o[j]));
        to_re[j] = c;
        to_im[j] = s;
    }
    println!(
        "dot(z,value)={:.3} dot(z,termO)={:.3} dot(z,termM)={:.3} dot(termO,termM)={:.3}",
        cdot(&sc.z_re, &sc.z_im, &val),
        cdot(&sc.z_re, &sc.z_im, &term_o),
        cdot(&sc.z_re, &sc.z_im, &term_m0),
        cdot(&to_re, &to_im, &term_m0)
    );
    let mut rr = sc.z_re;
    let mut ri = sc.z_im;
    for j in 0..D {
        let (s, c) = lut.sin_cos(BamPhase(term_m0[j]));
        rr[j] -= c;
        ri[j] -= s;
    }
    println!(
        "resid norm={:.2} dot(res,termO)={:.3}",
        (rr.iter().map(|x| x * x).sum::<f32>() + ri.iter().map(|x| x * x).sum::<f32>()).sqrt(),
        cdot(&rr, &ri, &term_o)
    );

    // Cue OBJ independente: z − termM, desvincula A_O, unrho.
    let term_m = bind(
        a.m.as_slice(),
        &rho(cb.modu.clean[f.ids[3] as usize].as_slice(), ROLE_M),
    );
    let mut ur = [0f32; D];
    let mut ui = [0f32; D];
    for j in 0..D {
        let (sm, cm) = lut.sin_cos(BamPhase(term_m[j]));
        let re = sc.z_re[j] - cm;
        let im = sc.z_im[j] - sm;
        let (so, co) = lut.sin_cos(BamPhase(a.o.as_slice()[j]));
        ur[j] = re * co + im * so;
        ui[j] = im * co - re * so;
    }
    let mut cr = [0f32; D];
    let mut ci = [0f32; D];
    for j in 0..D {
        cr[j] = ur[(j + D - ROLE_O) % D];
        ci[j] = ui[(j + D - ROLE_O) % D];
    }
    let mut csims = [0f32; N_ENT];
    let c = cleanup_complex(&cr, &ci, cb.ent.clean_q.as_slice(), &mut csims);
    let mut corder: Vec<(usize, f32)> = csims.iter().copied().enumerate().collect();
    corder.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
    println!(
        "cue independente: idx={} sim={:.3} true={} top={:?}",
        c.index,
        c.sim,
        f.ids[2],
        &corder[..5]
    );

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
