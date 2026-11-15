//! Aritmética angular binária (BAM) e hologramas de fase em `u16`.
//!
//! **BAM** (*Binary Angular Measurement*): a fase `θ ∈ [0, 2π)` é um inteiro
//! `u16 ∈ [0, 65536)`. Somar/subtrair fases é `wrapping_add`/`wrapping_sub` —
//! o módulo `2π` é **grátis** (nenhum `rem_euclid`, nenhum `atan2`).
//!
//! ## Componentes
//!
//! * [`BamPhase`] — `u16` com aritmética modular.
//! * [`BamLut`] — tabela de seno `1024 × i16` (2048 B, cabe na L1d) com
//!   interpolação linear: [`BamLut::sin_cos`] (referência, erro ≈ 3·10⁻⁵).
//! * [`poly_sin_q14`] — seno **100 % vetorizável** (parábola de Bhaskara
//!   refinada, só `i16`/`pmulhw`, sem *gather*): erro ≤ 2·10⁻³; é o motor de
//!   similaridade em lotes de [`LANES`] = 16 fasores por iteração (um `ymm`
//!   AVX2 de `u16`; em SSE4.2 são dois `xmm`).
//! * [`BamHolographicVector`]`<D>` — `D` fasores (`D = 256` ⇒ **512 bytes**)
//!   com `bind`/`unbind`/`similarity` por produto de Hadamard em fase.
//! * [`BamHolographicStore`] — memória holográfica (FHRR, Plate 2003) em BAM:
//!   `K` traços com fase aleatória por (fato, traço), *cleanup memory* do
//!   papel, *unbinding* coerente e similaridade com os valores — a mesma
//!   semântica do `FhrrStore` em `f32`, sem `sin_cos` e sem alocação na
//!   consulta ([`BamHolographicStore::query_into`]).
//!
//! ## Vetorização
//!
//! Todos os *kernels* trabalham em `chunks_exact(16)` sobre `u16`/`i16` com
//! laços de trip-count constante; o LLVM os emite como `vpsubw`/`vpmulhw`/
//! `vpaddw`/`vpmaddwd`. Nenhum `unsafe`, nenhum intrínseco: o ganho vem do
//! layout (`u16` contíguo, alinhado a 32 B) e de eliminar a trigonometria.
//! Compile com `-C target-cpu=native` (ver `.cargo/config.toml`) para AVX2.
//!
//! ## Precisão (limites honestos)
//!
//! * Quantização de fase: `2π/65536 ≈ 9,6·10⁻⁵ rad`.
//! * `poly_sin_q14`: erro máximo ≲ 2·10⁻³ (testado). Suficiente para ranking
//!   de similaridade (o piso de ruído de superposição é `≈ 1/√(D·K)`).

#![allow(clippy::needless_range_loop)]

use std::f32::consts::TAU;
use std::sync::OnceLock;

use crate::core::silicon_entropy::SiliconEntropy;
use crate::error::{AetherError, Result};

/// Fasores processados por lote (um registrador `ymm` de `u16`).
pub const LANES: usize = 16;
/// Entradas da LUT de seno (`2^10`).
pub const LUT_LEN: usize = 1024;
/// `16 − log2(LUT_LEN)`: bits de interpolação.
const LUT_SHIFT: u32 = 6;
/// Quarto de volta (`π/2`) em BAM.
pub(crate) const QUARTER: u16 = 0x4000;
/// Máximo de traços suportados (acumulador `i16` do *unbinding*).
pub const MAX_TRACES: usize = 15;
/// Escala Q do seno polinomial (`1.0 = 16384`).
pub const Q14_ONE: i32 = 16_384;
/// Escala Q das tabelas `cos/sin` dos valores (`1.0 = 4096`).
const Q12_ONE: f32 = 4096.0;

/// Fase em BAM: `[0, 2π) ↔ [0, 65536)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct BamPhase(pub u16);

impl BamPhase {
    /// Fase zero.
    pub const ZERO: Self = Self(0);
    /// `π/2`.
    pub const QUARTER_TURN: Self = Self(QUARTER);
    /// `π`.
    pub const HALF_TURN: Self = Self(0x8000);

    /// Converte radianos (qualquer sinal/magnitude) para BAM. Não finito ⇒ 0.
    #[inline]
    pub fn from_radians(rad: f32) -> Self {
        if !rad.is_finite() {
            return Self::ZERO;
        }
        let turns = rad * (1.0 / TAU);
        let frac = turns - turns.floor();
        Self(((frac * 65_536.0 + 0.5) as u32 & 0xFFFF) as u16)
    }

    /// Radianos em `[0, 2π)`.
    #[inline]
    pub fn to_radians(self) -> f32 {
        self.0 as f32 * (TAU / 65_536.0)
    }

    /// Radianos em `[−π, π)`.
    #[inline]
    pub fn to_signed_radians(self) -> f32 {
        (self.0 as i16) as f32 * (TAU / 65_536.0)
    }

    /// Soma modular `2π`.
    #[inline(always)]
    pub fn wrapping_add(self, o: Self) -> Self {
        Self(self.0.wrapping_add(o.0))
    }

    /// Subtração modular `2π`.
    #[inline(always)]
    pub fn wrapping_sub(self, o: Self) -> Self {
        Self(self.0.wrapping_sub(o.0))
    }

    /// Fase oposta `−θ`.
    #[inline(always)]
    pub fn wrapping_neg(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

impl std::ops::Add for BamPhase {
    type Output = Self;
    #[inline(always)]
    fn add(self, o: Self) -> Self {
        self.wrapping_add(o)
    }
}

impl std::ops::Sub for BamPhase {
    type Output = Self;
    #[inline(always)]
    fn sub(self, o: Self) -> Self {
        self.wrapping_sub(o)
    }
}

impl std::ops::Neg for BamPhase {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        self.wrapping_neg()
    }
}

/// Tabela de seno `1024 × i16` (Q15), alinhada a uma linha de cache.
#[derive(Clone, Debug)]
#[repr(align(64))]
pub struct BamLut {
    sin: [i16; LUT_LEN],
}

impl BamLut {
    /// Constrói a tabela (`sin(2π·i/1024)·32767`, arredondado).
    pub fn new() -> Self {
        let mut sin = [0i16; LUT_LEN];
        for (i, s) in sin.iter_mut().enumerate() {
            let a = (i as f64) * std::f64::consts::TAU / LUT_LEN as f64;
            *s = (a.sin() * 32767.0).round() as i16;
        }
        Self { sin }
    }

    /// Instância compartilhada e imutável (tabela constante, não é estado).
    pub fn global() -> &'static Self {
        static LUT: OnceLock<BamLut> = OnceLock::new();
        LUT.get_or_init(BamLut::new)
    }

    /// Interpola linearmente a tabela na fase BAM `phase`.
    #[inline(always)]
    fn interp(&self, phase: u16) -> i32 {
        let idx = (phase >> LUT_SHIFT) as usize;
        let frac = (phase & ((1 << LUT_SHIFT) - 1)) as i32;
        let s0 = self.sin[idx & (LUT_LEN - 1)] as i32;
        let s1 = self.sin[(idx + 1) & (LUT_LEN - 1)] as i32;
        s0 + (((s1 - s0) * frac + (1 << (LUT_SHIFT - 1))) >> LUT_SHIFT)
    }

    /// `(sin θ, cos θ)` em Q15 com interpolação linear.
    #[inline]
    pub fn sin_cos_q15(&self, p: BamPhase) -> (i16, i16) {
        let s = self.interp(p.0);
        let c = self.interp(p.0.wrapping_add(QUARTER));
        (s as i16, c as i16)
    }

    /// `(sin θ, cos θ)` em `f32`.
    #[inline]
    pub fn sin_cos(&self, p: BamPhase) -> (f32, f32) {
        let (s, c) = self.sin_cos_q15(p);
        (s as f32 * (1.0 / 32767.0), c as f32 * (1.0 / 32767.0))
    }
}

impl Default for BamLut {
    fn default() -> Self {
        Self::new()
    }
}

/// `sin` vetorizável em Q14 de 16 fasores: entrada = fase **assinada**
/// (`u16 as i16`, `±32768 ↔ ±π`), saída `sin·16384`.
///
/// Parábola de Bhaskara `4x(1−|x|)` com refinamento
/// `y = p(0,775 + 0,225|p|)`; apenas `i16` e multiplicações altas.
#[inline(always)]
pub fn poly_sin_q14(s: &[i16; LANES]) -> [i16; LANES] {
    let mut y = [0i16; LANES];
    for l in 0..LANES {
        let x = s[l];
        let ax = x ^ (x >> 15); // |x| (complemento de 1; erro de 1 LSB)
        let t = 32767 - ax;
        let h = ((x as i32 * t as i32) >> 16) as i16; // x(1−|x|) em Q14
        let p = h << 2; // 4·x(1−|x|) em Q14
        let ap = p ^ (p >> 15);
        let f = 12_698 + ((ap as i32 * 14_746) >> 16) as i16;
        y[l] = (((p as i32 * f as i32) >> 16) as i16) << 2;
    }
    y
}

/// Máscara de fasores visíveis (`0xFFFF` = visível, `0` = oculto).
#[derive(Clone, Copy, Debug)]
#[repr(C, align(32))]
pub struct BamMask<const D: usize> {
    pub(crate) lanes: [u16; D],
    pub(crate) visible: usize,
}

impl<const D: usize> BamMask<D> {
    /// Todos visíveis.
    pub fn all() -> Self {
        Self {
            lanes: [0xFFFF; D],
            visible: D,
        }
    }

    /// A partir de `bool`s (`true` = visível). `len` deve ser `D`.
    pub fn from_bools(mask: &[bool]) -> Result<Self> {
        if mask.len() != D {
            return Err(AetherError::shape([D], [mask.len()]));
        }
        let mut lanes = [0u16; D];
        let mut visible = 0usize;
        for (l, &m) in lanes.iter_mut().zip(mask) {
            if m {
                *l = 0xFFFF;
                visible += 1;
            }
        }
        Ok(Self { lanes, visible })
    }

    /// Fasores visíveis.
    pub fn visible(&self) -> usize {
        self.visible
    }
}

/// Hipervetor holográfico de `D` fasores BAM (`D % 16 == 0`).
///
/// `D = 256` ⇒ 512 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, align(32))]
pub struct BamHolographicVector<const D: usize> {
    pub(crate) phases: [u16; D],
}

impl<const D: usize> BamHolographicVector<D> {
    pub(crate) const VALID: () = assert!(D > 0 && D % LANES == 0, "D deve ser multiplo de 16");

    /// Vetor nulo (todas as fases em 0).
    pub fn zero() -> Self {
        let () = Self::VALID;
        Self { phases: [0; D] }
    }

    /// A partir das fases brutas.
    pub fn from_phases(phases: [u16; D]) -> Self {
        let () = Self::VALID;
        Self { phases }
    }

    /// Fases aleatórias uniformes (4 por `u64`).
    pub fn random(rng: &mut SiliconEntropy) -> Self {
        let mut v = Self::zero();
        for quad in v.phases.chunks_exact_mut(4) {
            let r = rng.next_u64();
            quad[0] = r as u16;
            quad[1] = (r >> 16) as u16;
            quad[2] = (r >> 32) as u16;
            quad[3] = (r >> 48) as u16;
        }
        v
    }

    /// A partir de radianos (`len == D`).
    pub fn from_radians(rad: &[f32]) -> Result<Self> {
        if rad.len() != D {
            return Err(AetherError::shape([D], [rad.len()]));
        }
        let mut v = Self::zero();
        for (p, &r) in v.phases.iter_mut().zip(rad) {
            *p = BamPhase::from_radians(r).0;
        }
        Ok(v)
    }

    /// Escreve as fases em radianos `[0, 2π)` (`len == D`).
    pub fn to_radians_into(&self, out: &mut [f32]) -> Result<()> {
        if out.len() != D {
            return Err(AetherError::shape([D], [out.len()]));
        }
        for (o, &p) in out.iter_mut().zip(&self.phases) {
            *o = BamPhase(p).to_radians();
        }
        Ok(())
    }

    /// Fases brutas.
    #[inline(always)]
    pub fn as_slice(&self) -> &[u16; D] {
        &self.phases
    }

    /// Fases brutas (mutável).
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [u16; D] {
        &mut self.phases
    }

    /// `self ⊙ other`: soma de fases, 16 fasores por lote.
    #[inline]
    pub fn bind(&self, other: &Self) -> Self {
        let mut out = *self;
        out.bind_assign(other);
        out
    }

    /// `self ⊙= other`.
    #[inline]
    pub fn bind_assign(&mut self, other: &Self) {
        for (a, b) in self
            .phases
            .chunks_exact_mut(LANES)
            .zip(other.phases.chunks_exact(LANES))
        {
            for l in 0..LANES {
                a[l] = a[l].wrapping_add(b[l]);
            }
        }
    }

    /// `self ⊙ conj(other)`: subtração de fases.
    #[inline]
    pub fn unbind(&self, other: &Self) -> Self {
        let mut out = *self;
        out.unbind_assign(other);
        out
    }

    /// `self ⊙= conj(other)`.
    #[inline]
    pub fn unbind_assign(&mut self, other: &Self) {
        for (a, b) in self
            .phases
            .chunks_exact_mut(LANES)
            .zip(other.phases.chunks_exact(LANES))
        {
            for l in 0..LANES {
                a[l] = a[l].wrapping_sub(b[l]);
            }
        }
    }

    /// Rotação global de fase.
    #[inline]
    pub fn rotate(&mut self, delta: BamPhase) {
        for p in self.phases.iter_mut() {
            *p = p.wrapping_add(delta.0);
        }
    }

    /// Similaridade `(1/D) Σ cos(θ_a − θ_b) ∈ [−1, 1]` (vetorizada).
    #[inline]
    pub fn similarity(&self, other: &Self) -> f32 {
        cos_sum_q14(&self.phases, &other.phases, None) as f32 / (Q14_ONE as f32 * D as f32)
    }

    /// Similaridade só nos fasores visíveis, normalizada por `visible`.
    #[inline]
    pub fn similarity_masked(&self, other: &Self, mask: &BamMask<D>) -> f32 {
        if mask.visible == 0 {
            return 0.0;
        }
        cos_sum_q14(&self.phases, &other.phases, Some(&mask.lanes)) as f32
            / (Q14_ONE as f32 * mask.visible as f32)
    }

    /// Similaridade de **referência** (LUT interpolada, escalar).
    pub fn similarity_exact(&self, other: &Self, lut: &BamLut) -> f32 {
        let mut acc = 0i64;
        for (&a, &b) in self.phases.iter().zip(&other.phases) {
            acc += lut.sin_cos_q15(BamPhase(a.wrapping_sub(b))).1 as i64;
        }
        acc as f32 / (32767.0 * D as f32)
    }
}

/// `Σ cos(a − b)` em Q14, em lotes de 16 (máscara opcional por AND).
#[inline]
fn cos_sum_q14(a: &[u16], b: &[u16], mask: Option<&[u16]>) -> i32 {
    let mut acc = [0i32; LANES];
    match mask {
        None => {
            for (ca, cb) in a.chunks_exact(LANES).zip(b.chunks_exact(LANES)) {
                let mut s = [0i16; LANES];
                for l in 0..LANES {
                    s[l] = ca[l].wrapping_sub(cb[l]).wrapping_add(QUARTER) as i16;
                }
                let c = poly_sin_q14(&s);
                for l in 0..LANES {
                    acc[l] += c[l] as i32;
                }
            }
        }
        Some(m) => {
            for ((ca, cb), cm) in a
                .chunks_exact(LANES)
                .zip(b.chunks_exact(LANES))
                .zip(m.chunks_exact(LANES))
            {
                let mut s = [0i16; LANES];
                for l in 0..LANES {
                    s[l] = ca[l].wrapping_sub(cb[l]).wrapping_add(QUARTER) as i16;
                }
                let c = poly_sin_q14(&s);
                for l in 0..LANES {
                    acc[l] += (c[l] & cm[l] as i16) as i32;
                }
            }
        }
    }
    acc.iter().sum()
}

pub use crate::core::bam_store::{
    BamHit, BamHolographicStore, BamScratch,
};

/// `Σ tri(a − b)` em Q14 — onda triangular `1 − 2|θ|/π` (≈ cos, só `sub`/`abs`).
///
/// Usada na *cleanup memory*: monotônica em `|θ|`, 4 operações por 16 fasores
/// e variância menor que o cosseno sob fase aleatória (`1/3` vs `1/2`).
#[inline]
pub(crate) fn tri_sum_q14(a: &[u16], b: &[u16], mask: &[u16]) -> i32 {
    let mut acc = [0i32; LANES];
    for ((ca, cb), cm) in a
        .chunks_exact(LANES)
        .zip(b.chunks_exact(LANES))
        .zip(mask.chunks_exact(LANES))
    {
        for l in 0..LANES {
            let s = ca[l].wrapping_sub(cb[l]) as i16;
            let ax = s ^ (s >> 15); // |s| (complemento de 1)
            let tri = (Q14_ONE as i16 - ax) & cm[l] as i16;
            acc[l] += tri as i32;
        }
    }
    acc.iter().sum()
}

/// Produto interno `i16 × i16 → i32` (vira `vpmaddwd`).
#[inline]
pub(crate) fn dot_i16(a: &[i16], b: &[i16]) -> i32 {
    let mut acc = [0i32; LANES];
    for (ca, cb) in a.chunks_exact(LANES).zip(b.chunks_exact(LANES)) {
        for l in 0..LANES {
            acc[l] += ca[l] as i32 * cb[l] as i32;
        }
    }
    acc.iter().sum()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_ocupa_2048_bytes() {
        assert_eq!(std::mem::size_of::<BamLut>(), 2048);
        assert_eq!(std::mem::size_of::<BamHolographicVector<256>>(), 512);
    }

    #[test]
    fn conversao_radianos_ida_e_volta() {
        for k in 0..2000 {
            let r = (k as f32) * 0.0123 - 7.0;
            let p = BamPhase::from_radians(r);
            let back = p.to_radians();
            let d = (back - r).rem_euclid(TAU);
            let err = d.min(TAU - d);
            assert!(err < 1.2e-4, "r={r} err={err}");
        }
        assert_eq!(BamPhase::from_radians(f32::NAN), BamPhase::ZERO);
    }

    #[test]
    fn aritmetica_modular() {
        let a = BamPhase(60_000);
        let b = BamPhase(10_000);
        assert_eq!((a + b).0, 4_464);
        assert_eq!((b - a).0, 15_536);
        assert_eq!((-b).0, 55_536);
    }

    #[test]
    fn lut_sin_cos_erro_pequeno() {
        let lut = BamLut::new();
        let mut max = 0.0f32;
        for p in (0..=u16::MAX).step_by(7) {
            let (s, c) = lut.sin_cos(BamPhase(p));
            let th = BamPhase(p).to_radians();
            max = max.max((s - th.sin()).abs()).max((c - th.cos()).abs());
        }
        assert!(max < 1e-4, "erro max {max}");
    }

    #[test]
    fn seno_polinomial_erro_abaixo_de_2e3() {
        let mut max = 0.0f32;
        let mut buf = [0i16; LANES];
        for base in (0..65_536usize).step_by(LANES) {
            for (l, b) in buf.iter_mut().enumerate() {
                *b = (base + l) as u16 as i16;
            }
            let y = poly_sin_q14(&buf);
            for l in 0..LANES {
                let th = BamPhase((base + l) as u16).to_signed_radians();
                max = max.max((y[l] as f32 / Q14_ONE as f32 - th.sin()).abs());
            }
        }
        assert!(max < 2e-3, "erro max {max}");
    }

    #[test]
    fn bind_unbind_inversos_e_similaridade() {
        let mut rng = SiliconEntropy::from_seed(11);
        let a = BamHolographicVector::<256>::random(&mut rng);
        let b = BamHolographicVector::<256>::random(&mut rng);
        assert_eq!(a.bind(&b).unbind(&b), a);
        assert!((a.similarity(&a) - 1.0).abs() < 3e-3);
        assert!(a.similarity(&b).abs() < 0.2);
        let lut = BamLut::global();
        assert!((a.similarity(&b) - a.similarity_exact(&b, lut)).abs() < 3e-3);
    }
}
