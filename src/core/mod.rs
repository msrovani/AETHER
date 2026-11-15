//! Pilares fisicos do motor AETHER.
//!
//! Cada pilar vive atras de uma feature (`m1`, `m2`, `m3`) para permitir
//! builds isolados — e' assim que varios agentes trabalham em paralelo sem
//! colisao de codigo e sem bloquear o build uns dos outros.

#[cfg(feature = "m1")]
pub mod thermodynamic_solver;

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

pub use crate::error::{AetherError, Result};
