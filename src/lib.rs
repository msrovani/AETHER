//! # AETHER — computacao pos-token baseada em fisica continua
//!
//! Prototipo experimental. Nenhuma parte do motor tokeniza, embute texto ou
//! transmite comandos semanticos: a informacao trafega sempre como sinal
//! continuo (onda, campo, fase de oscilador).
//!
//! Pilares:
//! * [`core::thermodynamic_solver`] (feature `m1`) — relaxamento termodinamico
//!   de Ising continuo via Bifurcacao Simulada Balistica (bSB).
//! * [`core::wave_interference`] (feature `m2`) — operador neural de Fourier
//!   (interferencia espectral) com transferencia independente da resolucao.
//! * [`core::kuramoto_field`] (feature `m3`) — acoplamento de fase P2P por
//!   datagramas UDP binarios de 40 bytes (zero texto, zero comando).
//! * [`simulation::closed_loop_organism`] (feature `m4`) — organismo integrado.

// `deny(unsafe_code)` (e não `forbid`): o macro `s!` do ndarray traz um
// `#[allow(unsafe_code)]` interno que um `forbid` na raiz do crate anularia.
// Nosso código não contém nenhum bloco `unsafe`.
#![deny(unsafe_code)]
#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

pub mod core;
pub mod error;
pub mod json;

/// Agente AETHER: inferência, MoE, coerência e autoaprendizado (`T2`).
#[cfg(feature = "agent")]
pub mod agent;

/// Ponte SGDB / intake (INTAKE_TRANSPLANT) — fora do hot path.
#[cfg(feature = "m1")]
pub mod knowledge;

/// Camada de plataforma (Fase 6): sensores reais, estado binário e LAN.
///
/// O núcleo auditado permanece congelado; I/O, tempo real e rede vivem aqui.
pub mod platform;

#[cfg(feature = "m4")]
pub mod simulation;

#[cfg(feature = "m5")]
pub mod phase_memory;

#[cfg(feature = "m6")]
pub mod thermodynamic_dreamer;

#[cfg(feature = "m7")]
pub mod morphogenesis;

#[cfg(feature = "m8")]
pub mod transducer_io;

#[cfg(all(feature = "m8", windows))]
pub mod transducer_host;

#[cfg(feature = "dashboard")]
pub mod dashboard;

/// Borda externa HTTP/JSON (T2) para apps de IA — `feature = "api"`.
#[cfg(feature = "api")]
pub mod api;

pub use error::{AetherError, Result};

/// Versao do protocolo de pacotes de fase (UDP, 40 bytes, little-endian).
pub const WIRE_PROTOCOL_VERSION: u8 = 1;
