//! T-GRAPH (A6) — evento estruturado → átomo tipado (BAM) → RG 64→16→4 →
//! assinatura/surpresa; claims → SGDB L6.
//!
//! # Contrato (docs/TOMADAS_FUTURO.md §A6; docs/CORE_TOMADAS.md §2.1 T-GRAPH)
//!
//! ```text
//! evento estruturado (IDs, timestamp, features) ──► átomo (64 fases BAM, u16)
//!                                                     │
//!                                                     ▼ RG 64→16→4
//!                                           assinatura macro [u16; 4] + surpresa
//!                                                     │
//!                                                     ▼
//!                                  T-MOE / T-PRED → T-COH → Action; claims → SGDB L6
//! ```
//!
//! **Disjuntores (ordem):**
//! * **schema break → reject** (`validate` → `Err`; nada é silenciosamente aceito);
//! * **auth ≤ 120** ([`GRAPH_AUTH`]); nunca 255 automático;
//! * **feature `graph` off** ([`graph_enabled`]); tomada off → no-op + log;
//! * **texto livre não atravessa** — só evento estruturado tipado (`GraphEvent`).
//!
//! Core só engole `Goal`/`Claim`/`Concept`/`Artifact`/`ToolOutcome`; bytes/texto
//! livre **param** na tomada. Esta tomada **não** alimenta `act` sozinha.

use crate::core::bam_phase::BamPhase;
use crate::core::certificate::Provenance;
use crate::core::renormalization::RenormalizationHierarchy;
use crate::core::silicon_entropy::splitmix64;
use crate::json::Json;

use super::tool::ToolError;
use super::{Claim, ExpertId};

/// Autoridade da tomada T-GRAPH: teto automático = **120** (nunca 255).
pub const GRAPH_AUTH: u8 = 120;
/// Canais micro do RG: **64 → 16 → 4**.
pub const GRAPH_MICRO: usize = 64;
/// Seed padrão para derivação do átomo (determinístico).
pub const GRAPH_SEED: u64 = 0xC0FF_EE00_A6A6_0001;
/// Artefato do gate A6 (evidência promovível para `data/` com `.meta.json`).
pub const GRAPH_GATE_PATH: &str = "target/aether_graph_gate.json";

/// Feature off por default: `feature = "graph"` (cargo) habilita a tomada.
#[must_use]
pub fn graph_enabled() -> bool {
    cfg!(feature = "graph")
}

/// Evento estruturado — **não** aceita texto livre arbitrário.
///
/// * `ids`: identificadores (nó, aresta, entidade); não-vazio, sem string vazia;
/// * `timestamp_ms`: ms desde epoch;
/// * `features`: pares `(nome, valor f64)`; valores devem ser finitos.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphEvent {
    /// IDs tipados do evento.
    pub ids: Vec<String>,
    /// Timestamp (ms).
    pub timestamp_ms: u64,
    /// Features numéricas nomeadas.
    pub features: Vec<(String, f64)>,
}

impl GraphEvent {
    /// Disjuntor **schema break → reject**. Sem regex no core; schema fixo.
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.ids.is_empty() {
            return Err(ToolError::InvalidArgs("schema break: ids vazio".into()));
        }
        if self.ids.iter().any(String::is_empty) {
            return Err(ToolError::InvalidArgs("schema break: id vazio".into()));
        }
        if self.features.is_empty() {
            return Err(ToolError::InvalidArgs(
                "schema break: features vazio".into(),
            ));
        }
        if self.features.iter().any(|(k, _)| k.is_empty()) {
            return Err(ToolError::InvalidArgs(
                "schema break: feature sem nome".into(),
            ));
        }
        if self.features.iter().any(|(_, v)| !v.is_finite()) {
            return Err(ToolError::InvalidArgs(
                "schema break: feature não-finita".into(),
            ));
        }
        if self.ids.iter().any(|id| !is_ident(id)) {
            return Err(ToolError::InvalidArgs(
                "schema break: id não é identificador (texto livre para na tomada)".into(),
            ));
        }
        if self.features.iter().any(|(k, _)| !is_ident(k)) {
            return Err(ToolError::InvalidArgs(
                "schema break: feature não é identificador (texto livre para na tomada)".into(),
            ));
        }
        Ok(())
    }

    /// Átomo tipado: [`GRAPH_MICRO`] fases BAM (`u16`) por `splitmix64` sobre
    /// ids + timestamp + features + seed. **Determinístico**; sem alocação no
    /// hot path (saída em array de 64).
    #[must_use]
    pub fn to_atom(&self, seed: u64) -> [BamPhase; GRAPH_MICRO] {
        let mut st = seed ^ GRAPH_SEED;
        mix_str(&mut st, "t-graph");
        for id in &self.ids {
            mix_str(&mut st, id);
        }
        mix_u64(&mut st, self.timestamp_ms);
        for (k, v) in &self.features {
            mix_str(&mut st, k);
            mix_u64(&mut st, v.to_bits());
        }
        let mut atom = [BamPhase::ZERO; GRAPH_MICRO];
        for p in atom.iter_mut() {
            *p = BamPhase((splitmix64(&mut st) & 0xFFFF) as u16);
        }
        atom
    }

    /// Assinatura macro `[u16; 4]` — RG **64→16→4** + `macro_signature`
    /// (Sprint A, R6, F23; Sprint B L6). Determinística.
    #[must_use]
    pub fn signature(&self, seed: u64) -> [u16; 4] {
        signature_from_atom(&self.to_atom(seed))
    }

    /// Evidência do gate: contrato medido do evento (RG, assinatura, surpresa,
    /// claims e teto de auth). Determinística para `(evento, seed)`.
    ///
    /// `artifact_from` diz qual lado do disjuntor escreveu o artefato, porque o
    /// gate roda duas vezes (`cfg!` só vê uma feature por binário).
    ///
    /// As claims vêm de `claims_from_event` (não do caminho `_inner`), então com
    /// a tomada off a evidência mostra `claims: 0` — é o disjuntor, medido.
    ///
    /// # Panics
    ///
    /// Se o evento não passar em [`GraphEvent::validate`].
    #[must_use]
    pub fn gate_evidence(&self, seed: u64) -> Json {
        let atom = self.to_atom(seed);
        let sig = signature_from_atom(&atom);
        let (micro, meso, macro_) = rg_levels();
        let claims = self
            .claims_from_event(seed)
            .expect("gate_evidence: evento precisa passar em validate()");
        let max_authority = claims.iter().map(|c| c.authority).max().unwrap_or(0);
        Json::obj([
            ("tomada", Json::str("T-GRAPH")),
            ("feature", Json::Bool(graph_enabled())),
            (
                "artifact_from",
                Json::str(if graph_enabled() { "on" } else { "off" }),
            ),
            ("auth_teto", Json::Num(f64::from(GRAPH_AUTH))),
            (
                "rg",
                Json::obj([
                    ("micro", Json::Num(micro as f64)),
                    ("meso", Json::Num(meso as f64)),
                    ("macro", Json::Num(macro_ as f64)),
                ]),
            ),
            ("atom_len", Json::Num(atom.len() as f64)),
            (
                "signature",
                Json::str(format!(
                    "{:x},{:x},{:x},{:x}",
                    sig[0], sig[1], sig[2], sig[3]
                )),
            ),
            ("surprise_self", Json::Num(f64::from(surprise(&sig, &sig)))),
            ("claims", Json::Num(claims.len() as f64)),
            ("max_authority", Json::Num(f64::from(max_authority))),
            ("ok", Json::Bool(true)),
        ])
    }

    /// Claims tipadas → SGDB L6, **auth ≤ 120**, `Provenance::Observed`.
    ///
    /// * schema break → `Err` (0 claims inventadas);
    /// * **tomada off** (`!graph_enabled()`) → `Ok(vec![])` no-op.
    pub fn claims_from_event(&self, seed: u64) -> Result<Vec<Claim>, ToolError> {
        self.validate()?;
        if !graph_enabled() {
            // tomada off → no-op + log (Eixo C).
            eprintln!("t-graph: tomada off — evento ignorado (feature `graph` off)");
            return Ok(Vec::new());
        }
        Ok(self.claims_from_event_inner(seed))
    }

    /// Como `claims_from_event`, mas **sem** verificar `graph_enabled()`.
    ///
    /// Disponível para fiar/emitir claims explicitamente (inclusive do gate, que
    /// roda com a feature off).
    ///
    /// # Panics
    ///
    /// Se `self.ids` estiver vazio ou o evento não passar em [`GraphEvent::validate`].
    /// **Não** valida sozinho: chame `validate()` antes — este é o caminho não
    /// checado, e o disjuntor de schema break mora em `validate`.
    #[must_use]
    pub fn claims_from_event_inner(&self, seed: u64) -> Vec<Claim> {
        let subject = self.ids[0].clone();
        let sig = self.signature(seed);
        let sig_str = format!("{:x},{:x},{:x},{:x}", sig[0], sig[1], sig[2], sig[3]);
        let mut claims = Vec::with_capacity(self.features.len() + 1);
        claims.push(Claim {
            subject: subject.clone(),
            relation: "graph.signature".to_string(),
            value: sig_str,
            confidence: 1.0,
            authority: GRAPH_AUTH,
            source: ExpertId::Memory,
            provenance: Provenance::Observed,
        });
        for (k, v) in &self.features {
            claims.push(Claim {
                subject: subject.clone(),
                relation: k.clone(),
                value: format!("{v}"),
                confidence: 1.0,
                authority: GRAPH_AUTH,
                source: ExpertId::Memory,
                provenance: Provenance::Observed,
            });
        }
        claims
    }
}

/// Porta **T-GRAPH** — produtor de eventos estruturados tipados → claims.
///
/// Core só engole `Goal`/`Claim`/`Concept`/`Artifact`/`ToolOutcome`; bytes ou
/// texto livre **param** na tomada. Disjuntor: schema break → `Err`.
pub trait GraphPort {
    /// Valida o evento e emite claims (`auth ≤ 120`); tomada off → `Ok(vec![])`.
    fn to_claims(&self, event: &GraphEvent, seed: u64) -> Result<Vec<Claim>, ToolError>;
}

/// Implementação canônica sobre [`GraphEvent`].
#[derive(Clone, Copy, Debug, Default)]
pub struct GraphEventPort;

impl GraphPort for GraphEventPort {
    fn to_claims(&self, event: &GraphEvent, seed: u64) -> Result<Vec<Claim>, ToolError> {
        event.claims_from_event(seed)
    }
}

/// Assinatura macro a partir de um átomo BAM: RG 64→16→4 + `macro_signature`.
#[must_use]
pub fn signature_from_atom(atom: &[BamPhase; GRAPH_MICRO]) -> [u16; 4] {
    let phases = atom_phases(atom);
    let mut rg =
        RenormalizationHierarchy::new(GRAPH_MICRO).expect("RG: GRAPH_MICRO=64 é múltiplo de 16");
    rg.forward(&phases).expect("RG: 64 fases finitas");
    rg.macro_signature()
}

/// Tamanhos `(micro, meso, macro)` do RG: **(64, 16, 4)**.
#[must_use]
pub fn rg_levels() -> (usize, usize, usize) {
    let rg = RenormalizationHierarchy::new(GRAPH_MICRO).expect("RG: 64");
    rg.level_sizes()
}

/// Converte átomo BAM (`u16`) em fases `f32 ∈ [0, 2π)`.
#[must_use]
pub fn atom_phases(atom: &[BamPhase; GRAPH_MICRO]) -> [f32; GRAPH_MICRO] {
    let tau = std::f32::consts::TAU;
    let mut out = [0.0f32; GRAPH_MICRO];
    for (o, p) in out.iter_mut().zip(atom) {
        *o = (p.0 as f32 / 65536.0) * tau;
    }
    out
}

/// **Surpresa**: distância circular `u16` média entre assinaturas
/// anterior e atual, normalizada `∈ [0, 1]`. 0 = idêntica.
#[must_use]
pub fn surprise(prev: &[u16; 4], next: &[u16; 4]) -> f32 {
    let mut acc = 0.0f32;
    for (p, n) in prev.iter().zip(next) {
        acc += p.wrapping_sub(*n).min(n.wrapping_sub(*p)) as f32;
    }
    (acc / (4.0 * 32768.0)).clamp(0.0, 1.0)
}

/// Identificador lexical aceito: `[A-Za-z0-9._:/-]`, 1..=128 bytes.
///
/// É o disjuntor de "texto livre não atravessa": frase, espaço, quebra de linha
/// ou blob não são IDs.
#[must_use]
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'-'))
}

fn mix_str(st: &mut u64, s: &str) {
    for b in s.bytes() {
        *st ^= u64::from(b);
        *st = st.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

fn mix_u64(st: &mut u64, x: u64) {
    *st ^= x;
    *st = st.wrapping_mul(0x9E37_79B9_7F4A_7C15);
}
