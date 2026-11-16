//! Pilares fisicos do motor AETHER.
//!
//! Cada pilar vive atras de uma feature (`m1`, `m2`, `m3`) para permitir
//! builds isolados — e' assim que varios agentes trabalham em paralelo sem
//! colisao de codigo e sem bloquear o build uns dos outros.

#[cfg(feature = "m1")]
pub mod thermodynamic_solver;

#[cfg(feature = "m1")]
pub mod certificate;

#[cfg(feature = "m1")]
pub mod presolve;

// E2 — pipeline lógico de 5 estágios (presolve → persistência → decomposição
// → BSB+Lagrange → certificado). Depende do solver e do certificado.
#[cfg(feature = "m1")]
pub mod logic_pipeline;

// F1/E3 — proveniência e ponte de eventos read-only (SPSC, zero-alloc).
#[cfg(feature = "m1")]
pub mod knowledge_bridge;

// F1 (Vol. III §7) — ponte T0→T1→T2 com payload de fase, escopo e certificado.
// Arquivo separado do `knowledge_bridge.rs` (claim MSG 0107); ver doc do módulo.
#[cfg(feature = "m1")]
pub mod knowledge_bridge_f1;

// E3 — transplantação geométrica (JL binário + fases lineares + verificador).
#[cfg(feature = "m1")]
pub mod transplant;

#[cfg(feature = "m1")]
pub mod active_inference;

// E4 — self-field de 16 canais (auto-predição + abstenção metacognitiva).
#[cfg(feature = "m1")]
pub mod self_field;

// F2/F3 — consolidação T1, gist L4 e fusão 2D com cue_into escopado.
#[cfg(feature = "m1")]
pub mod knowledge_fusion;

// E8 — reservatório congelado × física plástica (readout RLS).
#[cfg(feature = "m1")]
pub mod reservoir;

// E7 — corpo S0: telemetria real + 4 atuadores + teste anti-passeio.
#[cfg(feature = "m1")]
pub mod s0_body;

#[cfg(feature = "m1")]
pub mod koopman_sindy;

#[cfg(feature = "m1")]
pub mod renormalization;

#[cfg(feature = "m1")]
pub mod swarm_speciation;

#[cfg(feature = "m2")]
pub mod wave_interference;

#[cfg(feature = "m3")]
pub mod kuramoto_field;

#[cfg(feature = "m5")]
pub mod fhrr;

// FASE 5 — hardware extremus.
pub mod bam_phase;
pub mod bam_store;
pub mod core_topology;
pub mod silicon_entropy;
pub mod spsc_ring;

#[cfg(feature = "m1")]
pub mod sindy_jit;

#[cfg(feature = "m1")]
pub mod aether_governor;

#[cfg(feature = "m1")]
pub mod affinity;
#[cfg(feature = "m5")]
pub mod fhrr_bam;
// E1 — Phase-Language Head (sharded + ressonador ACF + acoplamento RG).
pub mod gpu_gate;
pub mod noise;
#[cfg(feature = "m5")]
pub mod phase_language;

pub use crate::error::{AetherError, Result};
