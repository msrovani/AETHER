//! Dashboard de métricas AETHER (borda de produto / lab visual).
//!
//! Carrega `target/aether_scorecard.json` e expõe catálogo de tooltips
//! (inferência, coerência, RAG, vazão…). A UI gráfica fica no binário
//! `aether-dashboard` (Windows + feature `dashboard`).

#![allow(clippy::needless_range_loop)]

mod catalog;
mod model;

pub use catalog::{dimension_tooltip, global_tooltip, throughput_tooltip};
pub use model::{
    load_scorecard, DimCard, ScorecardView, ThroughputRegime, ThroughputView,
};
