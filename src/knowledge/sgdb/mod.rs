//! `neural-sgdb` como camada durável do AETHER (Vol. III §3–§8).
//!
//! O SGDB não entra no caminho quente e não decide nada: ele guarda **fatos
//! tipados com proveniência** e devolve **evidência** para o host aplicar a
//! matriz 2D (`autoridade × factibilidade`). Este módulo faz as três conexões
//! que faltavam:
//!
//! ```text
//! 1. T0 → T1   SpscKnowledgePhaseBridge (já existia)         [0 alloc]
//! 2. T1 → T2   SgdbLedger: commit_run, outbox, replay, audit  [durável]
//! 3. T2 ↔ T0   recall → payload de fase → cue_into (semente)  [reacende]
//! ```
//!
//! # Mapa de conceitos
//!
//! | AETHER | SGDB | Onde |
//! |---|---|---|
//! | `ScopeId { organism, run, app }` | `scope_app` + `scope_run` + entidade | [`wire::scope_entity`] |
//! | `Layer::L2/L4/L6` | camada lógica no documento + entidade | [`wire::PhaseDoc`] |
//! | `Certificate { E, ΔE, viol }` | `cert` no documento (pedido #3) | [`wire::PhaseDoc`] |
//! | `authority 0..=255` | `authority` do hit + ordenação | [`SgdbLedger::decide`] |
//! | payload de fase (`f32` complexo) | `type=json` + `format=aether-phase/v1` | [`wire::PhaseDoc::to_text`] |
//! | `key u128` | hex no documento + entidade | [`wire::content_key`] |
//! | TTL/decay (`wall_ms`) | `set_ttl` + `created_tick` | [`SgdbClient::set_ttl`] |
//! | `audit_checkpoint` | `curate(op=audit_checkpoint)` | [`SgdbClient::audit_checkpoint`] |
//!
//! # O que este módulo **não** faz
//!
//! * Não escolhe o que entra no prompt (doutrina: "the core does not decide").
//! * Não inventa `payload_type=phase` no banco: usa `json` + `format`, que é o
//!   contrato atual; o pedido #1 do Vol. III §8 continua aberto do lado do SGDB.
//! * Não promete chave endereçável por conteúdo no banco: a chave `u128` viaja
//!   no documento e nas entidades (pedido #4).
//!
//! # Modos
//!
//! | Transporte | Uso |
//! |---|---|
//! | [`MockTransport`] | testes, demonstração, `--sgdb mock` |
//! | [`StdioTransport`] | servidor MCP local (`nsgdb` lançado por comando) |
//! | [`HttpTransport`] | servidor MCP em `http://host:porta/mcp` |

pub mod cli;
pub mod client;
pub mod ledger;
pub mod transport;
pub mod wire;
pub mod worker;

use std::fmt;

pub use client::{
    open, open_transport, RecallPage, SgdbClient, SgdbConfig, TextHit,
};
pub use ledger::{
    encode_phases, phase_key, phases_from_payload, SgdbLedger, SgdbMetrics, SgdbStatus,
};
pub use transport::{HttpTransport, MockTransport, Outbox, StdioTransport, Transport};
pub use wire::{MemoryHit, PhaseDoc};
pub use worker::T2Worker;

/// Erros do `T2`. Nenhum deles é fatal para o motor: o pior caso é offline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SgdbError {
    /// Servidor inalcançável / conexão caiu.
    Offline,
    /// Leitura estourou o timeout.
    Timeout,
    /// Falha de arquivo local (outbox).
    Io(String),
    /// Resposta não é JSON válido.
    Json(String),
    /// Violação de protocolo (JSON-RPC/MCP/HTTP).
    Protocol(String),
    /// Erro JSON-RPC devolvido pelo servidor.
    Rpc(String),
    /// Ferramenta recusou os argumentos.
    Tool(String),
    /// Configuração inválida.
    Config(String),
    /// Recurso não suportado (ex.: `https://`).
    NotSupported(String),
}

impl fmt::Display for SgdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => f.write_str("sgdb offline"),
            Self::Timeout => f.write_str("sgdb: timeout"),
            Self::Io(msg) => write!(f, "sgdb io: {msg}"),
            Self::Json(msg) => write!(f, "sgdb json: {msg}"),
            Self::Protocol(msg) => write!(f, "sgdb protocolo: {msg}"),
            Self::Rpc(msg) => write!(f, "sgdb rpc: {msg}"),
            Self::Tool(msg) => write!(f, "sgdb ferramenta: {msg}"),
            Self::Config(msg) => write!(f, "sgdb config: {msg}"),
            Self::NotSupported(msg) => write!(f, "sgdb não suportado: {msg}"),
        }
    }
}

impl std::error::Error for SgdbError {}

impl SgdbError {
    /// `true` se vale a pena tentar de novo (queda transitória de rede).
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Offline | Self::Timeout)
    }

    /// Dica acionável para o operador (não substitui a mensagem técnica).
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Offline => {
                "verifique se o servidor MCP está no ar (aether-sgdb doctor) ou use --mock"
            }
            Self::Timeout => {
                "aumente --timeout-ms ou verifique a carga do servidor; o outbox guarda o lote"
            }
            Self::Io(_) => "verifique permissão/caminho do --sgdb-outbox",
            Self::Json(_) | Self::Protocol(_) => {
                "o endpoint respondeu algo que não é MCP/JSON; confirme a URL (/mcp) e a versão"
            }
            Self::Rpc(_) => "o servidor recusou a chamada; veja a mensagem e o escopo",
            Self::Tool(_) => "os argumentos não batem com a ferramenta; use --help",
            Self::Config(_) => "corrija as flags/ambiente (aether-sgdb doctor mostra a configuração)",
            Self::NotSupported(_) => "use http:// (sem TLS) ou o transporte stdio",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erros_tem_mensagem_estavel() {
        for e in [
            SgdbError::Offline,
            SgdbError::Timeout,
            SgdbError::Io("x".into()),
            SgdbError::Rpc("y".into()),
            SgdbError::Tool("z".into()),
        ] {
            assert!(!e.to_string().is_empty());
        }
    }
}
