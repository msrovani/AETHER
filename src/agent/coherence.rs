//! Coerência: junta os especialistas, detecta contradição e decide.
//!
//! A regra é a mesma do Vol. III §6: **autoridade** (social) × **factibilidade**
//! (física/confiança). O agente não escolhe "a resposta mais bonita"; ele
//! escolhe a resposta que sobrevive a três testes:
//!
//! 1. os especialistas que a sustentam concordam no valor;
//! 2. a autoridade das fontes permite agir;
//! 3. o suporte é alto o bastante para não ser ruído.
//!
//! Contradição **não resolvida** ⇒ [`Action::Abstain`]. Contradição com uma
//! fonte claramente dominante ⇒ [`Action::Cite`] (cita, não age).

use crate::core::certificate::{Certificate as EngineCertificate, Decision};

use super::{Action, Claim, ExpertId, ExpertOutput};

/// Contradição entre especialistas para o mesmo `(sujeito, relação)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Contradiction {
    /// Sujeito.
    pub subject: String,
    /// Relação.
    pub relation: String,
    /// Valores em conflito (um por fonte).
    pub values: Vec<String>,
    /// Fontes.
    pub sources: Vec<ExpertId>,
}

/// Resultado da fusão.
#[derive(Clone, Debug)]
pub struct CoherenceResult {
    /// Claims vencedoras (uma por grupo, na ordem de suporte).
    pub claims: Vec<Claim>,
    /// Contradições sem vencedor.
    pub contradictions: usize,
    /// Detalhe das contradições.
    pub contradiction_details: Vec<Contradiction>,
    /// Suporte da resposta principal (`0..=1`).
    pub support: f32,
    /// Fração de fontes que concordam com a vencedora (`0..=1`).
    pub consensus: f32,
    /// Autoridade máxima entre as fontes da vencedora.
    pub authority: u8,
    /// Ação prescrita.
    pub action: Action,
    /// Resposta principal.
    pub answer: Option<Claim>,
    /// Justificativa curta (para log/telemetria).
    pub rationale: String,
}

impl Default for CoherenceResult {
    fn default() -> Self {
        Self {
            claims: Vec::new(),
            contradictions: 0,
            contradiction_details: Vec::new(),
            support: 0.0,
            consensus: 0.0,
            authority: 0,
            action: Action::Abstain,
            answer: None,
            rationale: "sem evidência".to_string(),
        }
    }
}

/// Motor de coerência (matriz 2D).
#[derive(Clone, Debug)]
pub struct CoherenceEngine {
    /// Piso de confiança para considerar a resposta inequívoca.
    pub gap_min: f64,
    /// Autoridade a partir da qual a fonte é "confiável" (HITL/consenso).
    pub authority_split: u8,
}

impl Default for CoherenceEngine {
    fn default() -> Self {
        Self {
            gap_min: 0.50,
            authority_split: 128,
        }
    }
}

impl CoherenceEngine {
    /// Ajusta os limiares.
    pub fn with_thresholds(mut self, gap_min: f64, authority_split: u8) -> Self {
        if gap_min.is_finite() && gap_min >= 0.0 {
            self.gap_min = gap_min;
        }
        self.authority_split = authority_split;
        self
    }

    /// Funde as saídas dos especialistas.
    pub fn merge(&self, outputs: &[ExpertOutput]) -> CoherenceResult {
        let claims: Vec<Claim> = outputs
            .iter()
            .flat_map(|o| o.claims.iter().cloned())
            .collect();
        if claims.is_empty() {
            return CoherenceResult {
                rationale: "nenhum especialista produziu claim".to_string(),
                ..CoherenceResult::default()
            };
        }

        // Agrupa por (sujeito, relação); memória textual e detalhes não
        // participam do confronto de valores.
        let mut groups: Vec<((String, String), Vec<Claim>)> = Vec::new();
        for claim in claims {
            if claim.subject.trim().is_empty() || claim.relation.trim().is_empty() {
                continue;
            }
            let key = claim.group();
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, list)) => list.push(claim),
                None => groups.push((key, vec![claim])),
            }
        }

        let mut winners: Vec<Claim> = Vec::new();
        let mut details: Vec<Contradiction> = Vec::new();
        let mut total_sources = 0usize;
        let mut agreeing_sources = 0usize;
        let mut contested = false;

        for (_, list) in groups {
            total_sources += list.len();
            // Agrupa valores distintos, mantendo a melhor claim de cada.
            let mut values: Vec<Claim> = Vec::new();
            for claim in &list {
                match values.iter_mut().find(|c| c.same_value(claim)) {
                    Some(best) => {
                        if claim.confidence > best.confidence
                            || (claim.confidence == best.confidence
                                && claim.authority > best.authority)
                        {
                            *best = claim.clone();
                        }
                    }
                    None => values.push(claim.clone()),
                }
            }
            if values.len() == 1 {
                agreeing_sources += list.len();
                winners.push(values.remove(0));
                continue;
            }
            // Conflito: a fonte com autoridade dominante vence, mas a ação
            // vira `Cite` (não se age com evidência contestada).
            contested = true;
            values.sort_by(|a, b| b.authority.cmp(&a.authority));
            let top = values[0].authority;
            let second = values.get(1).map_or(0, |c| c.authority);
            if top >= second.saturating_add(64) {
                let winner = values.remove(0);
                agreeing_sources += list
                    .iter()
                    .filter(|claim| claim.same_value(&winner))
                    .count();
                winners.push(winner);
            } else {
                details.push(Contradiction {
                    subject: values[0].subject.clone(),
                    relation: values[0].relation.clone(),
                    values: values.iter().map(|c| c.value.clone()).collect(),
                    sources: values.iter().map(|c| c.source).collect(),
                });
            }
        }

        if winners.is_empty() {
            let support = outputs
                .iter()
                .map(|o| o.confidence)
                .fold(0.0f32, f32::max);
            return CoherenceResult {
                contradictions: details.len(),
                contradiction_details: details,
                support,
                action: Action::Abstain,
                rationale: "todos os grupos entraram em contradição".to_string(),
                ..CoherenceResult::default()
            };
        }

        winners.sort_by(|a, b| {
            b.confidence
                .total_cmp(&a.confidence)
                .then_with(|| b.authority.cmp(&a.authority))
        });
        let winner = winners[0].clone();
        let support = winner.confidence;
        let consensus = if total_sources == 0 {
            0.0
        } else {
            (agreeing_sources as f32 / total_sources as f32).min(1.0)
        };
        let authority = winner.authority;
        let certificate = EngineCertificate::new(
            0.0,
            f64::from(support),
            details.len() as u32,
            f64::from(consensus),
            winner.provenance,
        );
        let decision = if contested {
            Decision::Cite
        } else {
            certificate.decide(authority, self.gap_min)
        };
        let action = match decision {
            Decision::Act => Action::Act,
            Decision::Cite => Action::Cite,
            Decision::Propose => Action::Propose,
            Decision::Abstain => Action::Abstain,
        };
        let rationale = match action {
            Action::Act => "autoridade e suporte suficientes".to_string(),
            Action::Cite => {
                if contested {
                    "evidência contestada: cita, não age".to_string()
                } else {
                    "autoridade alta, suporte abaixo do piso: cita, não age".to_string()
                }
            }
            Action::Propose => "coerente, mas sem fonte forte: propõe".to_string(),
            Action::Abstain => "suporte insuficiente".to_string(),
        };
        CoherenceResult {
            claims: winners,
            contradictions: details.len(),
            contradiction_details: details,
            support,
            consensus,
            authority,
            action,
            answer: if action == Action::Abstain {
                None
            } else {
                Some(winner)
            },
            rationale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::certificate::Provenance;

    fn output(value: &str, confidence: f32, authority: u8, source: ExpertId) -> ExpertOutput {
        ExpertOutput {
            expert: source,
            claims: vec![Claim::new(
                "Brasil",
                "capital",
                value,
                confidence,
                authority,
                source,
                Provenance::Transplanted,
            )],
            confidence,
            certificate: None,
            detail: String::new(),
            cost_us: 0,
        }
    }

    #[test]
    fn dois_especialistas_concordando_agem() {
        let engine = CoherenceEngine::default();
        let result = engine.merge(&[
            output("Brasília", 0.95, 255, ExpertId::Transplant),
            output("Brasília", 0.9, 200, ExpertId::Logic),
        ]);
        assert_eq!(result.action, Action::Act);
        assert_eq!(result.contradictions, 0);
        assert_eq!(result.answer.unwrap().value, "Brasília");
        assert!(result.consensus > 0.9);
    }

    #[test]
    fn autoridade_dominante_cita_nao_age() {
        let engine = CoherenceEngine::default();
        let result = engine.merge(&[
            output("Brasília", 0.9, 255, ExpertId::Transplant),
            output("Rio", 0.9, 100, ExpertId::Logic),
        ]);
        assert_eq!(result.action, Action::Cite);
        assert_eq!(result.answer.unwrap().value, "Brasília");
    }

    #[test]
    fn empate_de_autoridade_abstem() {
        let engine = CoherenceEngine::default();
        let result = engine.merge(&[
            output("Brasília", 0.9, 200, ExpertId::Transplant),
            output("Rio", 0.9, 200, ExpertId::Logic),
        ]);
        assert_eq!(result.action, Action::Abstain);
        assert!(result.answer.is_none());
        assert_eq!(result.contradictions, 1);
    }

    #[test]
    fn sem_claims_abstem() {
        let engine = CoherenceEngine::default();
        let result = engine.merge(&[ExpertOutput::empty(ExpertId::Phase, "vazio")]);
        assert_eq!(result.action, Action::Abstain);
    }
}
