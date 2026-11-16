//! scratch: compara cleanup complexo vs. projeção de fase no readout do PLH.

use std::f32::consts::TAU;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::bam_phase::{poly_sin_q14, BamHolographicVector, BamLut, BamPhase};
use aether_engine::core::silicon_entropy::splitmix64;

const D: usize = 160;
const LANES: usize = 16;
const K: usize = 6;
const M: usize = 43;

type BVec = BamHolographicVector<D>;

fn rho(v: &[u16; D], k: usize) -> [u16; D] {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = v[(j + k) % D];
    }
    out
}

fn bind(a: &[u16; D], b: &[u16; D]) -> [u16; D] {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = a[j].wrapping_add(b[j]);
    }
    out
}

fn unrho(v: &[u16; D], k: usize) -> [u16; D] {
    let k = k % D;
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = v[(j + D - k) % D];
    }
    out
}

fn atom(rng: &mut ChaCha8Rng) -> BVec {
    let p: Vec<f32> = (0..D).map(|_| rng.gen::<f32>() * TAU).collect();
    BVec::from_radians(&p).unwrap()
}

struct Shard {
    tre: Vec<[i16; D]>,
    tim: Vec<[i16; D]>,
    acc_re: Vec<[i32; D]>,
    acc_im: Vec<[i32; D]>,
    psi: Vec<u16>,
    shift: u32,
    rng: u64,
}

impl Shard {
    fn new() -> Self {
        Self {
            tre: vec![[0i16; D]; K],
            tim: vec![[0i16; D]; K],
            acc_re: vec![[0i32; D]; K],
            acc_im: vec![[0i32; D]; K],
            psi: Vec::new(),
            shift: 0,
            rng: 0x5151,
        }
    }
    fn add(&mut self, key: &[u16; D], value: &[u16; D]) {
        let lut = BamLut::global();
        for t in 0..K {
            let psi = if t == 0 { 0 } else { (splitmix64(&mut self.rng) >> 48) as u16 };
            self.psi.push(psi);
            for j in 0..D {
                let th = key[j].wrapping_add(value[j]).wrapping_add(psi);
                let (s, c) = lut.sin_cos_q15(BamPhase(th));
                self.acc_re[t][j] += c as i32;
                self.acc_im[t][j] += s as i32;
            }
        }
    }
    fn finalize(&mut self) {
        let max = self.acc_re.iter().chain(self.acc_im.iter()).flat_map(|a| a.iter()).map(|v| v.unsigned_abs()).max().unwrap_or(0);
        let bits = 32 - max.leading_zeros();
        self.shift = bits.saturating_sub(12);
        for t in 0..K {
            for j in 0..D {
                self.tre[t][j] = (self.acc_re[t][j] >> self.shift) as i16;
                self.tim[t][j] = (self.acc_im[t][j] >> self.shift) as i16;
            }
        }
    }
    fn unbind(&self, fact: usize, key: &[u16; D], zr: &mut [f32; D], zi: &mut [f32; D]) {
        zr.fill(0.0);
        zi.fill(0.0);
        let inv = 1.0 / 16384.0;
        for t in 0..K {
            let psi = self.psi[fact * K + t];
            for lane in 0..D / LANES {
                let base = lane * LANES;
                let mut sa = [0i16; LANES];
                let mut ca = [0i16; LANES];
                for l in 0..LANES {
                    let phi = key[base + l].wrapping_add(psi);
                    sa[l] = phi as i16;
                    ca[l] = phi.wrapping_add(0x4000) as i16;
                }
                let s = poly_sin_q14(&sa);
                let c = poly_sin_q14(&ca);
                for l in 0..LANES {
                    let cc = c[l] as f32 * inv;
                    let ss = s[l] as f32 * inv;
                    let mr = self.tre[t][base + l] as f32;
                    let mi = self.tim[t][base + l] as f32;
                    zr[base + l] += mr * cc + mi * ss;
                    zi[base + l] += mi * cc - mr * ss;
                }
            }
        }
    }
}

fn cleanup_complex(zr: &[f32; D], zi: &[f32; D], book: &[BVec], k: usize) -> (usize, f32, f32) {
    let lut = BamLut::global();
    let inv = 1.0 / (D as f32 * 32767.0 * k as f32);
    let mut best = 0;
    let mut bs = f32::NEG_INFINITY;
    let mut second = f32::NEG_INFINITY;
    for (i, a) in book.iter().enumerate() {
        let mut acc = 0f32;
        for j in 0..D {
            let (s, c) = lut.sin_cos(BamPhase(a.as_slice()[j]));
            acc += zr[j] * c + zi[j] * s;
        }
        let sim = acc * inv;
        if sim > bs { second = bs; bs = sim; best = i; } else if sim > second { second = sim; }
    }
    (best, bs, bs - second)
}

fn cleanup_phase(zr: &[f32; D], zi: &[f32; D], book: &[BVec]) -> (usize, f32, f32) {
    let mut cue = [0u16; D];
    for j in 0..D { cue[j] = BamPhase::from_radians(zi[j].atan2(zr[j])).0; }
    let q14 = 16384i32;
    let mut best = 0; let mut bs = f32::NEG_INFINITY; let mut second = f32::NEG_INFINITY;
    for (i, a) in book.iter().enumerate() {
        let mut acc = 0i32;
        for j in 0..D {
            let s = cue[j].wrapping_sub(a.as_slice()[j]) as i16;
            let ax = s ^ (s >> 15);
            acc += (q14 as i16 - ax) as i32;
        }
        let sim = acc as f32 / (q14 as f32 * D as f32);
        if sim > bs { second = bs; bs = sim; best = i; } else if sim > second { second = sim; }
    }
    (best, bs, bs - second)
}

fn main() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xE1);
    let ents: Vec<BVec> = (0..64).map(|_| atom(&mut rng)).collect();
    let verbs: Vec<BVec> = (0..16).map(|_| atom(&mut rng)).collect();
    let mods: Vec<BVec> = (0..8).map(|_| atom(&mut rng)).collect();
    let anchors: Vec<BVec> = (0..4).map(|_| atom(&mut rng)).collect();

    let role_term = |a: &BVec, f: &BVec, k: usize| bind(a.as_slice(), &rho(f.as_slice(), k));
    let mut shard = Shard::new();
    let mut facts = Vec::new();
    for f in 0..M {
        let s = f % 64;
        let v = (f / 64) % 16;
        let o = (f * 37 + 11) % 64;
        let m = (f * 5 + 3) % 8;
        let key = bind(&role_term(&anchors[0], &ents[s], 0), &role_term(&anchors[1], &verbs[v], 1));
        let val = bind(&role_term(&anchors[2], &ents[o], 2), &role_term(&anchors[3], &mods[m], 3));
        shard.add(&key, &val);
        facts.push((s, v, o, m, key));
    }
    shard.finalize();

    // debug fact 0: a chave desvinculada deve reconstruir o valor.
    {
        let (s, v, o, m, key) = facts[0];
        let val = bind(
            &role_term(&anchors[2], &ents[o], 2),
            &role_term(&anchors[3], &mods[m], 3),
        );
        let mut zr = [0f32; D];
        let mut zi = [0f32; D];
        shard.unbind(0, &key, &mut zr, &mut zi);
        let mut zp = [0u16; D];
        for j in 0..D { zp[j] = BamPhase::from_radians(zi[j].atan2(zr[j])).0; }
        let tri = |a: &[u16; D], b: &[u16; D]| {
            let mut acc = 0i32;
            for j in 0..D { let d = a[j].wrapping_sub(b[j]) as i16; let ax = d ^ (d >> 15); acc += (16384i16 - ax) as i32; }
            acc as f32 / (16384.0 * D as f32)
        };
        println!("fact0 s={s} v={v} o={o} m={m} tri(z,value)={:.3} tri(z,0)={:.3}", tri(&zp, &val), tri(&zp, &[0u16; D]));
        // valor esperado sem memória, direto.
        let mut vr = [0f32; D]; let mut vi = [0f32; D];
        let lut = BamLut::global();
        for j in 0..D { let (ss, cc) = lut.sin_cos(BamPhase(val[j])); vr[j] = cc; vi[j] = ss; }
        let mut or_ = [0f32; D]; let mut oi = [0f32; D];
        for j in 0..D {
            let (ss, cc) = lut.sin_cos(BamPhase(anchors[2].as_slice()[j]));
            or_[j] = vr[j] * cc + vi[j] * ss;
            oi[j] = vi[j] * cc - vr[j] * ss;
        }
        // o filler foi permutado por rho^2 no role_term; desfaz antes do cleanup.
        let mut cr = [0f32; D];
        let mut ci = [0f32; D];
        for j in 0..D {
            let src = (j + D - 2) % D; // rho^{-2}
            cr[j] = or_[src];
            ci[j] = oi[src];
        }
        let (oc, os, _) = cleanup_complex(&cr, &ci, &ents, 1);
        let mut d = [0u16; D];
        for j in 0..D { d[j] = val[j].wrapping_sub(anchors[2].as_slice()[j]); }
        let u = unrho(&d, 2);
        println!(
            "cleanup ideal: idx={oc} esperado={o} sim={os:.3} tri(unrho,ent)={:.3} tri(unrho,rho^2 ent)={:.3}",
            tri(&u, ents[o].as_slice()),
            tri(&u, &rho(ents[o].as_slice(), 2))
        );
    }

    let mut hits_c = 0; let mut hits_p = 0;
    let mut sim_c = 0.0; let mut sim_p = 0.0;
    for (f, (s, v, o, m, key)) in facts.iter().enumerate() {
        let mut zr = [0f32; D]; let mut zi = [0f32; D];
        shard.unbind(f, key, &mut zr, &mut zi);
        // complex: unbind object anchor, then cleanup; subtract mod re-encode would need iteration; first raw.
        let (ao_s, ao_c) = {
            let mut s = [0i16; D]; let mut c = [0i16; D];
            let lut = BamLut::global();
            for j in 0..D { let (ss, cc) = lut.sin_cos(BamPhase(anchors[2].as_slice()[j])); s[j] = (ss * 32767.0) as i16; c[j] = (cc * 32767.0) as i16; }
            (s, c)
        };
        let mut or_ = [0f32; D]; let mut oi = [0f32; D];
        for j in 0..D {
            let c = ao_c[j] as f32 / 32767.0; let s = ao_s[j] as f32 / 32767.0;
            or_[j] = zr[j] * c + zi[j] * s;
            oi[j] = zi[j] * c - zr[j] * s;
        }
        let mut cr = [0f32; D]; let mut ci = [0f32; D];
        for j in 0..D { let src = (j + D - 2) % D; cr[j] = or_[src]; ci[j] = oi[src]; }
        let (oc, os, _) = cleanup_complex(&cr, &ci, &ents, K);
        let (op, ops, _) = cleanup_phase(&cr, &ci, &ents);
        if oc == *o { hits_c += 1; }
        if op == *o { hits_p += 1; }
        sim_c += os; sim_p += ops;
        let _ = (v, m);
    }
    println!("M={M} K={K} complex_obj={:.3} (sim {:.3}) phase_obj={:.3} (sim {:.3})",
        hits_c as f64 / M as f64, sim_c / M as f32, hits_p as f64 / M as f64, sim_p / M as f32);
}

