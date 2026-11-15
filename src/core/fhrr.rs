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
//! `M_t += e^{i(θ_e + θ_r + θ_v + φ_t)}` em `K` traços com offsets
//! independentes `φ_t`. A consulta desvincula com o papel recuperado por uma
//! *cleanup memory* e escolhe o valor de maior similaridade. Os `K` traços
//! somam o sinal de forma coerente e o crosstalk de forma incoerente, o que
//! reduz o ruído de superposição por `√K` (superposição redundante de VSA) —
//! é o que permite 50 fatos com `D = 384` sem atrator espúrio.
//!
//! Referências:
//! * T. A. Plate, *Holographic Reduced Representations*, IEEE TNN 1995;
//!   *Holographic Reduced Representation*, CSLI 2003.
//! * D. Kleyko et al., *A Survey on Hyperdimensional Computing aka VSA*,
//!   arXiv:2111.06077 — FHRR: binding Hadamard, unbinding conjugado, cleanup.
//! * Schlegel et al., *Comparison of VSAs*, arXiv:2001.11797.
//! * Predição seletiva/abstenção: arXiv:2203.00211 e arXiv:2601.00138
//!   (confiança limitada pela evidência observada).
//!
//! Nada aqui usa texto, índices B-Tree ou alocação no caminho de leitura além
//! dos buffers de saída.

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

    /// `(1/|visible|) Σ_{j∈visible} Re(a_j · conj(b_j))`.
    fn similarity_visible(&self, other: &Self, visible: &[usize]) -> f32 {
        let mut acc = 0.0f32;
        for &j in visible {
            acc += self.re[j] * other.re[j] + self.im[j] * other.im[j];
        }
        if visible.is_empty() {
            0.0
        } else {
            acc / visible.len() as f32
        }
    }
}

/// Resultado de uma consulta holográfica (com as fases recuperadas).
#[derive(Clone, Debug)]
pub struct FhrrRecall {
    /// Fases recuperadas do valor (`arg` do vetor desvinculado).
    pub value: Vec<f32>,
    /// Índice do fato cujo papel `e ⊙ r` venceu a cleanup memory.
    pub fact_index: usize,
    /// Entidade do fato eleito.
    pub entity_index: usize,
    /// Relação do fato eleito.
    pub relation_index: usize,
    /// Valor com maior similaridade.
    pub value_index: usize,
    /// Similaridade do valor vencedor.
    pub similarity: f32,
    /// Margem para o segundo valor (confiança de seleção).
    pub margin: f32,
    /// Similaridade da entidade eleita com a pista parcial.
    pub entity_similarity: f32,
    /// Similaridade da relação eleita com a pista parcial.
    pub relation_similarity: f32,
    /// Evidência: similaridade do papel `e ⊙ r` da pista com o fato eleito.
    pub role_similarity: f32,
    /// Similaridade com todos os valores (SNR/calibração).
    pub value_similarities: Vec<f32>,
}

/// Resultado compacto (sem as fases recuperadas), para o caminho de baixa
/// latência.
#[derive(Clone, Debug)]
pub struct FhrrMatch {
    /// Índice do fato cujo papel `e ⊙ r` venceu a cleanup memory.
    pub fact_index: usize,
    /// Entidade do fato eleito.
    pub entity_index: usize,
    /// Relação do fato eleito.
    pub relation_index: usize,
    /// Valor com maior similaridade.
    pub value_index: usize,
    /// Similaridade do valor vencedor.
    pub similarity: f32,
    /// Margem para o segundo valor.
    pub margin: f32,
    /// Similaridade da entidade eleita com a pista parcial.
    pub entity_similarity: f32,
    /// Similaridade da relação eleita com a pista parcial.
    pub relation_similarity: f32,
    /// Evidência: similaridade do papel da pista com o fato eleito.
    pub role_similarity: f32,
    /// Similaridade com todos os valores.
    pub value_similarities: Vec<f32>,
}

impl FhrrMatch {
    /// Confiança de seleção (margem entre 1º e 2º valores).
    pub fn confidence(&self) -> f32 {
        self.margin.max(0.0)
    }
}

/// Resultado escalar de [`FhrrStore::query_into`] (sem alocação).
#[derive(Clone, Copy, Debug)]
pub struct FhrrHit {
    /// Fato eleito pela cleanup memory.
    pub fact_index: usize,
    /// Valor com maior similaridade.
    pub value_index: usize,
    /// Similaridade do valor vencedor.
    pub similarity: f32,
    /// Margem para o segundo valor.
    pub margin: f32,
    /// Evidência: similaridade do papel da pista com o fato eleito.
    pub role_similarity: f32,
}

/// Workspace pré-alocado de [`FhrrStore::query_into`].
#[derive(Clone, Debug)]
pub struct FhrrScratch {
    cue_re: Vec<f32>,
    cue_im: Vec<f32>,
    z_re: Vec<f32>,
    z_im: Vec<f32>,
    sims: Vec<f32>,
    n_sims: usize,
}

impl FhrrScratch {
    /// Similaridades com todos os valores da última consulta.
    pub fn sims(&self) -> &[f32] {
        &self.sims[..self.n_sims]
    }
}

/// Produto interno em blocos de 8 (vetoriza; sem normas redundantes).
#[inline]
fn dot8(a: &[f32], b: &[f32]) -> f32 {
    let mut lane = [0.0f32; 8];
    let ca = a.chunks_exact(8);
    let cb = b.chunks_exact(8);
    let (ra, rb) = (ca.remainder(), cb.remainder());
    for (x, y) in ca.zip(cb) {
        for l in 0..8 {
            lane[l] += x[l] * y[l];
        }
    }
    let mut acc = 0.0f32;
    for (x, y) in ra.iter().zip(rb) {
        acc += x * y;
    }
    acc + ((lane[0] + lane[4]) + (lane[1] + lane[5])) + ((lane[2] + lane[6]) + (lane[3] + lane[7]))
}
/// Memória holográfica de fatos `(entidade, relação, valor)`.
#[derive(Debug)]
pub struct FhrrStore {
    dim: usize,
    traces: usize,
    /// `traces × dim`, ordem C.
    trace_re: Vec<f32>,
    trace_im: Vec<f32>,
    /// Fase aleatória por `(fato, traço)`: decorrelaciona o crosstalk entre
    /// traços; o offset global cancelaria no unbinding e não reduziria ruído.
    fact_phase: Vec<f32>,
    /// cos ψ / sin ψ por (fato, traço) (evita sin_cos na consulta).
    fact_cos: Vec<f32>,
    fact_sin: Vec<f32>,
    /// RNG determinístico das fases por fato/traço.
    phase_rng: u32,
    entities: Vec<Phasor>,
    relations: Vec<Phasor>,
    values: Vec<Phasor>,
    /// `e_k ⊙ r_k` por fato (papel combinado), para cleanup em uma varredura.
    roles: Vec<Phasor>,
    fact_entity: Vec<usize>,
    fact_relation: Vec<usize>,
    fact_value: Vec<usize>,
}

#[inline]
fn xorshift_u32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

impl FhrrStore {
    /// Cria um store de dimensão `dim` com um único traço.
    pub fn new(dim: usize) -> Result<Self> {
        Self::with_traces(dim, 1)
    }

    /// Cria um store com `K` traços randomizados (`K ≥ 1`).
    ///
    /// Cada fato recebe uma fase aleatória independente por traço. No
    /// unbinding, o crosstalk dos demais fatos soma incoerentemente entre os
    /// `K` traços e o sinal do fato consultado soma coerentemente: o ruído de
    /// superposição cai por `√K`. O offset global por traço **não** serviria —
    /// ele cancela no unbinding.
    pub fn with_traces(dim: usize, traces: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("FHRR: dimensao deve ser > 0".into()));
        }
        if traces == 0 {
            return Err(AetherError::Config("FHRR: traces deve ser > 0".into()));
        }
        Ok(Self {
            dim,
            traces,
            trace_re: vec![0.0f32; dim * traces],
            trace_im: vec![0.0f32; dim * traces],
            fact_phase: Vec::new(),
            fact_cos: Vec::new(),
            fact_sin: Vec::new(),
            phase_rng: 0xA37E_0001u32 ^ (dim as u32) ^ ((traces as u32) << 16),
            entities: Vec::new(),
            relations: Vec::new(),
            values: Vec::new(),
            roles: Vec::new(),
            fact_entity: Vec::new(),
            fact_relation: Vec::new(),
            fact_value: Vec::new(),
        })
    }

    /// Dimensão `D`.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Número de traços.
    pub fn traces(&self) -> usize {
        self.traces
    }

    /// Número de fatos gravados.
    pub fn len(&self) -> usize {
        self.fact_value.len()
    }

    /// `true` se nenhum fato foi gravado.
    pub fn is_empty(&self) -> bool {
        self.fact_value.is_empty()
    }

    /// Número de átomos de valor distintos.
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

    /// Grava `entidade ⊗ relação ⊗ valor` nos `K` traços.
    pub fn add_fact(&mut self, entity: &[f32], relation: &[f32], value: &[f32]) -> Result<usize> {
        self.check("entity", entity)?;
        self.check("relation", relation)?;
        self.check("value", value)?;

        let e = Phasor::from_phases(entity);
        let r = Phasor::from_phases(relation);
        let v = Phasor::from_phases(value);

        // Papel combinado e ⊙ r.
        let mut role_re = Vec::with_capacity(self.dim);
        let mut role_im = Vec::with_capacity(self.dim);
        for j in 0..self.dim {
            role_re.push(e.re[j] * r.re[j] - e.im[j] * r.im[j]);
            role_im.push(e.re[j] * r.im[j] + e.im[j] * r.re[j]);
        }

        let mut psis = Vec::with_capacity(self.traces);
        for t in 0..self.traces {
            // Traço 0 é a referência (fase zero); demais decorrelacionam.
            let psi = if t == 0 {
                0.0
            } else {
                let u = xorshift_u32(&mut self.phase_rng);
                (u as f32 / u32::MAX as f32) * std::f32::consts::TAU
            };
            psis.push(psi);
            let (sp, cp) = psi.sin_cos();
            self.fact_cos.push(cp);
            self.fact_sin.push(sp);
            let base = t * self.dim;
            for j in 0..self.dim {
                let th = entity[j] + relation[j] + value[j];
                let (s, c) = th.sin_cos();
                self.trace_re[base + j] += c * cp - s * sp;
                self.trace_im[base + j] += s * cp + c * sp;
            }
        }
        self.fact_phase.extend_from_slice(&psis);

        let ei = self.entities.len();
        let ri = self.relations.len();
        let vi = self.values.len();
        self.entities.push(e);
        self.relations.push(r);
        self.values.push(v);
        self.roles.push(Phasor {
            re: role_re,
            im: role_im,
        });
        self.fact_entity.push(ei);
        self.fact_relation.push(ri);
        self.fact_value.push(vi);
        Ok(self.fact_value.len() - 1)
    }

    /// Desvincula `role` do fato `k` em todos os traços e soma coerentemente.
    fn unbind(&self, role: &Phasor, fact: usize) -> (Vec<f32>, Vec<f32>) {
        let dim = self.dim;
        let mut acc_re = vec![0.0f32; dim];
        let mut acc_im = vec![0.0f32; dim];
        let phase_base = fact * self.traces;
        for t in 0..self.traces {
            let psi = self.fact_phase[phase_base + t];
            let (sp, cp) = psi.sin_cos();
            let base = t * dim;
            for j in 0..dim {
                // papel efetivo = role ⊙ e^{iψ_{k,t}}
                let rr = role.re[j] * cp - role.im[j] * sp;
                let ri = role.re[j] * sp + role.im[j] * cp;
                let tr = self.trace_re[base + j];
                let ti = self.trace_im[base + j];
                // z = trace_t ⊙ conj(papel efetivo)
                acc_re[j] += tr * rr + ti * ri;
                acc_im[j] += ti * rr - tr * ri;
            }
        }
        (acc_re, acc_im)
    }

    fn visible(mask: &[bool]) -> Vec<usize> {
        let mut v = Vec::with_capacity(mask.len());
        for (j, &m) in mask.iter().enumerate() {
            if m {
                v.push(j);
            }
        }
        v
    }

    /// Consulta por `(entidade, relação)` com pista parcial, devolvendo o
    /// valor em fases.
    pub fn query(
        &self,
        entity_cue: &[f32],
        relation_cue: &[f32],
        mask: &[bool],
    ) -> Result<FhrrRecall> {
        let m = self.query_match(entity_cue, relation_cue, mask)?;
        let (zr, zi) = self.unbind(&self.roles[m.fact_index], m.fact_index);
        let value: Vec<f32> = (0..self.dim).map(|j| zi[j].atan2(zr[j])).collect();
        Ok(FhrrRecall {
            value,
            fact_index: m.fact_index,
            entity_index: m.entity_index,
            relation_index: m.relation_index,
            value_index: m.value_index,
            similarity: m.similarity,
            margin: m.margin,
            entity_similarity: m.entity_similarity,
            relation_similarity: m.relation_similarity,
            role_similarity: m.role_similarity,
            value_similarities: m.value_similarities,
        })
    }

    /// Cria o workspace de consulta para até `max_values` átomos de valor.
    ///
    /// Aloca **uma vez**; [`query_into`](Self::query_into) não aloca mais.
    pub fn scratch(&self, max_values: usize) -> FhrrScratch {
        FhrrScratch {
            cue_re: vec![0.0; self.dim],
            cue_im: vec![0.0; self.dim],
            z_re: vec![0.0; self.dim],
            z_im: vec![0.0; self.dim],
            sims: vec![0.0; max_values.max(self.values.len())],
            n_sims: 0,
        }
    }

    /// Consulta **zero-alocação**: cleanup do papel `e ⊙ r` (produto interno
    /// em blocos de 8, pista mascarada ⇒ sem gather), unbinding coerente dos
    /// `K` traços (`z = conj(papel) ⊙ Σ_t M_t e^{-iψ_t}`) e similaridade com
    /// todos os valores em `sc.sims()`.
    pub fn query_into(
        &self,
        entity_cue: &[f32],
        relation_cue: &[f32],
        mask: &[bool],
        sc: &mut FhrrScratch,
    ) -> Result<FhrrHit> {
        if self.is_empty() {
            return Err(AetherError::Config(
                "FHRR: consulta em memoria vazia".into(),
            ));
        }
        self.check("entity_cue", entity_cue)?;
        self.check("relation_cue", relation_cue)?;
        let dim = self.dim;
        if mask.len() != dim || sc.cue_re.len() != dim {
            return Err(AetherError::shape([dim], [mask.len().min(sc.cue_re.len())]));
        }
        if sc.sims.len() < self.values.len() {
            return Err(AetherError::shape([self.values.len()], [sc.sims.len()]));
        }

        // Papel da pista e⊙r: uma única sin_cos por posição visível; as
        // ocultas ficam em zero (não contribuem ao produto interno).
        let mut nvis = 0usize;
        for j in 0..dim {
            if mask[j] {
                let (s, c) = (entity_cue[j] + relation_cue[j]).sin_cos();
                sc.cue_re[j] = c;
                sc.cue_im[j] = s;
                nvis += 1;
            } else {
                sc.cue_re[j] = 0.0;
                sc.cue_im[j] = 0.0;
            }
        }

        // Cleanup memory: fato com maior Re⟨papel_cue, papel_k⟩.
        let mut best_k = 0usize;
        let mut best_s = f32::NEG_INFINITY;
        for (k, role) in self.roles.iter().enumerate() {
            let s = dot8(&sc.cue_re, &role.re) + dot8(&sc.cue_im, &role.im);
            if s > best_s {
                best_s = s;
                best_k = k;
            }
        }
        let role_similarity = if nvis == 0 { 0.0 } else { best_s / nvis as f32 };

        // Unbinding: S = Σ_t M_t e^{-iψ_t}; z = S ⊙ conj(papel_best).
        sc.z_re.fill(0.0);
        sc.z_im.fill(0.0);
        let pbase = best_k * self.traces;
        for t in 0..self.traces {
            let cp = self.fact_cos[pbase + t];
            let sp = self.fact_sin[pbase + t];
            let tr = &self.trace_re[t * dim..(t + 1) * dim];
            let ti = &self.trace_im[t * dim..(t + 1) * dim];
            for j in 0..dim {
                sc.z_re[j] += tr[j] * cp + ti[j] * sp;
                sc.z_im[j] += ti[j] * cp - tr[j] * sp;
            }
        }
        let role = &self.roles[best_k];
        for j in 0..dim {
            let (sr, si) = (sc.z_re[j], sc.z_im[j]);
            sc.z_re[j] = sr * role.re[j] + si * role.im[j];
            sc.z_im[j] = si * role.re[j] - sr * role.im[j];
        }

        // Similaridade com todos os valores (uma normalização final).
        let inv = 1.0f32 / (dim as f32 * self.traces as f32);
        let mut best_v = 0usize;
        let mut best_sim = f32::NEG_INFINITY;
        let mut second = f32::NEG_INFINITY;
        for (k, v) in self.values.iter().enumerate() {
            let s = (dot8(&sc.z_re, &v.re) + dot8(&sc.z_im, &v.im)) * inv;
            sc.sims[k] = s;
            if s > best_sim {
                second = best_sim;
                best_sim = s;
                best_v = k;
            } else if s > second {
                second = s;
            }
        }
        sc.n_sims = self.values.len();
        let margin = if sc.n_sims > 1 {
            best_sim - second
        } else {
            best_sim
        };
        Ok(FhrrHit {
            fact_index: best_k,
            value_index: best_v,
            similarity: best_sim,
            margin,
            role_similarity,
        })
    }

    /// Consulta compacta (sem fases): cleanup do papel, unbinding e
    /// similaridade com todos os valores. Aloca o resultado; para o caminho
    /// quente use [`query_into`](Self::query_into).
    pub fn query_match(
        &self,
        entity_cue: &[f32],
        relation_cue: &[f32],
        mask: &[bool],
    ) -> Result<FhrrMatch> {
        let mut sc = self.scratch(self.values.len());
        let hit = self.query_into(entity_cue, relation_cue, mask, &mut sc)?;
        let e_cue = Phasor::from_phases(entity_cue);
        let r_cue = Phasor::from_phases(relation_cue);
        let visible = Self::visible(mask);
        let fe = self.fact_entity[hit.fact_index];
        let fr = self.fact_relation[hit.fact_index];
        Ok(FhrrMatch {
            fact_index: hit.fact_index,
            entity_index: fe,
            relation_index: fr,
            value_index: hit.value_index,
            similarity: hit.similarity,
            margin: hit.margin,
            entity_similarity: e_cue.similarity_visible(&self.entities[fe], &visible),
            relation_similarity: r_cue.similarity_visible(&self.relations[fr], &visible),
            role_similarity: hit.role_similarity,
            value_similarities: sc.sims[..sc.n_sims].to_vec(),
        })
    }

    /// Consulta com **abstenção seletiva** (*warrant-based confidence*).
    ///
    /// Retorna `None` quando a evidência da pista — similaridade do papel
    /// `e ⊙ r` com o fato eleito — é menor que `min_evidence`. A confiança do
    /// readout é limitada pelo que a entrada observada sustenta
    /// (arXiv:2203.00211; arXiv:2601.00138), em vez de aceitar cegamente o
    /// atrator mais próximo da cleanup memory.
    pub fn query_selective(
        &self,
        entity_cue: &[f32],
        relation_cue: &[f32],
        mask: &[bool],
        min_evidence: f32,
    ) -> Result<Option<FhrrMatch>> {
        let m = self.query_match(entity_cue, relation_cue, mask)?;
        if m.role_similarity < min_evidence {
            Ok(None)
        } else {
            Ok(Some(m))
        }
    }

    /// Bytes ocupados pelos buffers (diagnóstico de pegada).
    pub fn memory_bytes(&self) -> usize {
        (self.trace_re.len() + self.trace_im.len()) * std::mem::size_of::<f32>()
            + self.fact_phase.len() * std::mem::size_of::<f32>()
            + (self.entities.len() + self.relations.len() + self.values.len() + self.roles.len())
                * self.dim
                * 2
                * std::mem::size_of::<f32>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phasors(n: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
        let mut state = (seed | 1) as u32;
        (0..n)
            .map(|_| {
                (0..dim)
                    .map(|_| {
                        let u = xorshift_u32(&mut state);
                        (u as f32 / u32::MAX as f32) * std::f32::consts::TAU
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn fhrr_bind_unbind_e_chave_ausente() {
        let dim = 512;
        let mut store = FhrrStore::with_traces(dim, 2).expect("store");
        let entities = phasors(20, dim, 0xA11CE);
        let relations = phasors(20, dim, 0xB0B);
        let values = phasors(20, dim, 0xC0FFEE);
        for k in 0..20 {
            store
                .add_fact(&entities[k], &relations[k], &values[k])
                .expect("add");
        }
        let mask = vec![true; dim];
        for k in 0..20 {
            let r = store
                .query(&entities[k], &relations[k], &mask)
                .expect("query");
            assert_eq!(r.value_index, k, "fato {k} recuperou {}", r.value_index);
            assert!(r.similarity > 0.5, "sim={}", r.similarity);
            assert!(r.margin > 0.2, "margem={}", r.margin);
            assert!(r.role_similarity > 0.9, "evidencia={}", r.role_similarity);
        }
        // Chave ausente: papel aleatorio nao tem evidencia -> abstenção.
        let absent_e = phasors(1, dim, 0xDEAD)[0].clone();
        let absent_r = phasors(1, dim, 0xBEEF)[0].clone();
        let gated = store
            .query_selective(&absent_e, &absent_r, &mask, 0.25)
            .expect("query");
        assert!(gated.is_none(), "chave ausente deveria absterse");
    }
}
