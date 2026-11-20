//! `aether-decision` — a face única do vega dentro do AETHER (Eixo E do plano
//! F2→F3, `docs/VEGA_F3_PLANO_CONSOLIDADO.md`).
//!
//! O resto do AETHER nunca chama o Python diretamente: ele fala com a trait
//! [`DecisionPort`](port::DecisionPort). Trocar a implementação (stub, roteirizada,
//! PyO3 real, AND gate) não toca o chat.
//!
//! ## Camadas
//!
//! | módulo | papel |
//! |---|---|
//! | [`types`] | contrato estável: `Answer`, `DecisionRequest/Response`, `AbstainReason` |
//! | [`port`] | a trait + `StubPort`/`ScriptedPort`/`AndGatePort` |
//! | [`gate`] | o AND gate puro (dois leitores, acordo obrigatório) |
//! | [`watchdog`] | orçamento de VRAM — estouro vira `engine_unavailable` |
//! | [`bootstrap`] | máquina de estados do warmup (`load` + `fit`) |
//! | [`config`] | pinos de versão (interpreter, modelo, corpus, orçamento) |
//! | `pyo3_impl` | o binding real (feature `pyo3`) |
//!
//! ## Regra de ouro
//!
//! O port **nunca** inventa resposta: falha de engine, exceção Python ou
//! estouro de VRAM viram [`AbstainReason::EngineUnavailable`](types::AbstainReason),
//! e o chamador degrada com honestidade.

pub mod bootstrap;
pub mod config;
pub mod gate;
pub mod port;
pub mod types;
pub mod watchdog;

#[cfg(feature = "pyo3")]
pub mod pyo3_impl;

pub use config::EngineConfig;
pub use gate::combine;
pub use port::{AndGatePort, DecisionPort, ScriptedPort, StubPort};
pub use types::{
    AbstainReason, Answer, DecisionError, DecisionMeta, DecisionRequest, DecisionResponse,
};

/// Versão do contrato (Eixo B: schema estável). Muda quando o JSON muda.
pub const CONTRACT_VERSION: &str = "decision/1";
