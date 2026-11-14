//! Testes do Módulo 2 — Operador Neural de Fourier (`WaveInterferenceBlock`).
//!
//! Cobre os invariantes do contrato (`docs/API_CONTRACT.md`):
//!
//! * `INV-W1` pesos (`r_phi`, `w_local`) não mudam com `L` (bit-a-bit).
//! * `INV-W2` aceita `L = 128, 512, 2048, 4096` (e também ímpares).
//! * `INV-W3` saída `[C_out, L]`, finita e limitada por `tanh`.
//! * `INV-W4` entropia espectral de campo coerente < de ruído.
//! * `INV-W5` entropia do espectro transferido <= entropia do campo cru.
//!
//! Mais: `forward_into == forward`, `forward_batched` == por item,
//! determinismo por semente, `wave_spectrum` com shape `[C_out, modes]`,
//! zero-padding de `L` ímpar e rejeição de shapes inválidos.

#![cfg(feature = "m2")]

use aether_engine::core::wave_interference::WaveInterferenceBlock;
use ndarray::{s, Array2, Array3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Campo de ruído broadband determinístico (alta entropia espectral).
fn noise_field(channels: usize, len: usize, seed: u64) -> Array2<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    Array2::<f32>::from_shape_fn((channels, len), |_| rng.gen::<f32>() * 2.0 - 1.0)
}

/// Campo coerente: senoides de baixa frequência (baixa entropia espectral).
fn sine_field(channels: usize, len: usize) -> Array2<f32> {
    Array2::<f32>::from_shape_fn((channels, len), |(c, k)| {
        let f = 2.0 + c as f32;
        let t = k as f32 / len as f32;
        (std::f32::consts::TAU * f * t).sin()
    })
}

// ---------------------------------------------------------------------------
// INV-W1 + INV-W2 + INV-W3
// ---------------------------------------------------------------------------

#[test]
fn inv_w1_pesos_invariantes_a_resolucao() {
    const C_IN: usize = 4;
    const C_OUT: usize = 8;
    const MODES: usize = 32;

    let mut block = WaveInterferenceBlock::new(C_IN, C_OUT, MODES, 0xC0FFEE);

    // Snapshot dos pesos antes de qualquer forward.
    let r_phi_antes: Vec<_> = block.r_phi().to_owned();
    let w_local_antes: Vec<_> = block.w_local().to_owned();
    assert_eq!(r_phi_antes.len(), C_OUT * C_IN * MODES);
    assert_eq!(w_local_antes.len(), C_OUT * C_IN);

    for &l in &[128usize, 512, 2048, 4096] {
        let u = noise_field(C_IN, l, 0xA11CE + l as u64);
        let v = block.forward(u.view()).expect("forward");

        assert_eq!(v.shape(), &[C_OUT, l], "INV-W3: shape errado para L={l}");
        assert!(
            v.iter().all(|x| x.is_finite()),
            "INV-W3: saída não finita em L={l}"
        );
        assert!(
            v.iter().all(|x| x.abs() <= 1.0),
            "INV-W3: tanh deveria limitar a saída a [-1, 1] em L={l}"
        );

        assert_eq!(
            block.r_phi(),
            r_phi_antes.as_slice(),
            "INV-W1: r_phi mudou com L={l}"
        );
        assert_eq!(
            block.w_local(),
            w_local_antes.as_slice(),
            "INV-W1: w_local mudou com L={l}"
        );
    }
}

#[test]
fn inv_w2_aceita_resolucoes_variadas() {
    let mut block = WaveInterferenceBlock::new(3, 5, 16, 0x5EED);
    // Pares "bons", pares ruins e ímpares (zero-padding documentado).
    for &l in &[2usize, 8, 64, 128, 255, 256, 511, 512, 999, 2048, 4096] {
        let u = noise_field(3, l, l as u64 ^ 0x1234);
        let v = block.forward(u.view()).expect("forward");
        assert_eq!(v.dim(), (5, l), "shape errado para L={l}");
        assert!(v.iter().all(|x| x.is_finite()), "não finito em L={l}");
        // INV-W1 também vale aqui: mesma instância, L diferentes.
        assert_eq!(block.r_phi().len(), 5 * 3 * 16);
        assert_eq!(block.w_local().len(), 5 * 3);
    }
}

#[test]
fn inv_w3_caminho_quente_repete_resultado() {
    // Com `with_max_len` o mesmo scratch é reutilizado para L <= max_len.
    let mut block = WaveInterferenceBlock::with_max_len(4, 8, 32, 512, 0xABCD);
    for &l in &[128usize, 200, 255, 256, 511, 512] {
        let u = noise_field(4, l, 77 + l as u64);
        let mut out = Array2::<f32>::zeros((8, l));
        block
            .forward_into(u.view(), &mut out)
            .expect("forward_into");
        assert_eq!(out.dim(), (8, l));
        assert!(out.iter().all(|x| x.is_finite()));

        // Segunda passada no mesmo L: bit-a-bit idêntica (scratch limpo).
        let mut out2 = Array2::<f32>::zeros((8, l));
        block
            .forward_into(u.view(), &mut out2)
            .expect("forward_into 2");
        assert_eq!(out, out2, "reuso do scratch alterou o resultado em L={l}");
    }
}

// ---------------------------------------------------------------------------
// forward_into == forward ; batched == por item
// ---------------------------------------------------------------------------

#[test]
fn forward_into_equivale_a_forward() {
    let u = noise_field(4, 512, 11);
    let mut a = WaveInterferenceBlock::new(4, 8, 32, 42);
    let mut b = WaveInterferenceBlock::with_max_len(4, 8, 32, 512, 42);

    let v = a.forward(u.view()).expect("forward");
    let mut out = Array2::<f32>::zeros((8, 512));
    b.forward_into(u.view(), &mut out).expect("forward_into");

    assert_eq!(v, out);
    assert!(v
        .iter()
        .zip(out.iter())
        .all(|(x, y)| x.to_bits() == y.to_bits()));
}

#[test]
fn batched_equivale_a_processar_item_a_item() {
    const BATCH: usize = 6;
    const L: usize = 256;

    let mut rng = ChaCha8Rng::seed_from_u64(0xB00B5);
    let batch = Array3::<f32>::from_shape_fn((BATCH, 4, L), |_| rng.gen::<f32>() * 2.0 - 1.0);

    let mut block = WaveInterferenceBlock::new(4, 8, 32, 9);
    let v = block
        .forward_batched(batch.view())
        .expect("forward_batched");
    assert_eq!(v.dim(), (BATCH, 8, L));
    assert!(v.iter().all(|x| x.is_finite()));

    // Mesma semente => mesmos pesos => cada item tem de coincidir bit-a-bit.
    let mut single = WaveInterferenceBlock::new(4, 8, 32, 9);
    for b in 0..BATCH {
        let item = single
            .forward(batch.slice(s![b, .., ..]))
            .expect("forward item");
        assert_eq!(
            v.slice(s![b, .., ..]),
            item,
            "item {b} do batch difere do forward escalar"
        );
    }
}

// ---------------------------------------------------------------------------
// Determinismo
// ---------------------------------------------------------------------------

#[test]
fn mesma_semente_mesmos_pesos_e_saida() {
    let u = noise_field(4, 256, 3);
    let mut a = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    let mut b = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEE);
    assert_eq!(a.r_phi(), b.r_phi());
    assert_eq!(a.w_local(), b.w_local());
    assert_eq!(
        a.forward(u.view()).expect("a"),
        b.forward(u.view()).expect("b")
    );

    // Semente diferente => pesos diferentes.
    let c = WaveInterferenceBlock::new(4, 8, 32, 0xC0FFEF);
    assert_ne!(a.r_phi(), c.r_phi());
}

// ---------------------------------------------------------------------------
// INV-W4 — entropia espectral: coerente < ruído
// ---------------------------------------------------------------------------

#[test]
fn inv_w4_entropia_coerente_menor_que_ruido() {
    // `compute_spectral_entropy` toma `&self`.
    let block = WaveInterferenceBlock::new(4, 8, 32, 0x5EED);

    let h_coherent = block
        .compute_spectral_entropy(sine_field(4, 512).view())
        .expect("entropia coerente");
    let h_noise = block
        .compute_spectral_entropy(noise_field(4, 512, 7).view())
        .expect("entropia ruído");

    assert!(
        h_coherent < h_noise,
        "INV-W4: entropia coerente {h_coherent} !< ruído {h_noise}"
    );
    assert!(
        (0.0..=1.0).contains(&h_coherent) && (0.0..=1.0).contains(&h_noise),
        "entropias fora de [0,1]: {h_coherent} / {h_noise}"
    );
    assert!(h_noise > 0.5, "ruído deveria ter entropia alta: {h_noise}");
    assert!(
        h_coherent < 0.35,
        "senoide deveria ter entropia baixa: {h_coherent}"
    );

    // Sinal nulo e sinal constante (DC puro) => entropia 0.
    let zeros = Array2::<f32>::zeros((4, 128));
    assert_eq!(block.compute_spectral_entropy(zeros.view()).unwrap(), 0.0);
    let dc = Array2::<f32>::from_shape_fn((4, 128), |_| 0.25);
    let h_dc = block.compute_spectral_entropy(dc.view()).unwrap();
    assert!(h_dc < 1e-6, "H(DC puro) = {h_dc} (esperado ~0)");
}

// ---------------------------------------------------------------------------
// INV-W5 — filtro de banda: entropia do espectro transferido <= cru
// ---------------------------------------------------------------------------

#[test]
fn inv_w5_filtro_de_banda_reduz_entropia() {
    for seed in [1u64, 2, 3, 4, 5, 0xDEAD] {
        for &l in &[128usize, 256, 512] {
            let mut block = WaveInterferenceBlock::new(4, 8, 32, seed);
            let u = noise_field(4, l, seed ^ 0x99);

            let raw = block
                .compute_spectral_entropy(u.view())
                .expect("entropia crua");
            block.forward(u.view()).expect("forward");
            let wave = block.compute_wave_entropy();

            assert!(
                wave <= raw,
                "INV-W5 (seed {seed}, L {l}): wave {wave} > raw {raw}"
            );
            assert!(
                (0.0..=1.0).contains(&wave),
                "entropia de onda fora de [0,1]: {wave}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// wave_spectrum / acessores
// ---------------------------------------------------------------------------

#[test]
fn wave_spectrum_shape_e_conteudo() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 17);
    assert_eq!(block.modes(), 32);
    assert_eq!(block.c_in(), 4);
    assert_eq!(block.c_out(), 8);

    // Antes de qualquer forward o espectro é nulo.
    assert_eq!(block.wave_spectrum().dim(), (8, 32));
    assert!(block
        .wave_spectrum()
        .iter()
        .all(|z| z.re == 0.0 && z.im == 0.0));
    assert_eq!(block.compute_wave_entropy(), 0.0);

    block
        .forward(noise_field(4, 128, 5).view())
        .expect("forward");

    assert_eq!(block.wave_spectrum().dim(), (8, 32));
    assert!(block
        .wave_spectrum()
        .iter()
        .any(|z| z.re != 0.0 || z.im != 0.0));
    let wave = block.compute_wave_entropy();
    assert!((0.0..=1.0).contains(&wave), "wave entropy {wave}");
}

#[test]
fn espectro_transferido_independe_da_resolucao_em_forma() {
    // M4 lê spec[[o,k]] com k < modes; a view precisa ter sempre [C_out, modes].
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 3);
    for &l in &[128usize, 512, 2048] {
        block
            .forward(noise_field(4, l, l as u64).view())
            .expect("forward");
        let spec = block.wave_spectrum();
        assert_eq!(spec.dim(), (8, 32));
        for o in 0..8 {
            for k in 0..block.modes() {
                assert!(spec[[o, k]].re.is_finite() && spec[[o, k]].im.is_finite());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shapes inválidos / configuração degenerada
// ---------------------------------------------------------------------------

#[test]
fn shapes_invalidos_retornam_erro() {
    let mut block = WaveInterferenceBlock::new(4, 8, 32, 1);

    // Entrada com canais errados.
    let wrong_in = Array2::<f32>::zeros((3, 128));
    assert!(block.forward(wrong_in.view()).is_err());
    assert!(block.compute_spectral_entropy(wrong_in.view()).is_err());

    // Saída com canais errados e com L errado.
    let u = Array2::<f32>::zeros((4, 128));
    let mut wrong_c = Array2::<f32>::zeros((7, 128));
    assert!(block.forward_into(u.view(), &mut wrong_c).is_err());
    let mut wrong_l = Array2::<f32>::zeros((8, 64));
    assert!(block.forward_into(u.view(), &mut wrong_l).is_err());

    // Lote com canais errados.
    let wrong_batch = Array3::<f32>::zeros((2, 3, 128));
    assert!(block.forward_batched(wrong_batch.view()).is_err());

    // L == 0 é rejeitado (não há espectro definido).
    let empty = Array2::<f32>::zeros((4, 0));
    assert!(block.forward(empty.view()).is_err());
    assert!(block.compute_spectral_entropy(empty.view()).is_err());

    // Configuração degenerada não entra em pânico: devolve erro.
    let mut no_in = WaveInterferenceBlock::new(0, 8, 32, 1);
    assert!(no_in.forward(Array2::<f32>::zeros((0, 8)).view()).is_err());
    let mut no_modes = WaveInterferenceBlock::new(4, 8, 0, 1);
    assert!(no_modes.forward(u.view()).is_err());
    assert_eq!(no_modes.compute_wave_entropy(), 0.0);
}
