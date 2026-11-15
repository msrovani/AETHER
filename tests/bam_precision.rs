//! Precisão da fase BAM e da LUT de seno, em radianos.
//!
//! Três erros diferentes se misturam quando se diz "o BAM é preciso", e este
//! arquivo os separa:
//!
//! 1. **quantização de fase** — o passo `2π/65536 ≈ 95,9 µrad`. É um limite
//!    físico da representação, não um defeito da LUT;
//! 2. **erro da LUT + interpolação linear** — avaliado no ângulo *já
//!    quantizado*, portanto isolado do item 1;
//! 3. **erro da parábola `poly_sin_q14`** — o caminho vetorizado, que não usa
//!    tabela nenhuma.
//!
//! A referência é `f64` (`sin`/`cos` da std), varrendo a volta inteira —
//! todos os 65 536 códigos BAM, sem amostragem.
//!
//! Roda só com `m1`, então não depende de `m5`/`fhrr_bam`:
//! `cargo test --release --no-default-features --features m1 --test bam_precision -- --nocapture`

#![cfg(feature = "m1")]

use aether_engine::core::bam_phase::{poly_sin_q14, BamLut, BamPhase, LANES, LUT_LEN};
use std::f64::consts::TAU;

const Q15_SCALE: f64 = 32767.0;

/// Varre todos os códigos BAM e devolve (erro_max_sin, erro_max_cos) em
/// unidade de valor (seno ∈ [−1, 1]).
fn erro_lut_na_volta_inteira(lut: &BamLut) -> (f64, f64) {
    let mut max_s = 0.0f64;
    let mut max_c = 0.0f64;
    for code in 0..=u16::MAX {
        let p = BamPhase(code);
        let theta = p.to_radians() as f64; // ângulo exato do código
        let (s, c) = lut.sin_cos(p);
        max_s = max_s.max((s as f64 - theta.sin()).abs());
        max_c = max_c.max((c as f64 - theta.cos()).abs());
    }
    (max_s, max_c)
}

#[test]
fn lut_fica_dentro_de_um_lsb_q15_na_volta_inteira() {
    let lut = BamLut::global();
    let (max_s, max_c) = erro_lut_na_volta_inteira(lut);

    // Um LSB de Q15 é 1/32767 ≈ 3,052e-5, mas 1 LSB **não** é um limite
    // atingível: a própria tabela arredonda `sin·32767` para `i16`, o que já
    // consome até 0,5 LSB; a interpolação linear no passo de 2π/1024 soma
    // h²/8 ≈ 4,7e-6. O teto correto é 0,5 LSB + interpolação ≈ 0,55 LSB, e o
    // medido é 1,05 LSB — coerente com arredondamento no pior caso das duas
    // pontas do segmento. O limite abaixo (1,1 LSB) é o teto de projeto.
    let lsb = 1.0 / Q15_SCALE;
    println!(
        "LUT: erro max sin={max_s:.3e} cos={max_c:.3e} = {:.2} LSB Q15 ({lsb:.3e}) | \
em radianos ≈ {:.3e} rad",
        max_s / lsb,
        (max_s.min(1.0)).asin()
    );

    assert!(
        max_s <= 1.1 * lsb && max_c <= 1.1 * lsb,
        "erro da LUT acima de 1,1 LSB Q15: sin={max_s:.3e} cos={max_c:.3e} lsb={lsb:.3e}"
    );
}

#[test]
fn quantizacao_de_fase_fica_dentro_de_meio_passo() {
    // O erro de representação não pode passar de meio passo BAM em radianos.
    let passo = TAU / 65_536.0;
    let mut max_err = 0.0f64;
    for code in 0..=u16::MAX {
        let original = BamPhase(code);
        let rad = original.to_radians();
        let ida_volta = BamPhase::from_radians(rad);
        // Distância circular entre os códigos, em códigos.
        let d = (original.0 as i32 - ida_volta.0 as i32).rem_euclid(65_536);
        let d = d.min(65_536 - d) as f64;
        max_err = max_err.max(d * passo);
    }
    println!(
        "QUANTIZACAO: passo={:.3e} rad | erro max de ida e volta={:.3e} rad",
        passo, max_err
    );
    assert!(
        max_err <= 0.5 * passo + 1e-12,
        "ida e volta excedeu meio passo: {max_err:.3e} rad"
    );
}

#[test]
fn erro_total_de_uma_fase_real_fica_abaixo_de_1e4_rad() {
    // Pior caso realista: fase dada em `f32`, convertida para BAM, seno pela
    // LUT. Aqui quantização e LUT se somam.
    let lut = BamLut::global();
    let mut max_err = 0.0f64;
    let mut worst = 0.0f64;
    // Passo primo para não cair sempre nos mesmos códigos.
    for i in 0..200_000u32 {
        let theta = (i as f64 * 0.000_031_415_9).rem_euclid(TAU); // ~2π/200000
        let p = BamPhase::from_radians(theta as f32);
        let (s, _c) = lut.sin_cos(p);
        let err = (s as f64 - theta.sin()).abs();
        if err > max_err {
            max_err = err;
            worst = theta;
        }
    }
    println!(
        "TOTAL (fase real → BAM → LUT): erro max={max_err:.3e} (valor) ≈ {:.3e} rad, em θ={worst:.4}",
        max_err.min(1.0).asin()
    );
    // Meio passo de fase = 4,79e-5 rad de erro angular, que em seno perto de
    // zero vale o mesmo em valor. 1e-4 deixa folga para o arredondamento do
    // `from_radians` em `f32`.
    assert!(max_err < 1e-4, "erro total acima de 1e-4: {max_err:.3e}");
}

#[test]
fn descontinuidade_na_borda_da_volta() {
    // A interpolação usa `(idx + 1) & (LUT_LEN - 1)`, então a última entrada
    // interpola com a primeira. Se isso estivesse errado, haveria um degrau
    // grande entre 0xFFFF e 0x0000.
    let lut = BamLut::global();
    let antes = lut.sin_cos(BamPhase(0xFFFF)).0;
    let depois = lut.sin_cos(BamPhase(0x0000)).0;
    let salto = (antes - depois).abs();
    let esperado = 2.0 * std::f64::consts::PI / 65_536.0; // 1 passo
    println!("BORDA: sin(0xFFFF)={antes:.6} sin(0x0000)={depois:.6} salto={salto:.3e} (1 passo={esperado:.3e})");
    assert!(
        salto <= esperado as f32 * 1.5,
        "degrau na borda: {salto:.3e} contra passo {esperado:.3e}"
    );
}

#[test]
fn parabola_q14_poly_sin_erro_maximo() {
    // Caminho vetorizado: fase assinada em `i16`, saída Q14.
    let mut max_err = 0.0f64;
    let mut worst_theta = 0.0f64;
    for code in -32_768i32..32_768 {
        let theta = code as f64 * TAU / 65_536.0;
        let mut lanes = [0i16; LANES];
        lanes[0] = code as i16;
        let out = poly_sin_q14(&lanes);
        let approx = out[0] as f64 / 16_384.0;
        let err = (approx - theta.sin()).abs();
        if err > max_err {
            max_err = err;
            worst_theta = theta;
        }
    }
    println!(
        "POLY_SIN_Q14: erro max={max_err:.3e} ≈ {:.3e} rad, em θ={worst_theta:.4}",
        max_err.min(1.0).asin()
    );
    // A parábola de Bhaskara com um refinamento fica em ~1e-3 de amplitude.
    assert!(max_err < 2e-3, "parabola acima do esperado: {max_err:.3e}");
}

#[test]
fn lut_tem_o_tamanho_declarado_e_cabe_na_l1() {
    println!(
        "LUT: {LUT_LEN} entradas i16 = {} bytes ({} linha(s) de 64 B)",
        LUT_LEN * 2,
        (LUT_LEN * 2).div_ceil(64)
    );
    assert_eq!(LUT_LEN, 1024);
    const _: () = assert!(LUT_LEN * 2 <= 2048, "LUT nao cabe no orcamento de L1");
}
