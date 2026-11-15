//! Protótipo FHRR em aritmética angular binária (BAM) — **experimento isolado**.
//!
//! Não substitui [`crate::core::fhrr::FhrrStore`]. Reutiliza
//! [`BamHolographicStore`] (`bam_phase`) e expõe:
//! * conversão `f32` radianos ↔ BAM;
//! * breakdown de estágios da consulta (cue / cleanup / unbind / similarity);
//! * veredito de integração (`Recall@1 == 1.0` **e** ganho e2e ≥ 1.5×).
//!
//! Critério (OPENCODE MSG 0081 / autorização Fase 1): se o ganho líquido for
//! `< 1.5×` frente ao dot-product `f32` FMA do `FhrrStore`, permanece só como
//! experimento documentado — sem poluir o núcleo.

#![allow(clippy::needless_range_loop)]

use std::time::Instant;

use crate::core::bam_phase::{
    BamHit, BamHolographicStore, BamHolographicVector, BamMask, BamScratch, LANES,
};
use crate::core::fhrr::FhrrStore;
use crate::error::Result;

/// Dimensão alinhada ao harness T12 (`cognitive_audit`, D=160, K=6).
pub const T12_DIM: usize = 160;
/// Traços do harness T12.
pub const T12_TRACES: usize = 6;
/// Fatos do harness T12.
pub const T12_FACTS: usize = 50;
/// Critério mínimo de speedup e2e para integração futura.
pub const INTEGRATION_SPEEDUP: f64 = 1.5;

/// Nanossundos por estágio da consulta BAM (média).
#[derive(Clone, Copy, Debug, Default)]
pub struct StageNs {
    /// Construção do papel da pista (`e ⊙ r` + máscara).
    pub cue_ns: f64,
    /// Cleanup memory (50×D similaridades de papel).
    pub cleanup_ns: f64,
    /// Unbinding coerente dos K traços.
    pub unbind_ns: f64,
    /// Similaridade com todos os valores (50×D).
    pub similarity_ns: f64,
    /// Consulta completa (wall).
    pub total_ns: f64,
}

/// Veredito do experimento lado a lado.
#[derive(Clone, Debug)]
pub struct BamIntegrationReport {
    pub recall_at_1: f64,
    pub fhrr_latency_us: f64,
    pub bam_latency_us: f64,
    pub speedup: f64,
    pub bam_stages: StageNs,
    pub fhrr_stage_proxy: StageNs,
    pub ready_for_integration: bool,
    pub note: &'static str,
}

/// Store BAM tipado no D do T12.
pub type BamT12Store = BamHolographicStore<T12_DIM>;
/// Vetor BAM tipado no D do T12.
pub type BamT12Vec = BamHolographicVector<T12_DIM>;

/// Converte fases em radianos para vetor BAM (len == D).
pub fn phases_to_bam<const D: usize>(rad: &[f32]) -> Result<BamHolographicVector<D>> {
    BamHolographicVector::<D>::from_radians(rad)
}

/// Máscara bool → [`BamMask`].
pub fn mask_to_bam<const D: usize>(mask: &[bool]) -> Result<BamMask<D>> {
    BamMask::<D>::from_bools(mask)
}

/// Micro-proxy dos estágios do `FhrrStore::query_into` (sem editar `fhrr.rs`).
///
/// Replica a aritmética dominante em buffers locais para decompor T12:
/// cue (`sin_cos` visíveis), cleanup (50×D FMA), unbind (K×D), similarity (50×D).
pub fn profile_fhrr_stage_proxy(
    dim: usize,
    traces: usize,
    facts: usize,
    visible_frac: f64,
    rounds: usize,
) -> StageNs {
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
    let mut acc = 0.0f32;

    // Semente barata (não precisa ser NoiseSource aqui).
    for (i, x) in role_re.iter_mut().enumerate() {
        *x = ((i * 1103515245 + 12345) as f32).sin();
    }
    for (i, x) in role_im.iter_mut().enumerate() {
        *x = ((i * 222222 + 7) as f32).cos();
    }
    tr_re.copy_from_slice(&role_re[..tr_re.len().min(role_re.len())]);
    // fill remaining with pattern
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
    StageNs {
        cue_ns: cue_ns / r,
        cleanup_ns: cleanup_ns / r,
        unbind_ns: unbind_ns / r,
        similarity_ns: sim_ns / r,
        total_ns: (cue_ns + cleanup_ns + unbind_ns + sim_ns) / r,
    }
}

/// Breakdown instrumentado da consulta BAM (chama kernels públicos em fatias).
pub fn profile_bam_query_stages<const D: usize>(
    store: &BamHolographicStore<D>,
    entity: &BamHolographicVector<D>,
    relation: &BamHolographicVector<D>,
    mask: &BamMask<D>,
    sc: &mut BamScratch<D>,
    rounds: usize,
) -> Result<(StageNs, crate::core::bam_phase::BamHit)> {
    // A API pública não expõe estágios; medimos wall total + proxy interno
    // via duas passagens: (1) só bind+máscara, (2) query completa.
    let mut cue_ns = 0.0f64;
    let mut total_ns = 0.0f64;
    let mut last = None;
    for _ in 0..rounds {
        let t0 = Instant::now();
        let _cue = entity.bind(relation);
        let _ = mask.visible();
        cue_ns += t0.elapsed().as_nanos() as f64;

        let t1 = Instant::now();
        let hit = store.query_into(entity, relation, mask, sc)?;
        total_ns += t1.elapsed().as_nanos() as f64;
        last = Some(hit);
    }
    let r = rounds as f64;
    let cue = cue_ns / r;
    let total = total_ns / r;
    // Distribui o restante proporcionalmente ao custo conhecido (cleanup≈sim,
    // unbind≈K·D): proxy para o relatório Fase 0.
    let rest = (total - cue).max(0.0);
    let stages = StageNs {
        cue_ns: cue,
        cleanup_ns: rest * 0.40,
        unbind_ns: rest * 0.25,
        similarity_ns: rest * 0.35,
        total_ns: total,
    };
    Ok((stages, last.expect("rounds > 0")))
}

/// Compara `FhrrStore` × BAM no protocolo T12 (D=160, K=6, 50 fatos, 40% oclusão).
pub fn compare_t12_fhrr_vs_bam(
    queries: usize,
    seed: u64,
) -> Result<BamIntegrationReport> {
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut fhrr = FhrrStore::with_traces(T12_DIM, T12_TRACES)?;
    let mut bam = BamT12Store::new(T12_TRACES)?;
    let mut atoms: Vec<(Vec<f32>, Vec<f32>, Vec<f32>)> = Vec::with_capacity(T12_FACTS);

    for _ in 0..T12_FACTS {
        let e: Vec<f32> = (0..T12_DIM)
            .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
            .collect();
        let r: Vec<f32> = (0..T12_DIM)
            .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
            .collect();
        let v: Vec<f32> = (0..T12_DIM)
            .map(|_| rng.gen::<f32>() * std::f32::consts::TAU)
            .collect();
        fhrr.add_fact(&e, &r, &v)?;
        let eb = phases_to_bam::<T12_DIM>(&e)?;
        let rb = phases_to_bam::<T12_DIM>(&r)?;
        let vb = phases_to_bam::<T12_DIM>(&v)?;
        bam.add_fact(&eb, &rb, &vb);
        atoms.push((e, r, v));
    }

    let mut fsc = fhrr.scratch(T12_FACTS);
    let mut bsc = bam.scratch(T12_FACTS);
    let mut hits_bam = 0usize;
    let mut fhrr_us = Vec::with_capacity(queries);
    let mut bam_us = Vec::with_capacity(queries);
    let mut stage_acc = StageNs::default();

    for q in 0..queries {
        let truth = (rng.gen::<u32>() as usize) % T12_FACTS;
        let mut mask = vec![true; T12_DIM];
        // 40% oclusão determinística.
        let mut bits = 0x7000u64.wrapping_add(q as u64);
        let mut hidden = 0usize;
        let target_hidden = T12_DIM * 40 / 100;
        while hidden < target_hidden {
            bits ^= bits << 13;
            bits ^= bits >> 7;
            bits ^= bits << 17;
            let i = (bits as usize) % T12_DIM;
            if mask[i] {
                mask[i] = false;
                hidden += 1;
            }
        }
        let mut ec = atoms[truth].0.clone();
        let mut rc = atoms[truth].1.clone();
        for i in 0..T12_DIM {
            if mask[i] {
                let noise = (rng.gen::<f32>() - 0.5) * 0.5;
                ec[i] += noise;
                rc[i] -= noise;
            }
        }

        let mut best_f = u128::MAX;
        for _ in 0..8 {
            let t = Instant::now();
            let _ = fhrr.query_into(&ec, &rc, &mask, &mut fsc)?;
            best_f = best_f.min(t.elapsed().as_nanos());
        }
        fhrr_us.push(best_f as f64 / 1000.0);

        let eb = phases_to_bam::<T12_DIM>(&ec)?;
        let rb = phases_to_bam::<T12_DIM>(&rc)?;
        let bm = mask_to_bam::<T12_DIM>(&mask)?;
        let mut best_b = u128::MAX;
        let mut hit_b = BamHit {
            fact_index: 0,
            value_index: 0,
            similarity: 0.0,
            margin: 0.0,
            role_similarity: 0.0,
        };
        for _ in 0..8 {
            let t = Instant::now();
            hit_b = bam.query_into(&eb, &rb, &bm, &mut bsc)?;
            best_b = best_b.min(t.elapsed().as_nanos());
        }
        bam_us.push(best_b as f64 / 1000.0);

        if hit_b.value_index == truth {
            hits_bam += 1;
        }

        if q == 0 {
            let (st, _) = profile_bam_query_stages(&bam, &eb, &rb, &bm, &mut bsc, 64)?;
            stage_acc = st;
        }
    }

    let recall = hits_bam as f64 / queries as f64;
    let fhrr_lat = fhrr_us.iter().sum::<f64>() / queries as f64;
    let bam_lat = bam_us.iter().sum::<f64>() / queries as f64;
    let speedup = if bam_lat > 0.0 {
        fhrr_lat / bam_lat
    } else {
        0.0
    };
    let ready = recall >= 1.0 - 1e-9 && speedup >= INTEGRATION_SPEEDUP;
    let note = if ready {
        "BAM pronto para integracao na proxima rodada (Recall@1=1.0 e speedup>=1.5x)"
    } else if recall >= 0.99 && speedup < INTEGRATION_SPEEDUP {
        "BAM isolado: ganho e2e < 1.5x frente ao FHRR f32 FMA — nao poluir o nucleo"
    } else {
        "BAM isolado: Recall@1 ou speedup abaixo do criterio de aceite"
    };

    let proxy = profile_fhrr_stage_proxy(T12_DIM, T12_TRACES, T12_FACTS, 0.60, 256);

    Ok(BamIntegrationReport {
        recall_at_1: recall,
        fhrr_latency_us: fhrr_lat,
        bam_latency_us: bam_lat,
        speedup,
        bam_stages: stage_acc,
        fhrr_stage_proxy: proxy,
        ready_for_integration: ready,
        note,
    })
}

/// Roofline analítico de banda de memória para referência `wgpu` (Fase 2).
///
/// Modelo: transferir `N×N` matriz `f32` (complex coupling) + `N` fases a
/// `BW_GBs` GB/s efetivos. Não executa GPU — só o teto de latência por banda.
#[derive(Clone, Copy, Debug)]
pub struct WgpuRoofline {
    pub n: usize,
    pub bytes: usize,
    pub t_us_at_100gbs: f64,
    pub t_us_at_400gbs: f64,
    pub t_us_at_900gbs: f64,
}

/// Calcula o roofline para `N ∈ {512,1024,4096}`.
pub fn wgpu_roofline_table() -> [WgpuRoofline; 3] {
    // A_ij complexo f32×2 + fases f32 + omega f32 ≈ 8*N² + 8*N bytes.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d_alinha_lanes() {
        assert_eq!(T12_DIM % LANES, 0);
    }

    #[test]
    fn roofline_monotono() {
        let t = wgpu_roofline_table();
        assert!(t[0].bytes < t[1].bytes && t[1].bytes < t[2].bytes);
        assert!(t[2].t_us_at_100gbs > t[2].t_us_at_900gbs);
    }
}
