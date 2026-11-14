//! Testes do Módulo 2 — invariantes INV-W1..W5 do contrato.

#![cfg(feature = "m2")]

use aether_engine::core::wave_interference::WaveInterferenceBlock;
use ndarray::Array2;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn noise(c: usize, l: usize, seed: u64) -> Array2<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    Array2::<f32>::from_shape_fn((c, l), |_| rng.gen::<f32>() * 2.0 - 1.0)
}

fn sine_field(c: usize, l: usize) -> Array2<f32> {
    Array2::<f32>::from_shape_fn((c, l), |(ch, x)| {
        let t = x as f32 / l as f32;
        (std::f32::consts::TAU * (2.0 + ch as f32) * t).sin()
    })
}

#[test]
fn w1_pesos_invariantes_entre_resolucoes() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    let r_phi_antes = block.r_phi().to_owned();
    let w_local_antes = block.w_local().to_owned();

    for &l in &[128usize, 512, 2048, 4096] {
        let v = block.forward(noise(4, l, l as u64).view()).expect("forward");
        assert_eq!(v.dim(), (8, l));
        assert_eq!(block.r_phi(), r_phi_antes.view(), "r_phi mudou com L={l}");
        assert_eq!(block.w_local(), w_local_antes.view(), "w_local mudou com L={l}");
    }
}

#[test]
fn w2_resolucoes_arbitrarias_sem_erro_de_dimensao() {
    let mut block = WaveInterferenceBlock::with_max_len(2, 3, 16, 2048, 7);
    for &l in &[16usize, 64, 128, 512, 2048] {
        let out = block.forward(noise(2, l, 99).view()).expect("forward");
        assert_eq!(out.dim(), (3, l));
        assert!(out.iter().all(|x| x.is_finite()), "saida nao finita em L={l}");
    }
    // acima do teto: erro explicito, nunca silencio
    assert!(block.forward(noise(2, 4096, 1).view()).is_err());
    // comprimento impar: erro explicito
    assert!(block.forward(noise(2, 129, 1).view()).is_err());
    // numero de canais errado: erro explicito
    assert!(block.forward(noise(5, 128, 1).view()).is_err());
}

#[test]
fn w3_saida_limitada_e_finita() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0x5150);
    let v = block.forward(noise(4, 512, 3).view()).expect("forward");
    assert!(v.iter().all(|x| x.is_finite() && x.abs() <= 1.0 + 1e-6));
}

#[test]
fn w4_entropia_coerente_menor_que_ruido() {
    let block = WaveInterferenceBlock::new(4, 8, 32, 0xBEEF);
    let h_coherent = block
        .compute_spectral_entropy(sine_field(4, 512).view())
        .expect("entropia");
    let h_noise = block
        .compute_spectral_entropy(noise(4, 512, 11).view())
        .expect("entropia");
    assert!(
        h_coherent < h_noise,
        "coerente {h_coherent} !< ruido {h_noise}"
    );
    assert!((0.0..=1.0).contains(&h_coherent));
    assert!((0.0..=1.0).contains(&h_noise));
}

#[test]
fn w5_filtro_de_banda_reduz_entropia() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 0xABCD);
    block.forward(noise(4, 512, 21).view()).expect("forward");
    let h_raw = block
        .compute_spectral_entropy(noise(4, 512, 21).view())
        .expect("entropia crua");
    let h_wave = block.compute_wave_entropy();
    assert!(
        h_wave <= h_raw + 1e-9,
        "espectro transferido {h_wave} !<= cru {h_raw}"
    );
}

#[test]
fn w6_forward_batched_por_item() {
    let mut block = WaveInterferenceBlock::new(2, 4, 16, 0x77);
    let b = 3usize;
    let l = 256usize;
    let mut u = ndarray::Array3::<f32>::zeros((b, 2, l));
    for i in 0..b {
        for c in 0..2 {
            for x in 0..l {
                u[[i, c, x]] = noise(2, l, i as u64 + 1)[[c, x]];
            }
        }
    }
    let out = block.forward_batched(u.view()).expect("batched");
    assert_eq!(out.dim(), (b, 4, l));
    assert!(out.iter().all(|x| x.is_finite()));
}
