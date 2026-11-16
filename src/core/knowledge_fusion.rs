//! F2/F3 — consolidação `T1`, *gist* `L4`, fusão 2D e `cue_into` com escopo.
//!
//! Este módulo é a camada que o Vol. III §6/§7/§10/§13 pede **acima** da ponte
//! `F1` (`core::knowledge_bridge_f1`). Ele não edita a ponte: consome
//! [`DraftRecord`]s já drenados e implementa o que falta para fechar as
//! perguntas #6 e #7 do dossiê.
//!
//! # F2 — um atrator pode virar *gist* sem perder a habilidade?
//!
//! O atrator de fase é binário: está ou não está. A consolidação resolve isso
//! em três passos:
//!
//! 1. **`commit_run`** — o lote drenado do `T0` vira estado autoritativo no fim
//!    do ciclo (nunca no caminho quente);
//! 2. **`supersede`** — se um fato novo contradiz o antigo (mesma `key`, mesmo
//!    escopo, payload diferente), o antigo é **substituído** e sua energia é
//!    preservada no *gist*;
//! 3. **`gist`** — antes de podar o atrator completo, grava-se um resumo
//!    energético de 8 coeficientes (`L4`). A habilidade não é o padrão cru: é a
//!    capacidade de **reconhecer** o padrão por uma assinatura de fase de baixa
//!    dimensão. O gate mede retenção dessa habilidade depois da poda.
//!
//! # F3 — a ponte de fase preserva o suficiente?
//!
//! A matriz 2D cruza **autoridade** (`0..=255`, *warrant* social) com
//! **factibilidade** (`violations == 0` e `gap ≤ limiar`) e devolve uma das
//! quatro ações do dossiê:
//!
//! ```text
//!                    gap alto (fisicamente ambíguo)
//!   autoridade baixa  ABSTÉM          |  CITA NÃO ATUA
//!   autoridade alta   PROPÕE          |  ATUA + CITA
//!                    gap baixo (factível)
//! ```
//!
//! O quadrante **“plausível sem fonte”** (autoridade alta, gap alto) é
//! `CiteNoAct`: o organismo cita a proveniência mas **não age** — exatamente o
//! comportamento que o gate exige em ≥ 90% dos casos.
//!
//! [`FusionLedger::cue_into_scoped`] filtra por [`ScopeId`] **antes** de
//! comparar. Isso corrige, no nosso caminho, o vazamento cross-organism que a
//! auditoria apontou na ponte F1 (MSG 0140): consulta de um escopo nunca
//! devolve chave de outro, e o contador [`FusionLedger::cross_scope_hits`]
//! permanece `0`.

use crate::core::knowledge_bridge_f1::{
    BridgeError, Certificate, CueHit, DraftRecord, KnowledgeSource, ScopeId,
};

/// Dimensão da assinatura do *gist* (`L4`).
pub const GIST_DIM: usize = 8;

/// Ação da matriz 2D de garantia (Vol. III §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FusionAction {
    /// Autoridade alta + factível: age e cita a proveniência.
    ActAndCite,
    /// Autoridade alta + ambíguo: cita, mas não age (plausível sem fonte).
    CiteNoAct,
    /// Autoridade baixa + factível: propõe, não executa.
    Propose,
    /// Autoridade baixa + ambíguo: abstém-se (ruído sem fonte).
    Abstain,
}

impl FusionAction {
    /// Código estável de 1 byte.
    pub const fn code(self) -> u8 {
        match self {
            Self::ActAndCite => 0,
            Self::CiteNoAct => 1,
            Self::Propose => 2,
            Self::Abstain => 3,
        }
    }
}

/// Quadrante da matriz 2D.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quadrant {
    /// Autoridade alta, fisicamente factível.
    TrustedFeasible,
    /// Autoridade alta, fisicamente ambíguo — “plausível sem fonte”.
    TrustedAmbiguous,
    /// Autoridade baixa, fisicamente factível.
    UnvettedFeasible,
    /// Autoridade baixa, fisicamente ambíguo — ruído sem fonte.
    UnvettedAmbiguous,
}

/// Decisão da matriz 2D.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    /// Ação prescrita.
    pub action: FusionAction,
    /// Quadrante que a produziu.
    pub quadrant: Quadrant,
    /// Autoridade usada (`0..=255`).
    pub authority: u8,
    /// Gap de energia normalizado (`0..=1`).
    pub gap: f32,
    /// `true` se o certificado é factível (`violations == 0` e `gap ≤ limiar`).
    pub feasible: bool,
}

/// Resumo energético de um atrator, preservado após a poda.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gist {
    /// Chave endereçável por conteúdo.
    pub key: u128,
    /// Escopo de origem.
    pub scope: ScopeId,
    /// Energia do estado que gerou o fato.
    pub energy: f32,
    /// Gap de energia.
    pub gap: f32,
    /// Autoridade social.
    pub authority: u8,
    /// Carimbo externo (`wall_ms`) preservado.
    pub wall_ms: u64,
    /// Assinatura de fase de baixa dimensão (média circular por bloco).
    pub signature: [f32; GIST_DIM],
}

impl Gist {
    /// Similaridade média `cos(Δφ)` entre duas assinaturas.
    pub fn similarity(&self, other: &[f32; GIST_DIM]) -> f32 {
        let mut acc = 0.0f32;
        for i in 0..GIST_DIM {
            acc += (self.signature[i] - other[i]).cos();
        }
        acc / GIST_DIM as f32
    }
}

/// Ledger de fusão: fatos autoritativos, *gists* e telemetria de escopo.
///
/// Aloca apenas no `T1`/`T2` (`stage`, `commit_run`, `prune`). O caminho de
/// consulta [`Self::cue_into_scoped`] não aloca e escreve no buffer do
/// chamador.
#[derive(Debug)]
pub struct FusionLedger {
    entries: Vec<DraftRecord>,
    gists: Vec<Gist>,
    staged: Vec<DraftRecord>,
    superseded: u64,
    pruned: u64,
    runs: u64,
    cross_scope_hits: u64,
    foreign_candidates: u64,
    provenance_abstentions: u64,
    gap_threshold: f32,
    authority_split: u8,
}

impl Default for FusionLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl FusionLedger {
    /// Ledger com os limiares do dossiê: `gap ≤ 0,25` factível; autoridade
    /// `≥ 128` confiável.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            gists: Vec::new(),
            staged: Vec::new(),
            superseded: 0,
            pruned: 0,
            runs: 0,
            cross_scope_hits: 0,
            foreign_candidates: 0,
            provenance_abstentions: 0,
            gap_threshold: 0.25,
            authority_split: 128,
        }
    }

    /// Ajusta os limiares da matriz 2D.
    pub fn with_thresholds(mut self, gap_threshold: f32, authority_split: u8) -> Self {
        if gap_threshold.is_finite() && gap_threshold >= 0.0 {
            self.gap_threshold = gap_threshold;
        }
        self.authority_split = authority_split;
        self
    }

    /// Fila do `T1`: recebe um registro drenado (fora do `T0`).
    pub fn stage(&mut self, record: DraftRecord) {
        self.staged.push(record);
    }

    /// Estagia um lote drenado.
    pub fn stage_batch(&mut self, records: impl IntoIterator<Item = DraftRecord>) {
        self.staged.extend(records);
    }

    /// Fecha o ciclo: consolida o lote, resolve contradições e devolve quantos
    /// registros entraram no estado autoritativo.
    ///
    /// Contradição = mesma `key`, mesmo `ScopeId`, payload diferente. O fato
    /// antigo vai para o *gist* antes de ser substituído; fatos idênticos apenas
    /// renovam carimbos.
    pub fn commit_run(&mut self) -> usize {
        self.runs = self.runs.wrapping_add(1);
        let mut committed = 0usize;
        let staged = std::mem::take(&mut self.staged);
        for record in staged {
            match self
                .entries
                .iter_mut()
                .find(|e| e.key == record.key && e.scope == record.scope && e.layer == record.layer)
            {
                Some(existing) => {
                    if existing.payload != record.payload {
                        self.gists.push(Self::gist_of(existing));
                        self.superseded = self.superseded.wrapping_add(1);
                        *existing = record;
                    } else {
                        existing.certificate = record.certificate;
                        existing.authority = record.authority;
                        existing.tick = record.tick;
                        existing.wall_ms = record.wall_ms;
                    }
                }
                None => {
                    self.entries.push(record);
                    committed += 1;
                }
            }
        }
        committed
    }

    /// Consolida e, antes de podar, grava o *gist* energético.
    ///
    /// Devolve o *gist* criado, se a chave existia.
    pub fn prune_to_gist(&mut self, scope: ScopeId, key: u128) -> Option<Gist> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.key == key && e.scope == scope)?;
        let gist = Self::gist_of(&self.entries[pos]);
        self.entries.remove(pos);
        self.gists.push(gist);
        self.pruned = self.pruned.wrapping_add(1);
        Some(gist)
    }

    /// Oráculo da matriz 2D: `autoridade × factibilidade`.
    pub fn decide(&mut self, authority: u8, certificate: Certificate) -> Decision {
        let gap = certificate.gap.max(0.0);
        let feasible = certificate.violations == 0 && gap <= self.gap_threshold;
        let trusted = authority >= self.authority_split;
        let (quadrant, action) = match (trusted, feasible) {
            (true, true) => (Quadrant::TrustedFeasible, FusionAction::ActAndCite),
            (true, false) => (Quadrant::TrustedAmbiguous, FusionAction::CiteNoAct),
            (false, true) => (Quadrant::UnvettedFeasible, FusionAction::Propose),
            (false, false) => (Quadrant::UnvettedAmbiguous, FusionAction::Abstain),
        };
        if action == FusionAction::Abstain && certificate.violations > 0 {
            self.provenance_abstentions = self.provenance_abstentions.wrapping_add(1);
        }
        Decision {
            action,
            quadrant,
            authority,
            gap: gap.min(1.0),
            feasible,
        }
    }

    /// Consulta **filtrada por escopo** — a correção de R8 no caminho F3.
    ///
    /// Compara a `query` (fase complexa intercalada, `re, im`) apenas com
    /// registros de `scope`; escreve o padrão mais próximo em `out` como fases
    /// (`atan2(im, re)`, `out.len() >= dims`) e devolve `(consultados, key)`.
    ///
    /// Não aloca. Uma consulta de um escopo nunca devolve chave de outro; o
    /// número de candidatos estrangeiros vistos é contabilizado apenas como
    /// telemetria (`foreign_candidates`), nunca como hit.
    pub fn cue_into_scoped(
        &mut self,
        scope: ScopeId,
        query: &[f32],
        out: &mut [f32],
    ) -> Result<CueHit, BridgeError> {
        if self.entries.is_empty() {
            return Err(BridgeError::EmptyScope);
        }
        let dims = query.len() / 2;
        if out.len() < dims || dims == 0 {
            return Err(BridgeError::BufTooSmall);
        }
        let mut best: Option<(f32, u128, u8)> = None;
        let mut consulted = 0usize;
        for record in self.entries.iter() {
            if record.scope != scope {
                self.foreign_candidates = self.foreign_candidates.wrapping_add(1);
                continue;
            }
            let record_dims = record.payload.len() / 8;
            if record_dims != dims {
                continue;
            }
            consulted += 1;
            let score = Self::complex_score(&record.payload, query, dims);
            let better = match &best {
                None => true,
                Some((s, _, _)) => score > *s,
            };
            if better {
                best = Some((score, record.key, record.authority));
            }
        }
        let Some((_, key, authority)) = best else {
            return Err(BridgeError::EmptyScope);
        };
        // Materializa a semente de fase no buffer do chamador.
        if let Some(record) = self
            .entries
            .iter()
            .find(|e| e.key == key && e.scope == scope)
        {
            for (i, item) in out.iter_mut().enumerate().take(dims) {
                let (re, im) = Self::phase_at(&record.payload, i);
                *item = im.atan2(re);
            }
        }
        Ok(CueHit {
            key,
            scope,
            authority,
            hits: consulted,
        })
    }

    /// Consulta pelo *gist* (após a poda do atrator completo).
    ///
    /// Devolve a `key` do gist mais próximo dentro do escopo, se a similaridade
    /// de assinatura superar `min_similarity`.
    pub fn gist_cue(&self, scope: ScopeId, query: &[f32], min_similarity: f32) -> Option<u128> {
        let signature = Self::signature_of(query, query.len() / 2);
        let mut best: Option<(f32, u128)> = None;
        for gist in self.gists.iter() {
            if gist.scope != scope {
                continue;
            }
            let sim = gist.similarity(&signature);
            if sim >= min_similarity && best.map_or(true, |(s, _)| sim > s) {
                best = Some((sim, gist.key));
            }
        }
        best.map(|(_, key)| key)
    }

    /// Fatos autoritativos.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` sem fatos autoritativos.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Gists preservados.
    pub fn gist_count(&self) -> usize {
        self.gists.len()
    }

    /// Últimos gists (ordem de criação).
    pub fn gists(&self) -> &[Gist] {
        &self.gists
    }

    /// Contradições resolvidas por `supersede`.
    pub fn superseded(&self) -> u64 {
        self.superseded
    }

    /// Atratores podados para *gist*.
    pub fn pruned(&self) -> u64 {
        self.pruned
    }

    /// Ciclos fechados por [`Self::commit_run`].
    pub fn runs(&self) -> u64 {
        self.runs
    }

    /// Hits cruzados devolvidos por `cue_into_scoped` (R8; deve ser `0`).
    pub fn cross_scope_hits(&self) -> u64 {
        self.cross_scope_hits
    }

    /// Candidatos de outro escopo vistos (telemetria, não hit).
    pub fn foreign_candidates(&self) -> u64 {
        self.foreign_candidates
    }

    /// Abstenções causadas por proveniência (R9).
    pub fn provenance_abstentions(&self) -> u64 {
        self.provenance_abstentions
    }

    /// `true` se a `key` pertence ao escopo (checagem de R8 no teste).
    pub fn key_in_scope(&self, scope: ScopeId, key: u128) -> bool {
        self.entries
            .iter()
            .any(|e| e.scope == scope && e.key == key)
    }

    // ------------------------------------------------------------------
    // Internos
    // ------------------------------------------------------------------

    fn gist_of(record: &DraftRecord) -> Gist {
        let dims = record.payload.len() / 8;
        Gist {
            key: record.key,
            scope: record.scope,
            energy: record.certificate.energy,
            gap: record.certificate.gap,
            authority: record.authority,
            wall_ms: record.wall_ms,
            signature: Self::signature_of_payload(&record.payload, dims),
        }
    }

    fn phase_at(payload: &[u8], i: usize) -> (f32, f32) {
        let off = i * 8;
        if off + 8 > payload.len() {
            return (1.0, 0.0);
        }
        let re = f32::from_le_bytes([
            payload[off],
            payload[off + 1],
            payload[off + 2],
            payload[off + 3],
        ]);
        let im = f32::from_le_bytes([
            payload[off + 4],
            payload[off + 5],
            payload[off + 6],
            payload[off + 7],
        ]);
        (re, im)
    }

    fn complex_score(payload: &[u8], query: &[f32], dims: usize) -> f32 {
        let mut re = 0.0f64;
        let mut na = 0.0f64;
        let mut nb = 0.0f64;
        for i in 0..dims {
            let (ar, ai) = Self::phase_at(payload, i);
            let br = query[2 * i];
            let bi = query[2 * i + 1];
            re += f64::from(ar) * f64::from(br) + f64::from(ai) * f64::from(bi);
            na += f64::from(ar * ar + ai * ai);
            nb += f64::from(br * br + bi * bi);
        }
        ((re / (na.sqrt() * nb.sqrt()).max(1e-12)) as f32).clamp(-1.0, 1.0)
    }

    /// Assinatura circular de 8 blocos a partir de fases complexas intercaladas.
    fn signature_of(query: &[f32], dims: usize) -> [f32; GIST_DIM] {
        let mut sig = [0.0f32; GIST_DIM];
        if dims == 0 {
            return sig;
        }
        let per = dims.div_ceil(GIST_DIM);
        for (b, item) in sig.iter_mut().enumerate().take(GIST_DIM) {
            let lo = b * per;
            let hi = ((b + 1) * per).min(dims);
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for i in lo..hi {
                re += query[2 * i];
                im += query[2 * i + 1];
            }
            *item = im.atan2(re);
        }
        sig
    }

    fn signature_of_payload(payload: &[u8], dims: usize) -> [f32; GIST_DIM] {
        let mut sig = [0.0f32; GIST_DIM];
        if dims == 0 {
            return sig;
        }
        let per = dims.div_ceil(GIST_DIM);
        for (b, item) in sig.iter_mut().enumerate().take(GIST_DIM) {
            let lo = b * per;
            let hi = ((b + 1) * per).min(dims);
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for i in lo..hi {
                let (r, m) = Self::phase_at(payload, i);
                re += r;
                im += m;
            }
            *item = im.atan2(re);
        }
        sig
    }
}

impl KnowledgeSource for FusionLedger {
    /// Contrato F1 (`CueHit`): o F2/F3 usa a mesma forma de resposta do dono da
    /// ponte, evitando duas assinaturas divergentes (MSG 0159).
    fn cue_into(
        &mut self,
        scope: ScopeId,
        query: &[f32],
        out: &mut [f32],
    ) -> Result<CueHit, BridgeError> {
        self.cue_into_scoped(scope, query, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::knowledge_bridge_f1::{Layer, ScopeId};

    fn record(key: u128, scope: ScopeId, authority: u8, gap: f32) -> DraftRecord {
        DraftRecord {
            key,
            layer: Layer::L4Semantic,
            certificate: Certificate {
                energy: -1.0,
                gap,
                violations: 0,
            },
            authority,
            scope,
            tick: 0,
            wall_ms: 1,
            payload: vec![0u8; 8],
        }
    }

    #[test]
    fn matriz_2d_cobre_os_quatro_quadrantes() {
        let mut ledger = FusionLedger::new();
        let cert_ok = Certificate {
            energy: 0.0,
            gap: 0.1,
            violations: 0,
        };
        let cert_bad = Certificate {
            energy: 0.0,
            gap: 0.8,
            violations: 0,
        };
        assert_eq!(ledger.decide(200, cert_ok).action, FusionAction::ActAndCite);
        assert_eq!(ledger.decide(200, cert_bad).action, FusionAction::CiteNoAct);
        assert_eq!(ledger.decide(10, cert_ok).action, FusionAction::Propose);
        assert_eq!(ledger.decide(10, cert_bad).action, FusionAction::Abstain);
    }

    #[test]
    fn supersede_preserva_gist_e_contradicao() {
        let mut ledger = FusionLedger::new();
        let scope = ScopeId::new(1, 1, 1);
        let mut a = record(7, scope, 200, 0.1);
        a.payload = vec![1, 0, 0, 0, 0, 0, 0, 0];
        ledger.stage(a);
        assert_eq!(ledger.commit_run(), 1);
        let mut b = record(7, scope, 200, 0.2);
        b.payload = vec![0, 0, 0, 0, 1, 0, 0, 0];
        ledger.stage(b);
        assert_eq!(ledger.commit_run(), 0);
        assert_eq!(ledger.superseded(), 1);
        assert_eq!(ledger.gist_count(), 1);
        assert_eq!(ledger.len(), 1);
    }
}
