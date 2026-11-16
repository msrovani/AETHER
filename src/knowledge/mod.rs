//! Ponte de conhecimento (INTAKE_TRANSPLANT) — cartório ↔ views materializadas.
//!
//! O SGDB externo é a fonte de verdade; HV/shards são derivados.
//! Feature `m1`. Não toca o hot path do organismo.

#[cfg(feature = "m1")]
pub mod sgdb_bridge;

/// Encaixe operacional do `neural-sgdb` (`T2`): MCP, outbox e leitura de fase.
#[cfg(feature = "sgdb")]
pub mod sgdb;
