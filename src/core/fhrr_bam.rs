//! Protótipo FHRR-BAM isolado (Fase 1) — **não** substitui `fhrr.rs`.
//!
//! Empacota [`BamHolographicStore`] com API em radianos `f32` para comparar
//! lado a lado com [`FhrrStore`] no protocolo T12 (D=160, K=6, 50 fatos).
//! Critério de integração (MSG 0081 / autorização): `Recall@1 == 1.0` **e**
//! speedup e2e ≥ 1.5×; caso contrário permanece experimento documentado.

#![allow(clippy::needless_range_loop)]

use crate::core::bam_phase::{
    BamHit, BamHolographicStore, BamHolographicVector, BamMask, BamScratch,
};
use crate::error::Result;

/// Dimensão do harness T12.
pub const T12_D: usize = 160;
/// Alias estável.
pub const T12_DIM: usize = T12_D;
/// Traços K do T12.
pub const T12_TRACES: usize = 6;
/// Fatos do T12.
pub const T12_FACTS: usize = 50;
/// Speedup mínimo para integração.
pub const INTEGRATION_SPEEDUP: f64 = 1.5;

/// Store BAM tipado no D do T12 + scratch interno para API em radianos.
#[derive(Debug)]
pub struct T12Bam {
    store: BamHolographicStore<T12_D>,
    scratch: BamScratch<T12_D>,
}

impl T12Bam {
    /// `traces` ∈ 1..=15; `max_values` dimensiona o scratch.
    pub fn new(traces: usize, max_values: usize) -> Result<Self> {
        let store = BamHolographicStore::<T12_D>::new(traces)?;
        let scratch = store.scratch(max_values);
        Ok(Self { store, scratch })
    }

    /// Bytes dos buffers de consulta.
    pub fn memory_bytes(&self) -> usize {
        self.store.memory_bytes()
    }

    /// Número de fatos gravados.
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// `true` se nenhum fato foi gravado.
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// Número de traços `K`.
    pub fn traces(&self) -> usize {
        self.store.traces()
    }

    /// Grava fato a partir de fases em radianos (`len == T12_D`).
    pub fn add_fact_radians(&mut self, e: &[f32], r: &[f32], v: &[f32]) -> Result<usize> {
        let eb = BamHolographicVector::<T12_D>::from_radians(e)?;
        let rb = BamHolographicVector::<T12_D>::from_radians(r)?;
        let vb = BamHolographicVector::<T12_D>::from_radians(v)?;
        Ok(self.store.add_fact(&eb, &rb, &vb))
    }

    /// Consulta zero-alloc a partir de radianos + máscara bool.
    pub fn query_radians(&mut self, e: &[f32], r: &[f32], mask: &[bool]) -> Result<BamHit> {
        let eb = BamHolographicVector::<T12_D>::from_radians(e)?;
        let rb = BamHolographicVector::<T12_D>::from_radians(r)?;
        let bm = BamMask::<T12_D>::from_bools(mask)?;
        self.store.query_into(&eb, &rb, &bm, &mut self.scratch)
    }

    /// Similaridades da última consulta.
    pub fn sims(&self) -> &[f32] {
        self.scratch.sims()
    }
}

/// Roofline analítico de banda para referência `wgpu` (Fase 2).
#[derive(Clone, Copy, Debug)]
pub struct WgpuRoofline {
    pub n: usize,
    pub bytes: usize,
    pub t_us_at_100gbs: f64,
    pub t_us_at_400gbs: f64,
    pub t_us_at_900gbs: f64,
}

/// `N ∈ {512, 1024, 4096}` — teto de latência por banda (sem executar GPU).
pub fn wgpu_roofline_table() -> [WgpuRoofline; 3] {
    fn one(n: usize) -> WgpuRoofline {
        let bytes = 8 * n * n + 8 * n;
        let us = |gbs: f64| (bytes as f64) / (gbs * 1e9) * 1e6;
        WgpuRoofline {
            n,
            bytes,
            t_us_at_100gbs: us(100.0),
            t_us_at_400gbs: us(400.0),
            t_us_at_900gbs: us(900.0),
        }
    }
    [one(512), one(1024), one(4096)]
}

/// Proxy de breakdown T12 sem editar `fhrr.rs` (cue/cleanup/unbind/sim).
pub fn profile_fhrr_stage_proxy(
    dim: usize,
    traces: usize,
    facts: usize,
    visible_frac: f64,
    rounds: usize,
) -> (f64, f64, f64, f64, f64) {
    let nvis = ((dim as f64) * visible_frac).round() as usize;
    let mut cue_re = vec![0.0f32; dim];
    let mut cue_im = vec![0.0f32; dim];
    let mut role_re = vec![0.0f32; dim * facts];
    let mut role_im = vec![0.0f32; dim * facts];
    let mut tr_re = vec![0.0f32; dim * traces];
    let mut tr_im = vec![0.0f32; dim * traces];
    let mut val_re = vec![0.0f32; dim * facts];
    let mut val_im = vec![0.0f32; dim * facts];
    let mut z_re = vec![0.0f32; dim];
    let mut z_im = vec![0.0f32; dim];
    for (i, x) in role_re.iter_mut().enumerate() {
        *x = ((i * 1103515245 + 12345) as f32).sin();
    }
    for (i, x) in role_im.iter_mut().enumerate() {
        *x = ((i * 222222 + 7) as f32).cos();
    }
    for (i, x) in tr_re.iter_mut().enumerate() {
        *x = ((i * 13) as f32).sin() * 0.1;
    }
    for (i, x) in tr_im.iter_mut().enumerate() {
        *x = ((i * 17) as f32).cos() * 0.1;
    }
    for (i, x) in val_re.iter_mut().enumerate() {
        *x = ((i * 19) as f32).cos();
    }
    for (i, x) in val_im.iter_mut().enumerate() {
        *x = ((i * 23) as f32).sin();
    }

    let mut cue_ns = 0.0f64;
    let mut cleanup_ns = 0.0f64;
    let mut unbind_ns = 0.0f64;
    let mut sim_ns = 0.0f64;
    let mut acc = 0.0f32;
    use std::time::Instant;
    for _ in 0..rounds {
        let t0 = Instant::now();
        for j in 0..nvis {
            let (s, c) = (j as f32 * 0.01).sin_cos();
            cue_re[j] = c;
            cue_im[j] = s;
        }
        cue_ns += t0.elapsed().as_nanos() as f64;

        let t1 = Instant::now();
        let mut best = f32::NEG_INFINITY;
        for k in 0..facts {
            let base = k * dim;
            let mut s = 0.0f32;
            for j in 0..dim {
                s += cue_re[j] * role_re[base + j] + cue_im[j] * role_im[base + j];
            }
            if s > best {
                best = s;
            }
        }
        acc += best;
        cleanup_ns += t1.elapsed().as_nanos() as f64;

        let t2 = Instant::now();
        z_re.fill(0.0);
        z_im.fill(0.0);
        for t in 0..traces {
            let base = t * dim;
            for j in 0..dim {
                z_re[j] += tr_re[base + j];
                z_im[j] += tr_im[base + j];
            }
        }
        for j in 0..dim {
            let (sr, si) = (z_re[j], z_im[j]);
            z_re[j] = sr * role_re[j] + si * role_im[j];
            z_im[j] = si * role_re[j] - sr * role_im[j];
        }
        unbind_ns += t2.elapsed().as_nanos() as f64;

        let t3 = Instant::now();
        let mut best_s = f32::NEG_INFINITY;
        for k in 0..facts {
            let base = k * dim;
            let mut s = 0.0f32;
            for j in 0..dim {
                s += z_re[j] * val_re[base + j] + z_im[j] * val_im[base + j];
            }
            if s > best_s {
                best_s = s;
            }
        }
        acc += best_s;
        sim_ns += t3.elapsed().as_nanos() as f64;
    }
    std::hint::black_box(acc);
    let r = rounds as f64;
    let cue = cue_ns / r;
    let cleanup = cleanup_ns / r;
    let unbind = unbind_ns / r;
    let sim = sim_ns / r;
    (cue, cleanup, unbind, sim, cue + cleanup + unbind + sim)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bam_phase::LANES;
    use crate::error::AetherError;

    #[test]
    fn d_alinha_lanes() {
        assert_eq!(T12_D % LANES, 0);
    }

    #[test]
    fn t12_bam_roundtrip_limpo() {
        let mut bam = T12Bam::new(6, 64).expect("bam");
        let mut phases = vec![0.0f32; T12_D];
        for (i, p) in phases.iter_mut().enumerate() {
            *p = (i as f32) * 0.01;
        }
        bam.add_fact_radians(&phases, &phases, &phases)
            .expect("add");
        let mask = vec![true; T12_D];
        let hit = bam.query_radians(&phases, &phases, &mask).expect("query");
        assert_eq!(hit.value_index, 0);
    }

    #[test]
    fn roofline_monotono() {
        let t = wgpu_roofline_table();
        assert!(t[0].bytes < t[1].bytes);
        assert!(t[2].t_us_at_100gbs > t[2].t_us_at_900gbs);
    }

    #[test]
    fn reject_dim_errada() {
        let mut bam = T12Bam::new(6, 8).expect("bam");
        let bad = vec![0.0f32; 8];
        let err = bam.add_fact_radians(&bad, &bad, &bad);
        assert!(matches!(err, Err(AetherError::Shape { .. }) | Err(_)));
    }
}
