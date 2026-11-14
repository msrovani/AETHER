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

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

pub mod core;
pub mod error;

#[cfg(feature = "m4")]
pub mod simulation;

pub use error::{AetherError, Result};

/// Versao do protocolo de pacotes de fase (UDP, 40 bytes, little-endian).
pub const WIRE_PROTOCOL_VERSION: u8 = 1;
