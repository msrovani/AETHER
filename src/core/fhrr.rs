//! FHRR — Fourier Holographic Reduced Representations.
//!
//! Implementação do modelo de Plate (1995/2003) para *Vector Symbolic
//! Architectures*: cada símbolo é um hipervetor de fasores unitários
//! `e^{iθ_j}`; **binding** é o produto de Hadamard (`⊙`, soma de fases) e
//! **unbinding** é o produto pelo conjugado (subtração de fases). O
//! **bundling** é a soma complexa, e a similaridade é
//! `Re(⟨a,b⟩)/D = (1/D)Σ cos(θ_a,j − θ_b,j)`.
//!
//! Um fato `ENTIDADE ⊗ RELAÇÃO ⊗ VALOR` é gravado como
//! `M += e^{i(θ_e + θ_r + θ_v)}`. A consulta desvincula com a entidade/relação
//! recuperadas por uma *cleanup memory* (item memory de fasores) e escolhe o
//! valor de maior similaridade. Isso dá recuperação **O(D)** por consulta, sem
//! atratores espúrios: uma chave inexistente produz dessincronia de fase, não
//! um padrão confiante — o oposto do que ocorre em memórias atratoras.
//!
//! Referências:
//! * T. A. Plate, *Holographic Reduced Representations*, IEEE TNN 1995;
//!   *Holographic Reduced Representation: Distributed Representation for
//!   Cognitive Structures*, CSLI 2003.
//! * D. Kleyko et al., *A Survey on Hyperdimensional Computing aka VSA*,
//!   arXiv:2111.06077 (2022) — FHRR: binding Hadamard, unbinding conjugado,
//!   cleanup memory.
//! * Schlegel et al., *Comparison of VSAs*, arXiv:2001.11797.
//!
//! Nada aqui usa texto, índices B-Tree ou alocação no caminho de leitura além
//! do vetor de saída e dos escores de cleanup.

#![allow(clippy::needless_range_loop)]

use crate::error::{AetherError, Result};

/// Fasor de dimensão `D` em coordenadas retangulares (pré-computadas).
#[derive(Clone, Debug)]
struct Phasor {
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Phasor {
    fn from_phases(phases: &[f32]) -> Self {
        let mut re = Vec::with_capacity(phases.len());
        let mut im = Vec::with_capacity(phases.len());
        for &p in phases {
            let (s, c) = p.sin_cos();
            re.push(c);
            im.push(s);
        }
        Self { re, im }
    }

    /// `(1/|mask|) Σ_{j∈mask} Re(a_j · conj(b_j))`.
    fn similarity_masked(&self, other: &Self, mask: &[bool]) -> f32 {
        let n = self.re.len();
        let mut acc = 0.0f32;
        let mut count = 0usize;
        for j in 0..n {
            if mask[j] {
                acc += self.re[j] * other.re[j] + self.im[j] * other.im[j];
                count += 1;
            }
        }
        if count == 0 {
            0.0
        } else {
            acc / count as f32
        }
    }

    fn similarity(&self, other: &Self) -> f32 {
        let n = self.re.len().min(other.re.len());
        let mut acc = 0.0f32;
        for j in 0..n {
            acc += self.re[j] * other.re[j] + self.im[j] * other.im[j];
        }
        if n == 0 {
            0.0
        } else {
            acc / n as f32
        }
    }
}

/// Resultado de uma consulta holográfica.
#[derive(Clone, Debug)]
pub struct FhrrRecall {
    /// Fases recuperadas do valor (`arg` do vetor desvinculado), `D` amostras.
    pub value: Vec<f32>,
    /// Índice da entidade escolhida pela cleanup memory.
    pub entity_index: usize,
    /// Índice da relação escolhida pela cleanup memory.
    pub relation_index: usize,
    /// Índice do valor com maior similaridade.
    pub value_index: usize,
    /// Similaridade do valor vencedor (0..1 típico).
    pub similarity: f32,
    /// Margem para o segundo colocado (confiança de seleção).
    pub margin: f32,
    /// Similaridade da entidade vencedora com a pista parcial.
    pub entity_similarity: f32,
    /// Similaridade da relação vencedora com a pista parcial.
    pub relation_similarity: f32,
    /// Similaridade com **todos** os valores gravados (para SNR/calibração).
    pub value_similarities: Vec<f32>,
}

impl FhrrRecall {
    /// Confiança de seleção: margem entre o melhor e o segundo valor.
    pub fn confidence(&self) -> f32 {
        self.margin.max(0.0)
    }
}

/// Memória holográfica de fatos `(entidade, relação, valor)`.
#[derive(Debug)]
pub struct FhrrStore {
    dim: usize,
    trace_re: Vec<f32>,
    trace_im: Vec<f32>,
    entities: Vec<Phasor>,
    relations: Vec<Phasor>,
    values: Vec<Phasor>,
    fact_entity: Vec<usize>,
    fact_relation: Vec<usize>,
    fact_value: Vec<usize>,
}

impl FhrrStore {
    /// Cria um store vazio de dimensão `dim` (recomendado `D ≥ 256`).
    pub fn new(dim: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config(
                "FHRR: dimensao deve ser > 0".into(),
            ));
        }
        Ok(Self {
            dim,
            trace_re: vec![0.0f32; dim],
            trace_im: vec![0.0f32; dim],
            entities: Vec::new(),
            relations: Vec::new(),
            values: Vec::new(),
            fact_entity: Vec::new(),
            fact_relation: Vec::new(),
            fact_value: Vec::new(),
        })
    }

    /// Dimensão `D`.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Número de fatos gravados.
    pub fn len(&self) -> usize {
        self.fact_value.len()
    }

    /// `true` se nenhum fato foi gravado.
    pub fn is_empty(&self) -> bool {
        self.fact_value.is_empty()
    }

    /// Número de átomos de valor distintos (tamanho da cleanup memory).
    pub fn value_atoms(&self) -> usize {
        self.values.len()
    }

    fn check(&self, name: &str, v: &[f32]) -> Result<()> {
        if v.len() != self.dim {
            return Err(AetherError::shape([self.dim], [v.len()]));
        }
        if let Some(i) = v.iter().position(|x| !x.is_finite()) {
            return Err(AetherError::Degenerate(format!(
                "FHRR: {name}[{i}] nao finito"
            )));
        }
        Ok(())
    }

    /// Grava `entidade ⊗ relação ⊗ valor` no traço holográfico.
    ///
    /// Retorna o índice do fato. Átomos repetidos são permitidos: a cleanup
    /// memory resolve empates pela similaridade.
    pub fn add_fact(
        &mut self,
        entity: &[f32],
        relation: &[f32],
        value: &[f32],
    ) -> Result<usize> {
        self.check("entity", entity)?;
        self.check("relation", relation)?;
        self.check("value", value)?;

        let e = Phasor::from_phases(entity);
        let r = Phasor::from_phases(relation);
        let v = Phasor::from_phases(value);

        let ei = self.entities.len();
        let ri = self.relations.len();
        let vi = self.values.len();

        for j in 0..self.dim {
            let th = entity[j] + relation[j] + value[j];
            let (s, c) = th.sin_cos();
            self.trace_re[j] += c;
            self.trace_im[j] += s;
        }

        self.entities.push(e);
        self.relations.push(r);
        self.values.push(v);
        self.fact_entity.push(ei);
        self.fact_relation.push(ri);
        self.fact_value.push(vi);
        Ok(self.fact_value.len() - 1)
    }

    /// Consulta por `(entidade, relação)` com pista parcial.
    ///
    /// `mask[j] == true` indica dimensão observada; as ocultas são ignoradas
    /// na cleanup memory. A desvinculação usa os átomos **completos** eleitos,
    /// de modo que a oclusão não vira ruído de fase no valor recuperado.
    pub fn query(
        &self,
        entity_cue: &[f32],
        relation_cue: &[f32],
        mask: &[bool],
    ) -> Result<FhrrRecall> {
        if self.is_empty() {
            return Err(AetherError::Config(
                "FHRR: consulta em memoria vazia".into(),
            ));
        }
        self.check("entity_cue", entity_cue)?;
        self.check("relation_cue", relation_cue)?;
        if mask.len() != self.dim {
            return Err(AetherError::shape([self.dim], [mask.len()]));
        }

        let e_cue = Phasor::from_phases(entity_cue);
        let r_cue = Phasor::from_phases(relation_cue);

        // Cleanup memory: entidade e relação mais próximas da pista parcial.
        let mut best_e = 0usize;
        let mut best_es = f32::NEG_INFINITY;
        for (k, e) in self.entities.iter().enumerate() {
            let s = e_cue.similarity_masked(e, mask);
            if s > best_es {
                best_es = s;
                best_e = k;
            }
        }
        let mut best_r = 0usize;
        let mut best_rs = f32::NEG_INFINITY;
        for (k, r) in self.relations.iter().enumerate() {
            let s = r_cue.similarity_masked(r, mask);
            if s > best_rs {
                best_rs = s;
                best_r = k;
            }
        }

        // z = M ⊙ conj(entidade_best ⊙ relação_best)  (unbinding FHRR).
        let e = &self.entities[best_e];
        let r = &self.relations[best_r];
        let mut value_re = vec![0.0f32; self.dim];
        let mut value_im = vec![0.0f32; self.dim];
        for j in 0..self.dim {
            let role_re = e.re[j] * r.re[j] - e.im[j] * r.im[j];
            let role_im = e.re[j] * r.im[j] + e.im[j] * r.re[j];
            // z = trace * conj(role)
            value_re[j] = self.trace_re[j] * role_re + self.trace_im[j] * role_im;
            value_im[j] = self.trace_im[j] * role_re - self.trace_re[j] * role_im;
        }

        // Similaridade com todos os valores (cleanup) + margem.
        let mut sims = vec![0.0f32; self.values.len()];
        for (k, v) in self.values.iter().enumerate() {
            let mut acc = 0.0f32;
            for j in 0..self.dim {
                acc += value_re[j] * v.re[j] + value_im[j] * v.im[j];
            }
            sims[k] = acc / self.dim as f32;
        }
        let mut best_v = 0usize;
        let mut second = f32::NEG_INFINITY;
        for (k, &s) in sims.iter().enumerate() {
            if s > sims[best_v] {
                second = sims[best_v];
                best_v = k;
            } else if s > second {
                second = s;
            }
        }
        let margin = if sims.len() > 1 {
            sims[best_v] - second
        } else {
            sims[best_v]
        };

        let value: Vec<f32> = (0..self.dim)
            .map(|j| value_im[j].atan2(value_re[j]))
            .collect();

        Ok(FhrrRecall {
            value,
            entity_index: best_e,
            relation_index: best_r,
            value_index: best_v,
            similarity: sims[best_v],
            margin,
            entity_similarity: best_es,
            relation_similarity: best_rs,
            value_similarities: sims,
        })
    }

    /// Bytes ocupados pelos buffers (diagnóstico de pegada).
    pub fn memory_bytes(&self) -> usize {
        (self.trace_re.len() + self.trace_im.len()) * std::mem::size_of::<f32>()
            + (self.entities.len() + self.relations.len() + self.values.len())
                * self.dim
                * 2
                * std::mem::size_of::<f32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phasors(n: usize, seed: u64) -> Vec<Vec<f32>> {
        let mut state = seed | 1;
        (0..n)
            .map(|_| {
                (0..64)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        ((state >> 40) as f32) / ((1u32 << 24) as f32) * std::f32::consts::TAU
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fhrr_bind_unbind_e_chave_ausente() {
        let dim = 512;
        let mut store = FhrrStore::new(dim).expect("store");
        let entities = phasors(20, 0xA11CE);
        let relations = phasors(20, 0xB0B);
        let values = phasors(20, 0xC0FFEE);
        for k in 0..20 {
            store
                .add_fact(&entities[k], &relations[k], &values[k])
                .expect("add");
        }
        let mask = vec![true; dim];
        for k in 0..20 {
            let r = store.query(&entities[k], &relations[k], &mask).expect("query");
            assert_eq!(r.value_index, k, "fato {k} recuperou {}", r.value_index);
            assert!(r.similarity > 0.5, "sim={}", r.similarity);
            assert!(r.margin > 0.2, "margem={}", r.margin);
        }
        let absent = store.query(&entities[0], &relations[1], &mask).expect("query");
        assert_ne!(absent.value_index, 0);
        assert!(absent.entity_similarity > 0.9);
        assert!(absent.relation_similarity > 0.9);
    }
}
