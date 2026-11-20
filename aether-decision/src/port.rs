//! A trait `DecisionPort` — único ponto de contato do restante do AETHER com
//! o decisor (Eixo E: isolamento). Implementações:
//! - `StubPort`: sempre abste com `engine_unavailable` (chat sem engine degrada
//!   honestamente — é o default até o PyO3 estar no ar);
//! - `ScriptedPort`: respostas roteirizadas para testes;
//! - `AndGatePort`: a chave dupla (primário + sentinela) — o gate do fórum
//!   MSG 0756/0758;
//! - `pyo3_impl::VegaPort` (feature `pyo3`): embed real do vegaml.

use crate::types::{Answer, DecisionError, DecisionRequest, DecisionResponse};

pub trait DecisionPort: Send + Sync {
    /// Uma decisão. Implementações NUNCA panicam por falha de engine:
    /// tudo vira `Result` — a conversão para abstain tipado é do chamador
    /// (ou direto em `and_gate`, que já o faz).
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, DecisionError>;
}

/// Port de degradação: usado quando o engine não está no ar. Sempre abste
/// com `engine_unavailable` — honestidade, não simulação de decisão.
#[derive(Debug, Default)]
pub struct StubPort;

impl DecisionPort for StubPort {
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, DecisionError> {
        let answers: Vec<Answer> = serde_json::Value::as_object(&req.questions)
            .map(|qs| {
                qs.keys()
                    .map(|q| Answer::abstained(q, crate::types::AbstainReason::EngineUnavailable))
                    .collect()
            })
            .unwrap_or_default();
        Ok(DecisionResponse {
            answers,
            conformal_set: None,
            meta: crate::types::DecisionMeta {
                latency_ms: 0.0,
                engine: "stub".into(),
                degraded: true,
                vram_used_mb: None,
            },
        })
    }
}

/// Port roteirizado: enfileira respostas; útil para testes do chat e da
/// integração das cadeiras sem carregar o engine.
#[derive(Debug, Default)]
pub struct ScriptedPort {
    queue: parking_lot_mutex::Mutex<Vec<Result<DecisionResponse, DecisionError>>>,
}

mod parking_lot_mutex {
    /// Mutex mínimo sem dependência externa (std é suficiente neste escopo).
    #[derive(Debug, Default)]
    pub struct Mutex<T>(std::sync::Mutex<T>);
    impl<T> Mutex<T> {
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            self.0.lock().expect("scripted port poisoned")
        }
    }
}

impl ScriptedPort {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, resp: Result<DecisionResponse, DecisionError>) {
        self.queue.lock().push(resp);
    }
}

impl DecisionPort for ScriptedPort {
    fn decide(&self, _req: &DecisionRequest) -> Result<DecisionResponse, DecisionError> {
        match self.queue.lock().pop() {
            Some(resp) => resp,
            None => Err(DecisionError::EngineUnavailable("fila scripted vazia".into())),
        }
    }
}

/// A chave dupla: dois leitores, um veredito.
///
/// O primário decide; a sentinela (especialista em OOD) veta. A regra
/// completa está em [`crate::gate::combine`] — aqui só orquestra e mede.
pub struct AndGatePort {
    primary: Box<dyn DecisionPort>,
    sentinel: Box<dyn DecisionPort>,
}

impl std::fmt::Debug for AndGatePort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndGatePort").finish()
    }
}

impl AndGatePort {
    pub fn new(primary: Box<dyn DecisionPort>, sentinel: Box<dyn DecisionPort>) -> Self {
        Self { primary, sentinel }
    }
}

impl DecisionPort for AndGatePort {
    fn decide(&self, req: &DecisionRequest) -> Result<DecisionResponse, DecisionError> {
        let p = self.primary.decide(req)?;
        let s = self.sentinel.decide(req)?;

        // Índice por pergunta: os dois ports respondem às MESMAS perguntas.
        let mut answers: Vec<Answer> = Vec::new();
        for pa in &p.answers {
            let sa = s.answers.iter().find(|a| a.question == pa.question);
            match sa {
                // A sentinela respondeu sobre a mesma pergunta: o gate decide.
                Some(sa) => answers.push(crate::gate::combine(pa, sa)),
                // A sentinela não cobriu a pergunta: não há veto, o primário vale.
                None => answers.push(pa.clone()),
            }
        }
        // Perguntas que só a sentinela respondeu (o primário não cobriu).
        for sa in &s.answers {
            if !answers.iter().any(|a| a.question == sa.question) {
                answers.push(sa.clone());
            }
        }

        Ok(DecisionResponse {
            answers,
            conformal_set: p.conformal_set.clone().or(s.conformal_set.clone()),
            meta: crate::types::DecisionMeta {
                latency_ms: p.meta.latency_ms + s.meta.latency_ms,
                engine: "and_gate".into(),
                degraded: p.meta.degraded || s.meta.degraded,
                vram_used_mb: p.meta.vram_used_mb.or(s.meta.vram_used_mb),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AbstainReason, DecisionMeta};

    fn req(questions: &[&str]) -> DecisionRequest {
        DecisionRequest {
            state: "estado de teste".into(),
            questions: serde_json::Value::Object(
                questions
                    .iter()
                    .map(|q| (q.to_string(), serde_json::json!({"type": "boolean"})))
                    .collect(),
            ),
        }
    }

    fn resp(question: &str, choice: &str) -> DecisionResponse {
        DecisionResponse {
            answers: vec![Answer {
                question: question.into(),
                choice: Some(choice.into()),
                probs: Some(serde_json::json!({choice: 0.9})),
                abstain: false,
                abstain_reason: None,
                readout: Some("engine".into()),
                confidence: Some(0.9),
                conformal_set: None,
                cv_accuracy: None,
            }],
            conformal_set: None,
            meta: DecisionMeta {
                latency_ms: 1.0,
                engine: "engine".into(),
                degraded: false,
                vram_used_mb: None,
            },
        }
    }

    #[test]
    fn stub_always_abstains_engine_unavailable() {
        let p = StubPort;
        let r = p.decide(&req(&["winner", "route"])).unwrap();
        assert!(r.meta.degraded);
        assert_eq!(r.answers.len(), 2);
        for a in r.answers {
            assert!(a.abstain);
            assert_eq!(a.abstain_reason, Some(AbstainReason::EngineUnavailable));
        }
    }

    #[test]
    fn scripted_returns_queue_then_fails_honest() {
        let p = ScriptedPort::new();
        p.push(Ok(resp("route", "responder")));
        assert!(p.decide(&req(&["route"])).is_ok());
        let err = p.decide(&req(&["route"])).unwrap_err();
        assert!(matches!(err, DecisionError::EngineUnavailable(_)));
    }

    #[test]
    fn and_gate_agrees_when_both_agree() {
        let primary = ScriptedPort::new();
        let sentinel = ScriptedPort::new();
        primary.push(Ok(resp("winner", "A")));
        sentinel.push(Ok(resp("winner", "A")));
        let gate = AndGatePort::new(Box::new(primary), Box::new(sentinel));
        let r = gate.decide(&req(&["winner"])).unwrap();
        assert_eq!(r.meta.engine, "and_gate");
        assert!(!r.answers[0].abstain);
        assert_eq!(r.answers[0].choice.as_deref(), Some("A"));
    }

    #[test]
    fn and_gate_escalates_on_disagreement() {
        let primary = ScriptedPort::new();
        let sentinel = ScriptedPort::new();
        primary.push(Ok(resp("winner", "A")));
        sentinel.push(Ok(resp("winner", "B")));
        let gate = AndGatePort::new(Box::new(primary), Box::new(sentinel));
        let r = gate.decide(&req(&["winner"])).unwrap();
        assert!(r.answers[0].abstain);
        assert_eq!(r.answers[0].abstain_reason, Some(AbstainReason::Conflict));
    }

    #[test]
    fn and_gate_propagates_engine_failure_as_error() {
        let primary = ScriptedPort::new();
        let sentinel = ScriptedPort::new();
        primary.push(Err(DecisionError::EngineUnavailable("VRAM".into())));
        sentinel.push(Ok(resp("winner", "A")));
        let gate = AndGatePort::new(Box::new(primary), Box::new(sentinel));
        let err = gate.decide(&req(&["winner"])).unwrap_err();
        let a = err.into_abstain("winner");
        assert_eq!(a.abstain_reason, Some(AbstainReason::EngineUnavailable));
    }
}
