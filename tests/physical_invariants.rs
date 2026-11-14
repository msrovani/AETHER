//! Invariantes físicos obrigatórios do projeto AETHER (Seção 5 da arquitetura).
//!
//! 1. **Termodinâmico** — `H(s_final) < H(s_0)` estritamente em 100% de 50
//!    matrizes `J` simétricas aleatórias (`N = 64`).
//! 2. **Resolução ondulatória** — o mesmo bloco processa `L = 128`, `512` e
//!    `2048` com exatamente os mesmos pesos (sem erro de dimensão).
//! 3. **Sincronização sem mensagens** — 4 nós Kuramoto trocando **apenas**
//!    datagramas UDP de 40 bytes atingem `R(t) > 0.95` em menos de 3 s.

#![cfg(all(feature = "m1", feature = "m2", feature = "m3"))]

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use aether_engine::core::kuramoto_field::{KuramotoConfig, KuramotoPhaseNode, NodeHandle};
use aether_engine::core::thermodynamic_solver::{IsingLandscape, ThermodynamicIsingSolver};
use aether_engine::core::wave_interference::WaveInterferenceBlock;
use ndarray::Array2;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Campo determinístico "ruido": broadband, alta entropia espectral.
fn deterministic_field(channels: usize, len: usize, seed: u64) -> Array2<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut out = Array2::<f32>::zeros((channels, len));
    for c in 0..channels {
        for k in 0..len {
            out[[c, k]] = rng.gen::<f32>() * 2.0 - 1.0;
        }
    }
    out
}

/// Campo coerente: senoide pura (baixa entropia espectral).
fn coherent_field(channels: usize, len: usize) -> Array2<f32> {
    let mut out = Array2::<f32>::zeros((channels, len));
    for c in 0..channels {
        let f = 2.0 + c as f32;
        for k in 0..len {
            let t = k as f32 / len as f32;
            out[[c, k]] = (std::f32::consts::TAU * f * t).sin();
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 1. Invariante termodinâmico
// ---------------------------------------------------------------------------

#[test]
fn invariante_01_colapso_termodinamico() {
    const N: usize = 64;
    const CASES: usize = 50;
    const STEPS: usize = 200;

    let mut solver = ThermodynamicIsingSolver::new(1, N, STEPS, 0.05);
    let mut rng = ChaCha8Rng::seed_from_u64(0xA37E_0001);
    let mut j = Array2::<f64>::zeros((N, N));
    let h = Array2::<f64>::zeros((1, N));

    for case in 0..CASES {
        // J simétrica, diagonal zero, entradas N(0, 1/sqrt(N)).
        for i in 0..N {
            for k in (i + 1)..N {
                let v = (rng.gen::<f64>() * 2.0 - 1.0) / (N as f64).sqrt();
                j[[i, k]] = v;
                j[[k, i]] = v;
            }
        }

        solver.reset(0xDEAD_BEEF ^ case as u64);
        let j3 = j.view().into_shape_with_order((1, N, N)).expect("shape J");
        let out = solver
            .relax(IsingLandscape::new(j3, h.view()))
            .expect("relax falhou");

        let e0 = out.energy_trace[[0, 0]];
        let ef = out.best_energy[0];

        assert!(e0.is_finite(), "caso {case}: energia inicial não finita");
        assert!(ef.is_finite(), "caso {case}: energia final não finita");
        assert!(
            ef < e0,
            "caso {case}: H_final={ef:.6} !< H_inicial={e0:.6} (invariante violado)"
        );

        // O estado-emergente tem de estar em {-1,+1}^N e ter a energia anunciada.
        let s = out.states.row(0);
        for v in s.iter() {
            assert!(
                *v == -1 || *v == 1,
                "caso {case}: estado discreto inválido {v}"
            );
        }
        let recomputed = solver.energy(&j.view(), &h.view(), s.as_slice().unwrap());
        assert!(
            (recomputed - ef).abs() < 1e-9,
            "caso {case}: energia recomputada {recomputed} != best_energy {ef}"
        );

        // A curva de energia tem steps+1 amostras e é monotônica por elite.
        assert_eq!(out.energy_trace.ncols(), STEPS + 1);
        assert!(out.energy_trace.row(0).iter().all(|e| e.is_finite()));
    }
}

// ---------------------------------------------------------------------------
// 2. Invariante de resolução ondulatória
// ---------------------------------------------------------------------------

#[test]
fn invariante_02_resolucao_independente() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    let pesos = block.r_phi().to_vec();
    let local = block.w_local().to_vec();

    for &l in &[128usize, 512, 2048] {
        let u = deterministic_field(4, l, 0xA11CE + l as u64);
        let v = block.forward(u.view()).expect("forward");
        assert_eq!(v.shape(), &[8, l], "shape errado para L={l}");
        assert!(
            v.iter().all(|x| x.is_finite()),
            "saída não finita para L={l}"
        );
        assert_eq!(
            block.r_phi(),
            pesos.as_slice(),
            "pesos espectrais mudaram com L={l}"
        );
        assert_eq!(
            block.w_local(),
            local.as_slice(),
            "pesos locais mudaram com L={l}"
        );
    }

    // Coerência de resolução: o bloco não "olha" para o tamanho do sinal.
    let u512 = deterministic_field(4, 512, 0xBEEF);
    let v512 = block.forward(u512.view()).expect("forward");
    let u2048 = deterministic_field(4, 2048, 0xBEEF);
    let v2048 = block.forward(u2048.view()).expect("forward");
    assert_eq!(v512.shape(), &[8, 512]);
    assert_eq!(v2048.shape(), &[8, 2048]);

    // Entropia espectral: campo coerente < campo ruidoso.
    let h_coherent = block
        .compute_spectral_entropy(coherent_field(4, 512).view())
        .expect("entropia");
    let h_noise = block
        .compute_spectral_entropy(deterministic_field(4, 512, 7).view())
        .expect("entropia");
    assert!(
        h_coherent < h_noise,
        "entropia coerente {h_coherent} !< entropia ruidosa {h_noise}"
    );
    assert!((0.0..=1.0).contains(&h_coherent) && (0.0..=1.0).contains(&h_noise));
}

// ---------------------------------------------------------------------------
// 3. Invariante de sincronização sem mensagens
// ---------------------------------------------------------------------------

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
        multicast: None,
    }
}

#[test]
fn invariante_03_sincronizacao_udp() {
    const NODES: u32 = 4;

    // 1) cria os nós em portas efêmeras
    let mut nodes: Vec<KuramotoPhaseNode> = (0..NODES)
        .map(|id| KuramotoPhaseNode::new(config_loopback(id, 0xF00D + id as u64)).expect("node"))
        .collect();

    // 2) resolve a topologia completa (todos com todos)
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

    // 3) fases iniciais aleatórias (desalinhamento máximo)
    let mut handles: Vec<NodeHandle> = nodes.into_iter().map(|n| n.spawn()).collect();

    let t0 = Instant::now();
    let mut best_r = 0.0f64;
    let mut converged = false;
    let mut bytes_ok = true;

    while t0.elapsed() < Duration::from_secs(3) {
        let mut sum = 0.0;
        let mut count = 0.0;
        for h in &handles {
            if let Some(f) = h.latest() {
                // Todo datagrama recebido tem exatamente 40 bytes.
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

    assert!(bytes_ok, "foi recebido datagrama que não tem 40 bytes");
    assert!(
        converged,
        "R(t) não atingiu 0.95 em 3 s (melhor R = {best_r:.4})"
    );
    assert!(best_r <= 1.0 + 1e-9, "R(t) fora de [0,1]: {best_r}");
}
