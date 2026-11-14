//! Testes do Módulo 3 (Kuramoto P2P por UDP binário). Feature `m3`.

#![cfg(feature = "m3")]

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use aether_engine::core::kuramoto_field::{KuramotoConfig, KuramotoPhaseNode, PhasePacket};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[test]
fn m3_codec_pacote_40_bytes() {
    assert_eq!(std::mem::size_of::<PhasePacket>(), 40);
    assert_eq!(std::mem::align_of::<PhasePacket>(), 8);

    let pkt = PhasePacket {
        timestamp_ns: 0x0123_4567_89AB_CDEF,
        node_id: 0xDEAD_BEEF,
        phase: [0.1, 1.2, 2.3, 3.4],
        xi: [-0.5, 0.25, 9.75],
    };
    let bytes = pkt.encode();
    assert_eq!(bytes.len(), 40);
    let back = PhasePacket::decode(&bytes).expect("decode");
    assert_eq!(back.node_id, pkt.node_id);
    assert_eq!(back.timestamp_ns, pkt.timestamp_ns);
    assert_eq!(back.phase, pkt.phase);
    assert_eq!(back.xi, pkt.xi);
    assert_eq!(back, pkt);

    // little-endian explícito no fio
    assert_eq!(bytes[0], 0xEF);
    assert_eq!(bytes[1], 0xCD);
    assert_eq!(bytes[8], 0xEF);
    assert_eq!(bytes[11], 0xDE);

    // datagramas de tamanho errado são rejeitados
    assert!(PhasePacket::decode(&bytes[..39]).is_err());
    assert!(PhasePacket::decode(&[0u8; 41]).is_err());
    assert!(PhasePacket::decode(&[]).is_err());
}

#[test]
fn m3_invariante_sincronizacao_udp_4_nos() {
    const NODES: u32 = 4;

    let mut nodes: Vec<KuramotoPhaseNode> = (0..NODES)
        .map(|id| KuramotoPhaseNode::new(config_loopback(id, 0xF00D + id as u64)).expect("node"))
        .collect();

    let addrs: Vec<SocketAddrV4> = nodes.iter().map(|n| n.local_addr()).collect();
    for (i, node) in nodes.iter_mut().enumerate() {
        let peers: Vec<SocketAddrV4> = addrs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, a)| *a)
            .collect();
        node.set_peers(peers);
    }

    let mut handles: Vec<_> = nodes.into_iter().map(|n| n.spawn()).collect();

    let t0 = Instant::now();
    let mut best_r = 0.0f64;
    let mut converged = false;
    let mut bytes_ok = true;

    while t0.elapsed() < Duration::from_secs(3) {
        let mut sum = 0.0f64;
        let mut count = 0.0f64;
        for h in &handles {
            if let Some(f) = h.latest() {
                if f.rx_bytes % 40 != 0 {
                    bytes_ok = false;
                }
                sum += f.order_parameter;
                count += 1.0;
            }
        }
        if count > 0.0 {
            let r = sum / count;
            best_r = best_r.max(r);
            if r > 0.95 {
                converged = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    for h in handles.drain(..) {
        let _ = h.stop();
    }

    assert!(bytes_ok, "datagrama com tamanho != 40 chegou ao nó");
    assert!(
        converged,
        "R(t) não atingiu 0.95 em 3 s (melhor R = {best_r:.4})"
    );
    assert!(best_r <= 1.0 + 1e-9);
}

#[test]
fn m3_perturbacao_propaga_sem_texto() {
    // A perturbação local ξ de um nó altera o parâmetro de ordem global dos
    // outros — nenhum payload textual/comando trafega, só fase e energia.
    let mut nodes: Vec<KuramotoPhaseNode> = (0..3u32)
        .map(|id| KuramotoPhaseNode::new(config_loopback(id, 0xBEEF + id as u64)).expect("node"))
        .collect();
    let addrs: Vec<SocketAddrV4> = nodes.iter().map(|n| n.local_addr()).collect();
    for (i, node) in nodes.iter_mut().enumerate() {
        let peers: Vec<SocketAddrV4> = addrs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, a)| *a)
            .collect();
        node.set_peers(peers);
    }

    // Convergir manualmente: rodar ~2 s de passos.
    let mut sim_nodes = nodes;
    for _ in 0..120 {
        for node in sim_nodes.iter_mut() {
            let _ = node.step(1.0 / 60.0);
        }
    }
    let r_before: f64 = sim_nodes
        .iter_mut()
        .map(|n| n.step(1.0 / 60.0).expect("step").order_parameter)
        .sum::<f64>()
        / 3.0;
    assert!(r_before > 0.95, "R antes da perturbação = {r_before:.4}");

    // Perturba o nó 0 e mede a queda de coerência.
    sim_nodes[0].inject_perturbation([30.0, 30.0, 0.0]);
    let mut r_after_sum = 0.0f64;
    for _ in 0..6 {
        for node in sim_nodes.iter_mut() {
            let f = node.step(1.0 / 60.0).expect("step");
            r_after_sum += f.order_parameter;
        }
    }
    let r_after = r_after_sum / 18.0;
    assert!(
        r_after < r_before,
        "a perturbação local deveria degradar R: antes {r_before:.4} depois {r_after:.4}"
    );
}

fn config_loopback(id: u32, seed: u64) -> KuramotoConfig {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let omega = [
        6.0 + (rng.gen::<f64>() - 0.5) * 0.04,
        6.0 + (rng.gen::<f64>() - 0.5) * 0.04,
        6.0 + (rng.gen::<f64>() - 0.5) * 0.04,
        6.0 + (rng.gen::<f64>() - 0.5) * 0.04,
    ];
    KuramotoConfig {
        node_id: id,
        bind: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
        peers: Vec::new(),
        omega,
        k_coupling: 8.0,
        hz: 60.0,
        adjacency: None,
        perturb_duration: Duration::from_millis(250),
    }
}
