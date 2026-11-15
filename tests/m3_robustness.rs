//! Robustez do Módulo 3 (Kuramoto P2P): deduplicação de peers.
//!
//! Um peer repetido em `KuramotoConfig::peers` ou em `set_peers` produziria
//! dois slots para um único endereço. Como um datagrama tem uma única origem,
//! só o primeiro slot viraria `heard = true` e o nó reportaria `R = 0.0` para
//! sempre (`heard < peers.len()`). Estes testes usam **apenas a API pública** e
//! provam o comportamento pela física: com dedup, os dois nós convergem.
//!
//! O overflow de `rx_bytes` (~4 GiB) é verificado no teste unitário do módulo
//! (`src/core/kuramoto_field.rs`), pois exige acesso ao contador interno.

#![cfg(feature = "m3")]

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use aether_engine::core::kuramoto_field::{
    KuramotoConfig, KuramotoPhaseNode, NodeHandle, PACKET_SIZE,
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

/// Média de `R` dos quadros disponíveis, checando o contrato do wire.
fn mean_order_parameter(handles: &[NodeHandle]) -> (f64, usize) {
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for handle in handles {
        if let Some(frame) = handle.latest() {
            assert_eq!(
                frame.rx_bytes % PACKET_SIZE as u32,
                0,
                "rx_bytes deve ser sempre multiplo de 40 (recebido {})",
                frame.rx_bytes
            );
            assert!(
                (0.0..=1.0).contains(&frame.order_parameter),
                "R fora de [0,1]: {}",
                frame.order_parameter
            );
            sum += frame.order_parameter;
            count += 1;
        }
    }
    (if count > 0 { sum / count as f64 } else { 0.0 }, count)
}

/// Três nós, com os peers repetidos 3–5x em `set_peers` e em
/// `KuramotoConfig::new`.
///
/// Sem dedup, `peers_heard` (nº de peers distintos) nunca alcança
/// `peers.len()` (nº de entradas com repetição) e `R` fica em 0.0 — o teste
/// falharia por timeout, provando a correção pelo comportamento.
#[test]
fn peer_duplicado_em_new_e_set_peers_ainda_converge() {
    const LIMIAR: f64 = 0.95;
    const PEERS_DISTINTOS: u32 = 2;

    let mut a = KuramotoPhaseNode::new(cfg(0)).expect("no a");
    let a_addr = a.local_addr();

    // Duplicatas na CONSTRUÇÃO (caminho `new`).
    let mut b_cfg = cfg(1);
    b_cfg.peers = vec![a_addr, a_addr, a_addr];
    let mut b = KuramotoPhaseNode::new(b_cfg).expect("no b");
    let b_addr = b.local_addr();

    let mut c_cfg = cfg(2);
    c_cfg.peers = vec![a_addr, b_addr, a_addr];
    let c = KuramotoPhaseNode::new(c_cfg).expect("no c");
    let c_addr = c.local_addr();

    // Malha completa com duplicatas em `set_peers` (os três enviam a todos).
    a.set_peers(vec![b_addr, b_addr, c_addr, c_addr]);
    b.set_peers(vec![a_addr, a_addr, c_addr, c_addr, c_addr]);

    let handles = vec![a.spawn(), b.spawn(), c.spawn()];

    let started = Instant::now();
    let mut best_r = 0.0f64;
    let mut converged_at: Option<Duration> = None;
    let mut saw_peer = false;

    while started.elapsed() < Duration::from_secs(3) {
        let (r, count) = mean_order_parameter(&handles);
        if count > 0 {
            if let Some(frame) = handles[0].latest() {
                if frame.peers_heard > 0 {
                    saw_peer = true;
                }
                // Peers duplicados viram UM slot cada: nunca mais de 2.
                assert!(
                    frame.peers_heard <= PEERS_DISTINTOS,
                    "peer duplicado criou slot extra: peers_heard = {}",
                    frame.peers_heard
                );
            }
            best_r = best_r.max(r);
            if r > LIMIAR && converged_at.is_none() {
                converged_at = Some(started.elapsed());
            }
            // Depois de convergir, observa mais 200 ms (R de regime).
            let observe_done = converged_at
                .map(|at| started.elapsed() >= at + Duration::from_millis(200))
                .unwrap_or(false);
            if observe_done {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    let elapsed = started.elapsed();
    for handle in handles {
        handle.stop().expect("stop do no");
    }

    eprintln!("dedup: melhor R = {best_r:.6} apos {elapsed:?} (limiar em {converged_at:?})");

    assert!(saw_peer, "nenhum peer foi ouvido");
    assert!(
        converged_at.is_some(),
        "R(t) nao atingiu {LIMIAR} em 3 s com peers duplicados (melhor R = {best_r:.4})"
    );
    assert!(best_r <= 1.0 + 1e-9);
}
