//! Dashboard de métricas AETHER (borda de produto / lab visual).
//!
//! Carrega `target/aether_scorecard.json` e expõe catálogo de tooltips
//! (inferência, coerência, RAG, vazão…). A UI gráfica fica no binário
//! `aether-dashboard` (Windows + feature `dashboard`).

#![allow(clippy::needless_range_loop)]

mod catalog;
mod demo_trace;
mod fmt;
mod gaps;
mod host_probe;
mod jobs;
mod model;
mod transplant_view;

pub use catalog::{
    dimension_ficha, dimension_tooltip, global_tooltip, glossary_entries, throughput_tooltip,
    DimFicha,
};
pub use demo_trace::{DemoSample, DemoTrace};
pub use fmt::fmt_compact;
pub use gaps::{Gap, GapSeverity};
pub use host_probe::{host_report_path, load_host_report, probe_and_save, HostReport};
pub use jobs::{JobKind, JobPhase, JobRunner, JobSnapshot};
pub use model::{load_scorecard, DimCard, ScorecardView, ThroughputRegime, ThroughputView};
pub use transplant_view::{
    dest_detail, gates_detail, load_transplant_view, origin_detail, DonationOrigin,
    TransplantGates, TransplantView, UsoCase,
};
