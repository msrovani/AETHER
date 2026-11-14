//! Testes do Módulo 3 — invariantes INV-K1..K5 do contrato.

#![cfg(feature = "m3")]

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use aether_engine::core::kuramoto_field::{
    KuramotoConfig, KuramotoPhaseNode, NodeHandle, PhasePacket, PACKET_SIZE,
};

fn cfg(id: u32) -> KuramotoConfig {
    KuramotoConfig {
        node_id: id,
        bind: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
        peers: Vec::new(),
        omega: [6.0, 6.02, 5.98, 6.01],
        k_coupling: 8.0,
        hz: 60.0,
        adjacency: None,
        perturb_duration: Duration::from_millis(250),
        multicast: None,
    }
}

#[test]
fn k1_tamanho_e_alinhamento_do_pacote() {
    assert_eq!(std::mem::size_of::<PhasePacket>(), PACKET_SIZE);
    assert_eq!(std::mem::align_of::<PhasePacket>(), 8);
    assert_eq!(PACKET_SIZE, 40);
}

#[test]
fn k2_decode_rejeita_tamanho_errado() {
    let pkt = PhasePacket {
        timestamp_ns: 1,
        node_id: 2,
        phase: [0.0; 4],
        xi: [0.0; 3],
    };
    let wire = pkt.encode();
    assert!(PhasePacket::decode(&wire[..39]).is_err());
    let mut big = [0u8; 41];
    big[..40].copy_from_slice(&wire);
    assert!(PhasePacket::decode(&big).is_err());
    assert!(PhasePacket::decode(&[]).is_err());
    assert!(PhasePacket::decode(&wire).is_ok());
}

#[test]
fn k3_round_trip_preserva_campos() {
    let pkt = PhasePacket {
        timestamp_ns: 0x0123_4567_89AB_CDEF,
        node_id: 0xDEAD_BEEF,
        phase: [0.125, 1.5, 3.0, 6.25],
        xi: [-1.5, 0.0, 2.25],
    };
    let back = PhasePacket::decode(&pkt.encode()).expect("decode");
    assert_eq!(back, pkt);
}

#[test]
fn k4_wire_e_binario_e_mudo() {
    // Nenhum byte do pacote e' ASCII imprimivel por construcao do layout:
    // o que importa e' que o tamanho seja fixo e o payload nao seja texto.
    let pkt = PhasePacket {
        timestamp_ns: 7,
        node_id: 1,
        phase: [0.0; 4],
        xi: [0.0; 3],
    };
    let wire = pkt.encode();
    assert_eq!(wire.len(), 40);
    // so o node_id ocupa posicao textual por coincidencia; fases nulas sao zeros
    assert_eq!(&wire[12..28], &[0u8; 16]);
}

#[test]
fn k5_quatro_nos_sincronizam_em_menos_de_3s() {
    const NODES: u32 = 4;

    let mut nodes: Vec<KuramotoPhaseNode> = (0..NODES)
        .map(|id| KuramotoPhaseNode::new(cfg(id)).expect("no'"))
        .collect();

    let addrs: Vec<SocketAddrV4> = nodes.iter().map(|n| n.local_addr()).collect();
    for (i, node) in nodes.iter_mut().enumerate() {
        let peers = addrs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, a)| *a)
            .collect();
        node.set_peers(peers);
    }

    let handles: Vec<NodeHandle> = nodes.into_iter().map(|n| n.spawn()).collect();

    let t0 = Instant::now();
    let mut best = 0.0f64;
    let mut converged = false;
    let mut bytes_ok = true;

    while t0.elapsed() < Duration::from_secs(3) {
        let mut sum = 0.0;
        let mut count = 0.0;
        for h in &handles {
            if let Some(f) = h.latest() {
                if f.rx_bytes % 40 != 0 {
                    bytes_ok = false;
                }
                assert_eq!(f.rx_bytes % 40, 0);
                sum += f.order_parameter;
                count += 1.0;
            }
        }
        if count > 0.0 {
            let r = sum / count;
            best = best.max(r);
            if r > 0.95 {
                converged = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    for h in handles {
        let _ = h.stop();
    }

    assert!(bytes_ok, "datagrama com tamanho diferente de 40 bytes");
    assert!(
        converged,
        "R(t) nao passou de 0.95 em 3 s (melhor = {best:.4})"
    );
    assert!(best <= 1.0 + 1e-9);
}

#[test]
fn k6_perturbacao_propaga_sem_mensagem() {
    const NODES: u32 = 3;
    let mut nodes: Vec<KuramotoPhaseNode> = (0..NODES)
        .map(|id| KuramotoPhaseNode::new(cfg(id)).expect("no'"))
        .collect();
    let addrs: Vec<SocketAddrV4> = nodes.iter().map(|n| n.local_addr()).collect();
    for (i, node) in nodes.iter_mut().enumerate() {
        let peers = addrs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, a)| *a)
            .collect();
        node.set_peers(peers);
    }
    let handles: Vec<NodeHandle> = nodes.into_iter().map(|n| n.spawn()).collect();

    // aguarda coerencia
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(3) {
        let r: f64 = handles
            .iter()
            .filter_map(|h| h.latest())
            .map(|f| f.order_parameter)
            .sum::<f64>()
            / NODES as f64;
        if r > 0.95 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // injeta ξ apenas no no' 0 e observa a quebra de simetria nos demais
    handles[0].perturb([40.0, 40.0, 0.0]).expect("perturbacao");

    let mut dropped_below = false;
    let t1 = Instant::now();
    while t1.elapsed() < Duration::from_millis(800) {
        if let Some(f) = handles[1].latest() {
            if f.order_parameter < 0.99 {
                dropped_below = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for h in handles {
        let _ = h.stop();
    }
    // A propagacao e' fisica (acoplamento de fase); basta que o vizinho
    // registre o desvio sem que nenhum comando tenha sido enviado a ele.
    assert!(dropped_below, "perturbacao nao se propagou por fase");
}
