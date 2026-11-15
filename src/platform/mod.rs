//! Camada de plataforma: hardware real, estado binário e LAN (Fase 6).
//!
//! O núcleo auditado (`crate::core`) permanece congelado em `v1.0-core`; tudo
//! que toca o sistema operacional — tempo real, terminal, arquivos e rede —
//! vive aqui. Nenhum módulo desta casca usa `unsafe`.
//!
//! * [`state_bridge`] — frame binário de 32 bytes com CRC-32 (consumo externo);
//! * [`host_transducer`] — sensores reais do PC (jitter, carga, cadência);
//! * [`lan`] — configuração de multicast para o nó Kuramoto (rede local).

pub mod state_bridge;

#[cfg(all(feature = "m2", feature = "m5"))]
pub mod host_transducer;

#[cfg(feature = "m3")]
pub mod lan;
