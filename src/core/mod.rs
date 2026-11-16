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

// E3 — transplantação geométrica (JL binário + fases lineares + verificador).
#[cfg(feature = "m1")]
pub mod transplant;

#[cfg(feature = "m1")]
pub mod active_inference;

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
pub mod gpu_gate;
pub mod noise;

pub use crate::error::{AetherError, Result};
