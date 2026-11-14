//! Testes do Módulo 3 — Kuramoto P2P sobre UDP binário de 40 bytes.
//!
//! Cobrem os invariantes `INV-K1..K5` do `docs/API_CONTRACT.md`:
//! layout do pacote, round-trip do codec, rejeição de tamanhos errados,
//! `R(t) ∈ [0,1]`, convergência de 4 nós em loopback e integridade do wire.

#![cfg(feature = "m3")]

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use aether_engine::core::kuramoto_field::{
    KuramotoConfig, KuramotoPhaseNode, NodeHandle, PhasePacket, PACKET_SIZE,
};
use aether_engine::AetherError;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn config_loopback(node_id: u32) -> KuramotoConfig {
    // Frequências naturais quase idênticas (dispersão ±0.02 rad/s), como no
    // invariante 3 da especificação.
    let base = 6.0f64;
    let offsets = [0.0, 0.013, -0.011, 0.007];
    KuramotoConfig {
        node_id,
        bind: SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0),
        peers: Vec::new(),
        omega: [
            base + offsets[0],
            base + offsets[1],
            base + offsets[2],
            base + offsets[3],
        ],
        k_coupling: 8.0,
        hz: 60.0,
        adjacency: None,
        perturb_duration: Duration::from_millis(250),
    }
}

fn node(id: u32) -> KuramotoPhaseNode {
    KuramotoPhaseNode::new(config_loopback(id)).expect("no kuramoto")
}

/// Todos-com-todos: cada nó conhece todos os outros endereços efêmeros.
fn wire_full_mesh(nodes: &mut [KuramotoPhaseNode]) {
    let addrs: Vec<SocketAddrV4> = nodes.iter().map(KuramotoPhaseNode::local_addr).collect();
    for (i, node) in nodes.iter_mut().enumerate() {
        let peers = addrs
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, addr)| *addr)
            .collect();
        node.set_peers(peers);
    }
}

// ---------------------------------------------------------------------------
// INV-K1 — layout de 40 bytes / align 8
// ---------------------------------------------------------------------------

#[test]
fn packet_layout_e_40_bytes_com_align_8() {
    assert_eq!(std::mem::size_of::<PhasePacket>(), PACKET_SIZE);
    assert_eq!(std::mem::size_of::<PhasePacket>(), 40);
    assert_eq!(std::mem::align_of::<PhasePacket>(), 8);

    // Layout little-endian explícito: offsets 0, 8, 12, 28.
    let packet = PhasePacket {
        timestamp_ns: 0x0102_0304_0506_0708,
        node_id: 0xDEAD_BEEF,
        phase: [0.25, 1.5, -2.0, 6.0],
        xi: [1.0, -1.0, 0.5],
    };
    let bytes = packet.encode();
    assert_eq!(bytes.len(), PACKET_SIZE);
    assert_eq!(&bytes[0..8], &0x0102_0304_0506_0708u64.to_le_bytes());
    assert_eq!(&bytes[8..12], &0xDEAD_BEEFu32.to_le_bytes());
    assert_eq!(&bytes[12..16], &0.25f32.to_le_bytes());
    assert_eq!(&bytes[16..20], &1.5f32.to_le_bytes());
    assert_eq!(&bytes[28..32], &1.0f32.to_le_bytes());
    assert_eq!(&bytes[32..36], &(-1.0f32).to_le_bytes());
}

// ---------------------------------------------------------------------------
// INV-K2 — decode rejeita qualquer tamanho != 40
// ---------------------------------------------------------------------------

#[test]
fn decode_rejeita_tamanhos_invalidos() {
    for len in [0usize, 1, 39, 41, 64, 80] {
        let buf = vec![0u8; len];
        match PhasePacket::decode(&buf) {
            Err(AetherError::BadPacketLen { len: got, expected }) => {
                assert_eq!(got, len, "tamanho reportado deve ser o recebido");
                assert_eq!(expected, PACKET_SIZE);
            }
            other => panic!("len={len}: esperava BadPacketLen, veio {other:?}"),
        }
    }
    // Exatamente 40 bytes sempre decodifica.
    assert!(PhasePacket::decode(&[0u8; PACKET_SIZE]).is_ok());
}

// ---------------------------------------------------------------------------
// INV-K3 — round-trip preserva todos os campos
// ---------------------------------------------------------------------------

#[test]
fn encode_decode_roundtrip() {
    let packet = PhasePacket {
        timestamp_ns: 0xA37E_1234_5678_9ABC,
        node_id: 7,
        phase: [0.0, 1.0 / 3.0, 2.5, 5.9],
        xi: [-12.5, 0.0, 3.25],
    };
    let decoded = PhasePacket::decode(&packet.encode()).expect("roundtrip");
    assert_eq!(decoded, packet);
    assert_eq!(decoded.timestamp_ns, packet.timestamp_ns);
    assert_eq!(decoded.node_id, packet.node_id);
    assert_eq!(decoded.phase, packet.phase);
    assert_eq!(decoded.xi, packet.xi);
}

// ---------------------------------------------------------------------------
// INV-K5 (pré-condição) — R reportado é honesto antes de ouvir todos os peers
// ---------------------------------------------------------------------------

#[test]
fn order_parameter_nao_converge_antes_de_ouvir_os_peers() {
    let mut a = node(0);
    let b = node(1);
    a.set_peers(vec![b.local_addr()]);

    // Nenhum pacote de `b` foi recebido: R deve ser exatamente 0.0 (nunca 1.0).
    let frame = a.step(1.0 / 60.0).expect("step");
    assert_eq!(
        frame.order_parameter, 0.0,
        "R nao pode ser reportado antes de ouvir todos os peers"
    );
    assert_eq!(frame.rx_packets, 0);
    assert_eq!(frame.rx_bytes, 0);

    // Nó isolado (sem peers): o único oscilador está em fase consigo mesmo.
    let mut solo = node(2);
    let frame = solo.step(1.0 / 60.0).expect("step solo");
    assert!(
        (0.0..=1.0).contains(&frame.order_parameter),
        "R fora de [0,1]: {}",
        frame.order_parameter
    );
    assert!(frame.phase.iter().all(|theta| (0.0..std::f64::consts::TAU).contains(theta)));
}

#[test]
fn step_rejeita_dt_invalido() {
    let mut solo = node(3);
    assert!(solo.step(0.0).is_err());
    assert!(solo.step(-0.016).is_err());
    assert!(solo.step(f64::NAN).is_err());
}

// ---------------------------------------------------------------------------
// INV-K4 + INV-K5 — 4 nós em loopback convergem a R > 0.95 em < 3 s
// ---------------------------------------------------------------------------

#[test]
fn quatro_nos_convergem_em_menos_de_3s() {
    const NODES: u32 = 4;
    const LIMIAR: f64 = 0.95;

    let mut nodes: Vec<KuramotoPhaseNode> = (0..NODES).map(node).collect();
    wire_full_mesh(&mut nodes);
    let handles: Vec<NodeHandle> = nodes.into_iter().map(KuramotoPhaseNode::spawn).collect();

    let started = Instant::now();
    let mut best_r = 0.0f64;
    let mut converged = false;
    let mut saw_packets = false;
    let mut wire_ok = true;

    while started.elapsed() < Duration::from_secs(3) {
        let mut sum = 0.0f64;
        let mut count = 0.0f64;
        for handle in &handles {
            if let Some(frame) = handle.latest() {
                // INV-K4: o wire só transporta múltiplos exatos de 40 bytes.
                if frame.rx_bytes % PACKET_SIZE as u32 != 0 {
                    wire_ok = false;
                }
                if frame.rx_packets > 0 {
                    saw_packets = true;
                }
                assert!(
                    (0.0..=1.0).contains(&frame.order_parameter),
                    "R fora de [0,1]: {}",
                    frame.order_parameter
                );
                sum += frame.order_parameter;
                count += 1.0;
            }
        }
        if count > 0.0 {
            let r = sum / count;
            best_r = best_r.max(r);
            if r > LIMIAR {
                converged = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let elapsed = started.elapsed();
    for handle in handles {
        handle.stop().expect("stop do no");
    }

    assert!(wire_ok, "rx_bytes nao e multiplo de 40");
    assert!(saw_packets, "nenhum datagrama de 40 bytes foi contabilizado");
    assert!(
        converged,
        "R(t) nao atingiu {LIMIAR} em 3 s (melhor R = {best_r:.4}, t = {elapsed:?})"
    );
    assert!(best_r <= 1.0 + 1e-9);
    assert!(elapsed < Duration::from_secs(3), "convergencia levou {elapsed:?}");
}

// ---------------------------------------------------------------------------
// Handle: perturbação DC/oscilatória e parada limpa
// ---------------------------------------------------------------------------

#[test]
fn handle_perturba_e_para() {
    let mut a = node(0);
    let mut b = node(1);
    let a_addr = a.local_addr();
    let b_addr = b.local_addr();
    a.set_peers(vec![b_addr]);
    b.set_peers(vec![a_addr]);

    let ha = a.spawn();
    let hb = b.spawn();

    assert_eq!(ha.node_id(), 0);
    assert_eq!(hb.node_id(), 1);

    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline && ha.latest().is_none() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let frame = ha.latest().expect("frame publicado");
    assert!(frame.tx_packets > 0, "o no deve transmitir a cada tick");
    assert!(frame.phase.iter().all(|theta| theta.is_finite()));

    ha.perturb([0.5, -0.5, 0.0]).expect("perturb DC");
    ha.perturb_oscillatory([20.0, 20.0, 0.0], 10.0)
        .expect("perturb oscilatoria");
    assert!(ha.perturb_oscillatory([0.0; 3], -1.0).is_err());

    // Deixa a perturbação oscilatória agir por alguns períodos.
    std::thread::sleep(Duration::from_millis(80));
    let after = ha.latest().expect("frame apos perturbacao");
    assert!(after.phase.iter().all(|theta| theta.is_finite()));
    assert!((0.0..=1.0).contains(&after.order_parameter));

    ha.stop().expect("stop a");
    hb.stop().expect("stop b");
}
