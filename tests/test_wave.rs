//! Testes do Módulo 2 (interferência espectral). Feature `m2`.

#![cfg(feature = "m2")]

use aether_engine::core::wave_interference::WaveInterferenceBlock;
use ndarray::Array2;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

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

#[test]
fn m2_invariante_resolucao_128_512_2048() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    let pesos = block.r_phi().to_vec();
    let local = block.w_local().to_vec();

    for &l in &[128usize, 512, 2048] {
        let u = deterministic_field(4, l, 0xA11CE + l as u64);
        let v = block.forward(u.view()).expect("forward");
        assert_eq!(v.shape(), &[8, l]);
        assert!(v.iter().all(|x| x.is_finite()));
        assert_eq!(block.r_phi(), pesos.as_slice(), "R_Phi mudou com L={l}");
        assert_eq!(block.w_local(), local.as_slice(), "W_local mudou com L={l}");
    }
}

#[test]
fn m2_l_impar_pade_para_potencia_de_dois() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    for &l in &[255usize, 999, 100] {
        let u = deterministic_field(4, l, l as u64);
        let v = block.forward(u.view()).expect("forward");
        assert_eq!(v.shape(), &[8, l], "L={l} deve voltar com o mesmo tamanho");
        assert!(v.iter().all(|x| x.is_finite()));
    }
}

#[test]
fn m2_entropia_coerente_menor_que_ruido() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    let h_coerente = block
        .compute_spectral_entropy(coherent_field(4, 512).view())
        .expect("entropia");
    let h_ruido = block
        .compute_spectral_entropy(deterministic_field(4, 512, 7).view())
        .expect("entropia");
    assert!(
        h_coerente < h_ruido,
        "coerente {h_coerente} !< ruido {h_ruido}"
    );
    assert!((0.0..=1.0).contains(&h_coerente));
    assert!((0.0..=1.0).contains(&h_ruido));
}

#[test]
fn m2_filtro_reduz_entropia_espectral() {
    // O caminho de onda é band-limited por construção: a entropia do espectro
    // transferido tem de ser menor que a do campo cru (INV-W5).
    let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, 512, 0xC0FFEE);
    let u = deterministic_field(4, 512, 99);
    let mut v = Array2::<f32>::zeros((8, 512));
    block.forward_into(u.view(), &mut v).expect("forward");
    let h_cru = block.compute_spectral_entropy(u.view()).expect("entropia");
    let h_onda = block.compute_wave_entropy();
    assert!(
        h_onda < h_cru,
        "entropia de onda {h_onda} !< entropia cru {h_cru}"
    );
}

#[test]
fn m2_lote_b_processa() {
    let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, 512, 0xC0FFEE);
    let u = deterministic_field(4, 512, 0x1BAD);
    let mut ub = Array3::<f32>::zeros((3, 4, 512));
    for b in 0..3 {
        for c in 0..4 {
            for k in 0..512 {
                ub[[b, c, k]] = u[[c, k]] * (b as f32 * 0.5 + 1.0);
            }
        }
    }
    let v = block.forward_batched(ub.view()).expect("batch");
    assert_eq!(v.shape(), &[3, 8, 512]);
    assert!(v.iter().all(|x| x.is_finite()));
}

use ndarray::Array3;
