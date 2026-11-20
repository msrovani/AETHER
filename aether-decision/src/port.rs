//! A trait `DecisionPort` — único ponto de contato do restante do AETHER com
//! o decisor (Eixo E: isolamento). Implementações:
//! - `StubPort`: sempre abste com `engine_unavailable` (chat sem engine degrada
//!   honestamente — é o default até o PyO3 estar no ar);
//! - `ScriptedPort`: respostas roteirizadas para testes;
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
            meta: crate::types::DecisionMeta {
                latency_ms: 0.0,
                engine: "stub".into(),
                degraded: true,
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
        pub fn new(v: T) -> Self {
            Self(std::sync::Mutex::new(v))
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AbstainReason, Answer, DecisionMeta};

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
        p.push(Ok(DecisionResponse {
            answers: vec![Answer::abstained("route", AbstainReason::Unknown)],
            meta: DecisionMeta {
                latency_ms: 1.0,
                engine: "scripted".into(),
                degraded: true,
            },
        }));
        assert!(p.decide(&req(&["route"])).is_ok());
        let err = p.decide(&req(&["route"])).unwrap_err();
        assert!(matches!(err, DecisionError::EngineUnavailable(_)));
    }
}
