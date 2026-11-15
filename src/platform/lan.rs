//! Configuração de LAN do governador (UDP multicast `239.255.42.99:7777`).
//!
//! O transporte vive no núcleo (`crate::core::kuramoto_field`), que já entra no
//! grupo (`join_multicast_v4`), liga o *loop* local e fala pacotes binários de
//! 40 bytes. Aqui só montamos a configuração de plataforma: bind no grupo,
//! `peers` apontando para o próprio grupo (um envio alcança todos os membros)
//! e identificação do nó.
//!
//! Rodar duas instâncias na mesma máquina funciona porque o nó liga
//! `SO_REUSEADDR`; o eco do próprio `node_id` é descartado na recepção.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use crate::core::kuramoto_field::KuramotoConfig;

/// Endereço do grupo multicast.
pub const LAN_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);
/// Porta do grupo.
pub const LAN_PORT: u16 = 7777;
/// Taxa do laço de fase (Hz).
pub const LAN_HZ: f64 = 20.0;

/// Configuração de nó para a LAN (multicast).
pub fn node_config(node_id: u32, hz: f64) -> KuramotoConfig {
    let group = SocketAddrV4::new(LAN_MULTICAST_ADDR, LAN_PORT);
    KuramotoConfig {
        node_id,
        bind: SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, LAN_PORT),
        peers: vec![group],
        omega: [1.10, 0.90, 1.25, 0.75],
        k_coupling: 8.0,
        hz,
        adjacency: None,
        perturb_duration: Duration::from_millis(150),
        multicast: Some(LAN_MULTICAST_ADDR),
    }
}

/// Configuração de nó para um grupo arbitrário (testes/duas redes).
pub fn node_config_at(node_id: u32, hz: f64, group: Ipv4Addr, port: u16) -> KuramotoConfig {
    let mut cfg = node_config(node_id, hz);
    cfg.bind = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    cfg.peers = vec![SocketAddrV4::new(group, port)];
    cfg.multicast = Some(group);
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_aponta_para_o_grupo_e_porta_do_contrato() {
        let cfg = node_config(42, LAN_HZ);
        assert_eq!(cfg.bind.port(), 7777);
        assert_eq!(cfg.multicast, Some(LAN_MULTICAST_ADDR));
        assert_eq!(cfg.peers, vec![SocketAddrV4::new(LAN_MULTICAST_ADDR, 7777)]);
        assert_eq!(cfg.node_id, 42);
    }
}
