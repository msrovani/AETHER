//! Memória holográfica BAM (FHRR em fase inteira `u16`).
//!
//! Mesma semântica do `FhrrStore` (Plate 2003) — `K` traços com fase aleatória
//! por (fato, traço), *cleanup memory* do papel, *unbinding* coerente,
//! similaridade com os valores — mas com fases em BAM (ver
//! [`crate::core::bam_phase`]) e **zero alocação** em [`query_into`].
//!
//! ```text
//! M_t[j]  = Σ_k e^{i(θe + θr + θv + ψ_{k,t})}            (K traços, Q15 → i16)
//! cleanup = argmax_k Σ_{j∈visível} tri(cue_j − role_{k,j}) (onda triangular)
//! z[j]    = Σ_t M_t[j] · e^{-i(role_k + ψ_{k,t})[j]}      (poly_sin_q14)
//! sim_v   = Re⟨z, e^{iθ_v}⟩ / norma                       (vpmaddwd, Q12)
//! ```
//!
//! ## Semântica de similaridade
//!
//! * `role_similarity` usa a **onda triangular** `1 − 2|θ|/π` (não o cosseno):
//!   no papel puro vale 1 (`θ = 0`) e ≈ 0 para fases aleatórias.
//! * `similarity`/`margin` dos valores usam o produto interno complexo
//!   normalizado (≈ 1 em recuperação limpa).
//!
//! ## Limites
//!
//! * `D % 16 == 0`; `K ∈ [1, 15]` (acumulador `i16` do *unbinding*).
//! * Quantização: traços em `i16` com folga de 12 bits; ruído de
//!   quantização ≪ crosstalk de superposição (`≈ √(N/K)` unidades).
//! * [`BamHolographicStore::add_fact`] requantiza (`O(K·D)`): é caminho de
//!   construção, não de consulta.
//!
//! [`query_into`]: BamHolographicStore::query_into

#![allow(clippy::needless_range_loop)]

use crate::core::bam_phase::{
    dot_i16, poly_sin_q14, tri_sum_q14, BamHolographicVector, BamLut, BamMask, BamPhase, LANES,
    Q14_ONE, QUARTER,
};
use crate::core::silicon_entropy::splitmix64;
use crate::error::{AetherError, Result};

/// Máximo de traços suportados (acumulador `i16` do *unbinding*).
pub const MAX_TRACES: usize = 15;
/// Escala Q das tabelas `cos/sin` dos valores (`1.0 = 4096`).
const Q12_ONE: f32 = 4096.0;

/// Valor armazenado: `cos/sin` pré-computados em Q12.
#[derive(Clone, Debug)]
#[repr(C, align(32))]
struct StoredValue<const D: usize> {
    cos: [i16; D],
    sin: [i16; D],
}

/// Traço quantizado (Q arbitrária comum a todos os traços).
#[derive(Clone, Debug)]
#[repr(C, align(32))]
struct QuantTrace<const D: usize> {
    re: [i16; D],
    im: [i16; D],
}

/// Resultado escalar de [`BamHolographicStore::query_into`] (sem alocação).
#[derive(Clone, Copy, Debug)]
pub struct BamHit {
    /// Fato eleito pela *cleanup memory* do papel.
    pub fact_index: usize,
    /// Valor com maior similaridade.
    pub value_index: usize,
    /// Similaridade do valor vencedor (≈ 1 em recuperação perfeita).
    pub similarity: f32,
    /// Margem para o segundo valor.
    pub margin: f32,
    /// Similaridade triangular do papel da pista com o fato eleito.
    pub role_similarity: f32,
}

/// Workspace pré-alocado de [`BamHolographicStore::query_into`].
#[derive(Clone, Debug)]
pub struct BamScratch<const D: usize> {
    z_re: [i16; D],
    z_im: [i16; D],
    sims: Vec<f32>,
    n_sims: usize,
}

impl<const D: usize> BamScratch<D> {
    /// Similaridades com todos os valores da última consulta.
    pub fn sims(&self) -> &[f32] {
        &self.sims[..self.n_sims]
    }
}

/// Memória holográfica BAM (`entidade ⊗ relação ⊗ valor`, `K` traços).
#[derive(Debug)]
pub struct BamHolographicStore<const D: usize> {
    traces: usize,
    /// Acumuladores `K × D` em Q15 (construção).
    acc_re: Vec<i32>,
    acc_im: Vec<i32>,
    /// Traços quantizados para a consulta.
    quant: Vec<QuantTrace<D>>,
    /// Deslocamento de quantização (`acc >> shift`).
    shift: u32,
    roles: Vec<BamHolographicVector<D>>,
    values: Vec<StoredValue<D>>,
    /// Fase aleatória `ψ` por (fato, traço), em BAM.
    psi: Vec<u16>,
    phase_rng: u64,
}

impl<const D: usize> BamHolographicStore<D> {
    /// Cria o store com `traces ∈ [1, 15]` traços randomizados.
    pub fn new(traces: usize) -> Result<Self> {
        let () = BamHolographicVector::<D>::VALID;
        if traces == 0 || traces > MAX_TRACES {
            return Err(AetherError::Config(format!(
                "BAM: traces deve estar em 1..={MAX_TRACES}"
            )));
        }
        Ok(Self {
            traces,
            acc_re: vec![0; traces * D],
            acc_im: vec![0; traces * D],
            quant: vec![
                QuantTrace {
                    re: [0; D],
                    im: [0; D]
                };
                traces
            ],
            shift: 0,
            roles: Vec::new(),
            values: Vec::new(),
            psi: Vec::new(),
            phase_rng: 0xBA3_F00D ^ ((D as u64) << 20) ^ traces as u64,
        })
    }

    /// Número de fatos.
    pub fn len(&self) -> usize {
        self.roles.len()
    }

    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    /// Número de traços `K`.
    pub fn traces(&self) -> usize {
        self.traces
    }

    /// Bytes ocupados pelos buffers de consulta (papéis, valores, traços).
    pub fn memory_bytes(&self) -> usize {
        self.roles.len() * std::mem::size_of::<BamHolographicVector<D>>()
            + self.values.len() * std::mem::size_of::<StoredValue<D>>()
            + self.quant.len() * std::mem::size_of::<QuantTrace<D>>()
            + self.psi.len() * 2
    }

    /// Grava `e ⊙ r ⊙ v` nos `K` traços (cada um com sua fase `ψ`).
    pub fn add_fact(
        &mut self,
        entity: &BamHolographicVector<D>,
        relation: &BamHolographicVector<D>,
        value: &BamHolographicVector<D>,
    ) -> usize {
        let lut = BamLut::global();
        let role = entity.bind(relation);
        let theta = role.bind(value);
        for t in 0..self.traces {
            let psi = if t == 0 {
                0u16
            } else {
                (splitmix64(&mut self.phase_rng) >> 48) as u16
            };
            self.psi.push(psi);
            let base = t * D;
            for j in 0..D {
                let (s, c) = lut.sin_cos_q15(BamPhase(theta.phases[j].wrapping_add(psi)));
                self.acc_re[base + j] += c as i32;
                self.acc_im[base + j] += s as i32;
            }
        }
        // Valor: cos/sin em Q12 para o produto interno `pmaddwd`.
        let mut sv = StoredValue {
            cos: [0; D],
            sin: [0; D],
        };
        for j in 0..D {
            let (s, c) = lut.sin_cos_q15(BamPhase(value.phases[j]));
            sv.cos[j] = ((c as f32) * (Q12_ONE / 32767.0)).round() as i16;
            sv.sin[j] = ((s as f32) * (Q12_ONE / 32767.0)).round() as i16;
        }
        self.roles.push(role);
        self.values.push(sv);
        self.requantize();
        self.roles.len() - 1
    }

    /// Requantiza os acumuladores para `i16` com folga (`|M| ≤ 4095`).
    fn requantize(&mut self) {
        let max = self
            .acc_re
            .iter()
            .chain(self.acc_im.iter())
            .map(|v| v.unsigned_abs())
            .max()
            .unwrap_or(0);
        let bits = 32 - max.leading_zeros();
        self.shift = bits.saturating_sub(12);
        for t in 0..self.traces {
            let base = t * D;
            for j in 0..D {
                self.quant[t].re[j] = (self.acc_re[base + j] >> self.shift) as i16;
                self.quant[t].im[j] = (self.acc_im[base + j] >> self.shift) as i16;
            }
        }
    }

    /// Cria o workspace (aloca **uma vez**).
    pub fn scratch(&self, max_values: usize) -> BamScratch<D> {
        BamScratch {
            z_re: [0; D],
            z_im: [0; D],
            sims: vec![0.0; max_values.max(self.values.len())],
            n_sims: 0,
        }
    }

    /// Consulta **zero-alocação**: *cleanup* do papel `e ⊙ r` (mascarado),
    /// *unbinding* coerente dos `K` traços e similaridade com os valores.
    pub fn query_into(
        &self,
        entity_cue: &BamHolographicVector<D>,
        relation_cue: &BamHolographicVector<D>,
        mask: &BamMask<D>,
        sc: &mut BamScratch<D>,
    ) -> Result<BamHit> {
        if self.is_empty() {
            return Err(AetherError::Config("BAM: consulta em memoria vazia".into()));
        }
        if sc.sims.len() < self.values.len() {
            return Err(AetherError::shape([self.values.len()], [sc.sims.len()]));
        }
        let cue = entity_cue.bind(relation_cue);

        // 1. Cleanup memory do papel (fase pura, 16 fasores/lote).
        let (best_k, best_s) = self.cleanup(&cue, mask);
        let role_similarity = if mask.visible == 0 {
            0.0
        } else {
            best_s as f32 / (Q14_ONE as f32 * mask.visible as f32)
        };

        // 2. Unbinding coerente dos K traços do fato eleito.
        self.unbind_into(best_k, sc);

        // 3. Similaridade com todos os valores (`pmaddwd`).
        let unit = (32767i32 >> self.shift) as f32;
        let norm = 1.0 / (D as f32 * self.traces as f32 * (unit * 0.25 / 16.0) * Q12_ONE);
        let mut best_v = 0usize;
        let mut best_sim = f32::NEG_INFINITY;
        let mut second = f32::NEG_INFINITY;
        for (k, v) in self.values.iter().enumerate() {
            let s = dot_i16(&sc.z_re, &v.cos) + dot_i16(&sc.z_im, &v.sin);
            let sim = s as f32 * norm;
            sc.sims[k] = sim;
            if sim > best_sim {
                second = best_sim;
                best_sim = sim;
                best_v = k;
            } else if sim > second {
                second = sim;
            }
        }
        sc.n_sims = self.values.len();
        let margin = if sc.n_sims > 1 {
            best_sim - second
        } else {
            best_sim
        };
        Ok(BamHit {
            fact_index: best_k,
            value_index: best_v,
            similarity: best_sim,
            margin,
            role_similarity,
        })
    }

    /// Fato cujo papel melhor casa com a pista (soma triangular Q14).
    #[inline]
    fn cleanup(&self, cue: &BamHolographicVector<D>, mask: &BamMask<D>) -> (usize, i32) {
        let mut best_k = 0usize;
        let mut best_s = i32::MIN;
        for (k, role) in self.roles.iter().enumerate() {
            let s = tri_sum_q14(&cue.phases, &role.phases, &mask.lanes);
            if s > best_s {
                best_s = s;
                best_k = k;
            }
        }
        (best_k, best_s)
    }

    /// `z = Σ_t M_t · e^{-i(role_k + ψ_{k,t})}`, requantizado `>> 4`.
    #[inline]
    fn unbind_into(&self, k: usize, sc: &mut BamScratch<D>) {
        sc.z_re.fill(0);
        sc.z_im.fill(0);
        let role = &self.roles[k].phases;
        let psi_base = k * self.traces;
        for t in 0..self.traces {
            let psi = self.psi[psi_base + t];
            let tr = &self.quant[t];
            for ((((c_role, c_re), c_im), z_re), z_im) in role
                .chunks_exact(LANES)
                .zip(tr.re.chunks_exact(LANES))
                .zip(tr.im.chunks_exact(LANES))
                .zip(sc.z_re.chunks_exact_mut(LANES))
                .zip(sc.z_im.chunks_exact_mut(LANES))
            {
                let mut sa = [0i16; LANES];
                let mut ca = [0i16; LANES];
                for l in 0..LANES {
                    let phi = c_role[l].wrapping_add(psi);
                    sa[l] = phi as i16;
                    ca[l] = phi.wrapping_add(QUARTER) as i16;
                }
                let s = poly_sin_q14(&sa);
                let c = poly_sin_q14(&ca);
                for l in 0..LANES {
                    let (mr, mi) = (c_re[l] as i32, c_im[l] as i32);
                    let (cc, ss) = (c[l] as i32, s[l] as i32);
                    z_re[l] += (((mr * cc) >> 16) + ((mi * ss) >> 16)) as i16;
                    z_im[l] += (((mi * cc) >> 16) - ((mr * ss) >> 16)) as i16;
                }
            }
        }
        // z ≤ 2^15 ⇒ z>>4 mantém os produtos com Q12 dentro de i32.
        for (zr, zi) in sc.z_re.iter_mut().zip(sc.z_im.iter_mut()) {
            *zr >>= 4;
            *zi >>= 4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::silicon_entropy::SiliconEntropy;

    #[test]
    fn recall_holografico_com_oclusao() {
        const D: usize = 256;
        const FACTS: usize = 50;
        let mut rng = SiliconEntropy::from_seed(0xFAC7);
        let mut store = BamHolographicStore::<D>::new(6).expect("store");
        let mut atoms = Vec::new();
        for _ in 0..FACTS {
            let e = BamHolographicVector::<D>::random(&mut rng);
            let r = BamHolographicVector::<D>::random(&mut rng);
            let v = BamHolographicVector::<D>::random(&mut rng);
            store.add_fact(&e, &r, &v);
            atoms.push((e, r));
        }
        let mut sc = store.scratch(FACTS);
        let mut mask_b = vec![false; D];
        for (i, m) in mask_b.iter_mut().enumerate() {
            *m = i % 5 < 2; // 40 % visíveis
        }
        let mask = BamMask::<D>::from_bools(&mask_b).expect("mask");
        for (k, (e, r)) in atoms.iter().enumerate() {
            let hit = store.query_into(e, r, &mask, &mut sc).expect("query");
            assert_eq!(hit.value_index, k, "recall@1 perfeito no cenario limpo");
            assert!(hit.role_similarity > 0.95);
        }
    }

    #[test]
    fn rejeita_traces_fora_do_intervalo() {
        assert!(BamHolographicStore::<64>::new(0).is_err());
        assert!(BamHolographicStore::<64>::new(MAX_TRACES + 1).is_err());
    }
}
