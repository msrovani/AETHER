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
const QUARTER: u16 = 0x4000;
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
    lanes: [u16; D],
    visible: usize,
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
    phases: [u16; D],
}

impl<const D: usize> BamHolographicVector<D> {
    const VALID: () = assert!(D > 0 && D % LANES == 0, "D deve ser multiplo de 16");

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

/// Valor armazenado: fases + `cos/sin` pré-computados em Q12.
#[derive(Clone, Debug)]
#[repr(C, align(32))]
struct StoredValue<const D: usize> {
    cos: [i16; D],
    sin: [i16; D],
}

/// Traço quantizado (Q arbitrária comum a todos os traços).
#[derive(Clone, Debug)]
#[repr(C, align(32))]
struct QuantTrace<const D: usize> {
    re: [i16; D],
    im: [i16; D],
}

/// Resultado escalar de [`BamHolographicStore::query_into`] (sem alocação).
#[derive(Clone, Copy, Debug)]
pub struct BamHit {
    /// Fato eleito pela *cleanup memory* do papel.
    pub fact_index: usize,
    /// Valor com maior similaridade.
    pub value_index: usize,
    /// Similaridade do valor vencedor (≈ 1 em recuperação perfeita).
    pub similarity: f32,
    /// Margem para o segundo valor.
    pub margin: f32,
    /// Similaridade do papel da pista com o fato eleito.
    pub role_similarity: f32,
}

/// Workspace pré-alocado de [`BamHolographicStore::query_into`].
#[derive(Clone, Debug)]
pub struct BamScratch<const D: usize> {
    z_re: [i16; D],
    z_im: [i16; D],
    sims: Vec<f32>,
    n_sims: usize,
}

impl<const D: usize> BamScratch<D> {
    /// Similaridades com todos os valores da última consulta.
    pub fn sims(&self) -> &[f32] {
        &self.sims[..self.n_sims]
    }
}

/// Memória holográfica BAM (`entidade ⊗ relação ⊗ valor`, `K` traços).
#[derive(Debug)]
pub struct BamHolographicStore<const D: usize> {
    traces: usize,
    /// Acumuladores `K × D` em Q15 (construção).
    acc_re: Vec<i32>,
    acc_im: Vec<i32>,
    /// Traços quantizados para a consulta.
    quant: Vec<QuantTrace<D>>,
    /// Deslocamento de quantização (`acc >> shift`).
    shift: u32,
    roles: Vec<BamHolographicVector<D>>,
    values: Vec<StoredValue<D>>,
    /// Fase aleatória `ψ` por (fato, traço), em BAM.
    psi: Vec<u16>,
    phase_rng: u64,
}

impl<const D: usize> BamHolographicStore<D> {
    /// Cria o store com `traces ∈ [1, 15]` traços randomizados.
    pub fn new(traces: usize) -> Result<Self> {
        let () = BamHolographicVector::<D>::VALID;
        if traces == 0 || traces > MAX_TRACES {
            return Err(AetherError::Config(format!(
                "BAM: traces deve estar em 1..={MAX_TRACES}"
            )));
        }
        Ok(Self {
            traces,
            acc_re: vec![0; traces * D],
            acc_im: vec![0; traces * D],
            quant: vec![
                QuantTrace {
                    re: [0; D],
                    im: [0; D]
                };
                traces
            ],
            shift: 0,
            roles: Vec::new(),
            values: Vec::new(),
            psi: Vec::new(),
            phase_rng: 0xBA3_F00D ^ ((D as u64) << 20) ^ traces as u64,
        })
    }

    /// Número de fatos.
    pub fn len(&self) -> usize {
        self.roles.len()
    }

    /// `true` se vazio.
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    /// Número de traços `K`.
    pub fn traces(&self) -> usize {
        self.traces
    }

    /// Bytes ocupados pelos buffers de consulta (papéis, valores, traços).
    pub fn memory_bytes(&self) -> usize {
        self.roles.len() * std::mem::size_of::<BamHolographicVector<D>>()
            + self.values.len() * std::mem::size_of::<StoredValue<D>>()
            + self.quant.len() * std::mem::size_of::<QuantTrace<D>>()
            + self.psi.len() * 2
    }

    /// Grava `e ⊙ r ⊙ v` nos `K` traços (cada um com sua fase `ψ`).
    pub fn add_fact(
        &mut self,
        entity: &BamHolographicVector<D>,
        relation: &BamHolographicVector<D>,
        value: &BamHolographicVector<D>,
    ) -> usize {
        let lut = BamLut::global();
        let role = entity.bind(relation);
        let theta = role.bind(value);
        for t in 0..self.traces {
            let psi = if t == 0 {
                0u16
            } else {
                (splitmix(&mut self.phase_rng) >> 48) as u16
            };
            self.psi.push(psi);
            let base = t * D;
            for j in 0..D {
                let (s, c) = lut.sin_cos_q15(BamPhase(theta.phases[j].wrapping_add(psi)));
                self.acc_re[base + j] += c as i32;
                self.acc_im[base + j] += s as i32;
            }
        }
        // Valor: cos/sin em Q12 para o produto interno `pmaddwd`.
        let mut sv = StoredValue {
            cos: [0; D],
            sin: [0; D],
        };
        for j in 0..D {
            let (s, c) = lut.sin_cos_q15(BamPhase(value.phases[j]));
            sv.cos[j] = ((c as f32) * (Q12_ONE / 32767.0)).round() as i16;
            sv.sin[j] = ((s as f32) * (Q12_ONE / 32767.0)).round() as i16;
        }
        self.roles.push(role);
        self.values.push(sv);
        self.requantize();
        self.roles.len() - 1
    }

    /// Requantiza os acumuladores para `i16` com folga (`|M| ≤ 4095`).
    fn requantize(&mut self) {
        let max = self
            .acc_re
            .iter()
            .chain(self.acc_im.iter())
            .map(|v| v.unsigned_abs())
            .max()
            .unwrap_or(0);
        let bits = 32 - max.leading_zeros();
        self.shift = bits.saturating_sub(12);
        for t in 0..self.traces {
            let base = t * D;
            for j in 0..D {
                self.quant[t].re[j] = (self.acc_re[base + j] >> self.shift) as i16;
                self.quant[t].im[j] = (self.acc_im[base + j] >> self.shift) as i16;
            }
        }
    }

    /// Cria o workspace (aloca **uma vez**).
    pub fn scratch(&self, max_values: usize) -> BamScratch<D> {
        BamScratch {
            z_re: [0; D],
            z_im: [0; D],
            sims: vec![0.0; max_values.max(self.values.len())],
            n_sims: 0,
        }
    }

    /// Consulta **zero-alocação**: *cleanup* do papel `e ⊙ r` (mascarado),
    /// *unbinding* coerente dos `K` traços e similaridade com os valores.
    pub fn query_into(
        &self,
        entity_cue: &BamHolographicVector<D>,
        relation_cue: &BamHolographicVector<D>,
        mask: &BamMask<D>,
        sc: &mut BamScratch<D>,
    ) -> Result<BamHit> {
        if self.is_empty() {
            return Err(AetherError::Config("BAM: consulta em memoria vazia".into()));
        }
        if sc.sims.len() < self.values.len() {
            return Err(AetherError::shape([self.values.len()], [sc.sims.len()]));
        }
        let cue = entity_cue.bind(relation_cue);

        // 1. Cleanup memory do papel (fase pura, 16 fasores/lote).
        let mut best_k = 0usize;
        let mut best_s = i32::MIN;
        for (k, role) in self.roles.iter().enumerate() {
            let s = cos_sum_q14(&cue.phases, &role.phases, Some(&mask.lanes));
            if s > best_s {
                best_s = s;
                best_k = k;
            }
        }
        let role_similarity = if mask.visible == 0 {
            0.0
        } else {
            best_s as f32 / (Q14_ONE as f32 * mask.visible as f32)
        };

        // 2. Unbinding: z = Σ_t M_t · e^{-i(role_k + ψ_{k,t})}.
        sc.z_re.fill(0);
        sc.z_im.fill(0);
        let role = &self.roles[best_k].phases;
        let psi_base = best_k * self.traces;
        for t in 0..self.traces {
            let psi = self.psi[psi_base + t];
            let tr = &self.quant[t];
            for ((((c_role, c_re), c_im), z_re), z_im) in role
                .chunks_exact(LANES)
                .zip(tr.re.chunks_exact(LANES))
                .zip(tr.im.chunks_exact(LANES))
                .zip(sc.z_re.chunks_exact_mut(LANES))
                .zip(sc.z_im.chunks_exact_mut(LANES))
            {
                let mut sa = [0i16; LANES];
                let mut ca = [0i16; LANES];
                for l in 0..LANES {
                    let phi = c_role[l].wrapping_add(psi);
                    sa[l] = phi as i16;
                    ca[l] = phi.wrapping_add(QUARTER) as i16;
                }
                let s = poly_sin_q14(&sa);
                let c = poly_sin_q14(&ca);
                for l in 0..LANES {
                    let (mr, mi) = (c_re[l] as i32, c_im[l] as i32);
                    let (cc, ss) = (c[l] as i32, s[l] as i32);
                    z_re[l] += (((mr * cc) >> 16) + ((mi * ss) >> 16)) as i16;
                    z_im[l] += (((mi * cc) >> 16) - ((mr * ss) >> 16)) as i16;
                }
            }
        }
        // z ≤ 2^15 ⇒ z>>4 mantém os produtos com Q12 dentro de i32.
        for (zr, zi) in sc.z_re.iter_mut().zip(sc.z_im.iter_mut()) {
            *zr >>= 4;
            *zi >>= 4;
        }

        // 3. Similaridade com todos os valores (`pmaddwd`).
        let unit = (32767i32 >> self.shift) as f32;
        let norm = 1.0 / (D as f32 * self.traces as f32 * (unit * 0.25 / 16.0) * Q12_ONE);
        let mut best_v = 0usize;
        let mut best_sim = f32::NEG_INFINITY;
        let mut second = f32::NEG_INFINITY;
        for (k, v) in self.values.iter().enumerate() {
            let s = dot_i16(&sc.z_re, &v.cos) + dot_i16(&sc.z_im, &v.sin);
            let sim = s as f32 * norm;
            sc.sims[k] = sim;
            if sim > best_sim {
                second = best_sim;
                best_sim = sim;
                best_v = k;
            } else if sim > second {
                second = sim;
            }
        }
        sc.n_sims = self.values.len();
        let margin = if sc.n_sims > 1 {
            best_sim - second
        } else {
            best_sim
        };
        Ok(BamHit {
            fact_index: best_k,
            value_index: best_v,
            similarity: best_sim,
            margin,
            role_similarity,
        })
    }
}

/// Produto interno `i16 × i16 → i32` (vira `vpmaddwd`).
#[inline]
fn dot_i16(a: &[i16], b: &[i16]) -> i32 {
    let mut acc = [0i32; LANES];
    for (ca, cb) in a.chunks_exact(LANES).zip(b.chunks_exact(LANES)) {
        for l in 0..LANES {
            acc[l] += ca[l] as i32 * cb[l] as i32;
        }
    }
    acc.iter().sum()
}

/// SplitMix64 local (fases `ψ` determinísticas do store).
#[inline]
fn splitmix(state: &mut u64) -> u64 {
    crate::core::silicon_entropy::splitmix64(state)
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

    #[test]
    fn recall_holografico_com_oclusao() {
        const D: usize = 256;
        const FACTS: usize = 50;
        let mut rng = SiliconEntropy::from_seed(0xFAC7);
        let mut store = BamHolographicStore::<D>::new(6).expect("store");
        let mut atoms = Vec::new();
        for _ in 0..FACTS {
            let e = BamHolographicVector::<D>::random(&mut rng);
            let r = BamHolographicVector::<D>::random(&mut rng);
            let v = BamHolographicVector::<D>::random(&mut rng);
            store.add_fact(&e, &r, &v);
            atoms.push((e, r));
        }
        let mut sc = store.scratch(FACTS);
        let mut mask_b = vec![false; D];
        for (i, m) in mask_b.iter_mut().enumerate() {
            *m = i % 5 < 2; // 40 % visíveis
        }
        let mask = BamMask::<D>::from_bools(&mask_b).expect("mask");
        let mut hits = 0;
        for k in 0..FACTS {
            let hit = store
                .query_into(&atoms[k].0, &atoms[k].1, &mask, &mut sc)
                .expect("query");
            if hit.value_index == k {
                hits += 1;
            }
        }
        assert_eq!(hits, FACTS, "recall@1 deve ser perfeito no cenario limpo");
    }
}
