//! Dashboard de métricas AETHER (borda de produto / lab visual).
//!
//! Carrega `target/aether_scorecard.json` e expõe catálogo de tooltips
//! (inferência, coerência, RAG, vazão…). A UI gráfica fica no binário
//! `aether-dashboard` (Windows + feature `dashboard`).

#![allow(clippy::needless_range_loop)]

mod ai_view;
mod catalog;
mod console;
mod dec_view;
mod demo_trace;
mod f4_view;
mod fmt;
mod gaps;
mod hitl_view;
mod host_probe;
mod jobs;
mod model;
mod transplant_view;

pub use ai_view::{
    agent_ask_detail, api_case_detail, collect_wave_gaps, load_agent_drive, load_api_eval,
    AgentAskRow, AgentDriveReport, ApiEvalCase, ApiEvalReport,
};
pub use catalog::{
    dimension_ficha, dimension_tooltip, global_tooltip, glossary_entries, throughput_tooltip,
    DimFicha,
};
pub use console::{draw_console, ConsolePane};
pub use dec_view::{dec_detail, dec_program_line, load_dec_view, DecView};
pub use demo_trace::{DemoSample, DemoTrace};
pub use f4_view::{f4_detail, f4_program_line, load_f4_view, F4View};
pub use hitl_view::{hitl_detail, hitl_program_line, load_hitl_view, HitlBand, HitlView};
pub use fmt::fmt_compact;
pub use gaps::{Gap, GapSeverity};
pub use host_probe::{host_report_path, load_host_report, probe_and_save, HostReport};
pub use jobs::{JobKind, JobPhase, JobRunner, JobSnapshot};
pub use model::{load_scorecard, DimCard, ScorecardView, ThroughputRegime, ThroughputView};
pub use transplant_view::{
    dest_detail, gates_detail, load_transplant_view, origin_detail, DonationOrigin,
    TransplantGates, TransplantView, UsoCase,
};
