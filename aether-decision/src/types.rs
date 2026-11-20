//! Tipos do contrato do AND gate — o cano único entre chat, decisor, pesquisa
//! externa e nsgdb (Eixos A/B do plano F2→F3).
//!
//! Regra: a abstenção é TIPADA e carrega o MOTIVO. `unknown` é a porta de
//! saída para o vetor de pesquisa externa; `ambiguous`/`conflict` escalam para
//! o HITL; `engine_unavailable` é degradação honesta (nunca inventa).

use serde::{Deserialize, Serialize};

/// Motivo tipado da abstenção. Todo chamador deve tratar os quatro casos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbstainReason {
    /// Fora do léxico / OOD. Único motivo que admite o vetor de pesquisa
    /// externa (F2): evidência volta CITADA em quarentena (autoridade 60–120).
    Unknown,
    /// Margem fina / pergunta ambígua. Escala para o HITL com os scores.
    Ambiguous,
    /// Readers em discordância no AND gate. Escala para o HITL.
    Conflict,
    /// Engine indisponível (watchdog VRAM, exceção Python, crash). O chat
    /// degrada — nunca age, nunca inventa.
    EngineUnavailable,
}

impl AbstainReason {
    /// Só `unknown` abre o vetor de pesquisa externa (decisão HITL, F2).
    pub fn admits_research(self) -> bool {
        matches!(self, AbstainReason::Unknown)
    }

    /// Escala para humano?
    pub fn escalates_hitl(self) -> bool {
        matches!(self, AbstainReason::Ambiguous | AbstainReason::Conflict)
    }
}

/// Um answer de uma pergunta do gate. Espelha a saída do `vegaml.decide`:
/// campos `None` são omitidos no JSON (contrato Eixo B: schema estável).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Answer {
    pub question: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probs: Option<serde_json::Value>,
    #[serde(default)]
    pub abstain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "motivo")]
    pub abstain_reason: Option<AbstainReason>,
    /// `engine` | `ttt` | `and_gate` — proveniência do readout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readout: Option<String>,
    /// Confiança do readout (probabilidade da melhor opção).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

impl Answer {
    pub fn abstained(question: impl Into<String>, reason: AbstainReason) -> Self {
        Self {
            question: question.into(),
            abstain: true,
            abstain_reason: Some(reason),
            ..Self::default()
        }
    }
}

/// Pedido ao port. `state` é digest de certificado ≤320 tokens (Eixo B) —
/// quem monta o request garante o orçamento; o port não re-corta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub state: String,
    /// Perguntas no schema tipado do vegaml (instruções + criteria).
    pub questions: serde_json::Value,
}

/// Resposta completa do gate, com proveniência (Eixo C: evidência auditável).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResponse {
    pub answers: Vec<Answer>,
    /// Conjunto conformal (Eixo B: schema `{answers, conformal_set, meta}`).
    /// Vem do `prediction_set` do vegaml; `None` enquanto o head `ttt` não
    /// produz (o head não carrega conjunto conformal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conformal_set: Option<serde_json::Value>,
    pub meta: DecisionMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionMeta {
    /// Latência medida do decide() inteiro (ms).
    pub latency_ms: f64,
    /// Identificador do gate (ex.: "vega-u-0.8b AND gemma-prose").
    pub engine: String,
    /// true quando o port respondeu por degradação (stub, fallback).
    pub degraded: bool,
    /// VRAM em uso no momento da decisão (MiB), quando o watchdog mediu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_used_mb: Option<f64>,
}

impl Default for DecisionMeta {
    fn default() -> Self {
        Self {
            latency_ms: 0.0,
            engine: "stub".into(),
            degraded: true,
            vram_used_mb: None,
        }
    }
}

/// Erro do port. Exceção Python / engine morto NUNCA sobe como panic —
/// vira `EngineUnavailable` e o chamador produz abstain tipado (Eixo E).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionError {
    EngineUnavailable(String),
    Contract(String),
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionError::EngineUnavailable(m) => write!(f, "engine indisponível: {m}"),
            DecisionError::Contract(m) => write!(f, "contrato violado: {m}"),
        }
    }
}

impl std::error::Error for DecisionError {}

impl DecisionError {
    /// Converte o erro em abstain tipado — a fronteira da trait (Eixo E).
    pub fn into_abstain(self, question: impl Into<String>) -> Answer {
        let reason = match &self {
            DecisionError::EngineUnavailable(_) => AbstainReason::EngineUnavailable,
            DecisionError::Contract(_) => AbstainReason::Ambiguous,
        };
        Answer::abstained(question, reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unknown_admits_research() {
        assert!(AbstainReason::Unknown.admits_research());
        assert!(!AbstainReason::Ambiguous.admits_research());
        assert!(!AbstainReason::Conflict.admits_research());
        assert!(!AbstainReason::EngineUnavailable.admits_research());
    }

    #[test]
    fn ambiguous_and_conflict_escalate_hitl() {
        assert!(AbstainReason::Ambiguous.escalates_hitl());
        assert!(AbstainReason::Conflict.escalates_hitl());
        assert!(!AbstainReason::Unknown.escalates_hitl());
        assert!(!AbstainReason::EngineUnavailable.escalates_hitl());
    }

    #[test]
    fn error_becomes_typed_abstain_never_panics() {
        let a = DecisionError::EngineUnavailable("VRAM > 5GB".into())
            .into_abstain("winner");
        assert!(a.abstain);
        assert_eq!(a.abstain_reason, Some(AbstainReason::EngineUnavailable));

        let b = DecisionError::Contract("schema".into()).into_abstain("route");
        assert_eq!(b.abstain_reason, Some(AbstainReason::Ambiguous));
    }

    #[test]
    fn answer_json_omits_none_fields() {
        let a = Answer::abstained("passou", AbstainReason::Unknown);
        let j = serde_json::to_string(&a).unwrap();
        assert!(!j.contains("choice"));
        assert!(!j.contains("probs"));
        assert!(j.contains("\"abstain_reason\":\"unknown\""));
    }
}
