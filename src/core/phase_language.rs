//! E1 — *Phase-Language Head* (PLH) na borda: memória sharded + ressonador ACF.
//!
//! O E1b mediu a lei do acumulador holográfico: `M_max ≈ 0,4·D` fatos
//! (`D=160, K=6` ⇒ 64 fatos, `recall@1 ≥ 99%`). Empilhar 1.000 fatos numa única
//! superposição plana colapsa o `recall@1`. Este módulo monta a cabeça de
//! linguagem **em cima da lei**, não contra ela:
//!
//! 1. **Memória sharded por papel sintático** — 24 shards com `≤ 48` fatos cada
//!    (75% do teto medido). O shard de um fato é função determinística do par
//!    `(sujeito, verbo)`; cada shard é um acumulador de `K` traços em BAM
//!    (`u16`, `|z| = 1` exato) com fase `ψ` por `(fato, traço)`.
//! 2. **Binding não-comutativo com âncoras fixas** — `ρ^k` é um deslocamento
//!    cíclico de posições (permutação), não uma rotação de fase: por isso
//!    `ρ^a(x) ⊗ ρ^b(y) ≠ ρ^b(x) ⊗ ρ^a(y)`. O código de sentença é
//!    `Ψ = A_S⊗ρ^0(s) ⊕ A_V⊗ρ^1(v) ⊕ A_O⊗ρ^2(o) ⊕ A_M⊗ρ^3(m)`.
//! 3. **Ressonador ACF** (*Asymmetric Codebook Factorizer*) — fatora o valor
//!    entre os papéis por *cleanup* iterativo com projeção *re-encode* (átomo
//!    limpo): decisões na cópia **perturbada** do codebook (`±4% de π`,
//!    `ChaCha8Rng`), validação na cópia **limpa**.
//!
//! # O detalhe que decide o README
//!
//! O acumulador guarda uma **soma complexa** de `K` traços. A projeção
//! `θ = arg(z)` *por dimensão* antes do cleanup descarta a magnitude — que é
//! exatamente a evidência de coerência dos `K` traços. O cleanup do PLH usa o
//! produto interno complexo `Re⟨z, átomo⟩ / (D·K)`, como o `BamHolographicStore`
//! do E1b; só os **átomos** (entrada/saída do codebook) são estritamente
//! unitários. Medido: a projeção por dimensão levava `recall@1` a 0,17 e SNR a
//! 0,84σ; o cleanup complexo devolve `recall ≥ 0,99` e SNR > 5σ.
//!
//! # Invariantes de borda
//!
//! * `#![deny(unsafe_code)]` no crate; este módulo não usa `unsafe`.
//! * `≤ 48` fatos por acumulador — **nunca** há superposição plana > 48.
//! * O caminho de consulta (`query_into`) não aloca.
//! * Fasores de codebook com magnitude unitária estrita; o acumulador é
//!   complexo e a normalização por `K` é explícita.

#![allow(clippy::needless_range_loop)]

use std::f32::consts::{PI, TAU};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::core::bam_phase::{poly_sin_q14, BamHolographicVector, BamLut, BamPhase};
use crate::core::renormalization::RenormalizationHierarchy;
use crate::core::silicon_entropy::splitmix64;

/// Dimensão do espaço de fase (mesma do E1b).
pub const D: usize = 160;
/// Fasores por lote vetorizado (um `ymm` de `u16`).
const LANES: usize = 16;
/// Um quarto de volta em BAM.
const QUARTER: u16 = 0x4000;
/// Escala Q14 do seno polinomial (`1.0 = 16384`).
const Q14: f32 = 16_384.0;
/// Escala Q15 dos codebooks pré-computados (`1.0 = 32767`).
const Q15: f32 = 32_767.0;

/// Traços por acumulador. O bundle de 2 fasores por fato dobra os termos
/// interferentes; `K=17` mantém o SNR > 5σ com 43 fatos (86 termos) e ainda
/// cabe no orçamento de working set (485 KB < 500 KB).
pub const K_TRACES: usize = 17;
/// Shards (relações sintáticas endereçáveis).
pub const SHARDS: usize = 24;
/// Teto absoluto de fatos por acumulador (75% de `0,4·D = 64`).
pub const SHARD_CAP: usize = 48;
/// Fatos do gate de escala.
pub const N_FACTS: usize = 1000;
/// Vocabulário de entidades (sujeito **e** objeto — o mesmo codebook, para que
/// `A→B` e `B→A` possam ser comparados com os mesmos átomos).
pub const N_ENT: usize = 64;
/// Verbos (= relação).
pub const N_VERB: usize = 16;
/// Modificadores.
pub const N_MOD: usize = 8;
/// Jitter angular da cópia ACF do codebook (`±4% de π`).
pub const JITTER_FRAC: f32 = 0.04;
/// Passos máximos do ressonador de sentença (4 fatores).
pub const RES_ITERS: usize = 4;
/// Passos máximos do ressonador do predicado (2 fatores).
pub const PAIR_ITERS: usize = 2;
/// Piso de evidência para abstenção (escolhido *a priori*, não no teste).
pub const SIM_FLOOR: f32 = 0.55;
/// Piso de margem (1º − 2º) para abstenção.
pub const MARGIN_FLOOR: f32 = 0.08;

/// Papéis sintáticos na ordem de `ρ^k`.
pub const ROLE_S: usize = 0;
pub const ROLE_V: usize = 1;
pub const ROLE_O: usize = 2;
pub const ROLE_M: usize = 3;

/// Fase de um fasor (`u16`, `[0, 2π) ↔ [0, 65536)`).
pub type Phases = [u16; D];
/// Vetor holográfico BAM de `D` fasores unitários.
pub type BVec = BamHolographicVector<D>;

// ---------------------------------------------------------------------------
// Álgebra de fase: ρ (permutação), binding, similaridade, projeção unitária
// ---------------------------------------------------------------------------

/// `ρ^k(v)[j] = v[(j+k) mod D]` — deslocamento cíclico **de posições**.
///
/// É uma permutação, não uma rotação global: `ρ` quebra a comutatividade do
/// binding (`ρ^a(x) ⊗ ρ^b(y) ≠ ρ^b(x) ⊗ ρ^a(y)`) e não cancela no unbinding.
#[inline]
pub fn rho(v: &Phases, k: usize) -> Phases {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = v[(j + k) % D];
    }
    out
}

/// `ρ^{-k}` — a inversa exata da permutação acima.
#[inline]
pub fn unrho(v: &Phases, k: usize) -> Phases {
    let k = k % D;
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = v[(j + D - k) % D];
    }
    out
}

/// Binding FHRR: produto de Hadamard = soma de fases (`|z| = 1` preservado).
#[inline]
pub fn bind(a: &Phases, b: &Phases) -> Phases {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = a[j].wrapping_add(b[j]);
    }
    out
}

/// `(1/D)·Σ tri(θ_a − θ_b)` em Q14, com `tri(x) = 1 − 2|x|/π`.
#[inline]
pub fn tri_sim(a: &Phases, b: &Phases) -> f32 {
    let mut acc = [0i32; LANES];
    for (ca, cb) in a.chunks_exact(LANES).zip(b.chunks_exact(LANES)) {
        for l in 0..LANES {
            let s = ca[l].wrapping_sub(cb[l]) as i16;
            let ax = s ^ (s >> 15);
            acc[l] += (Q14 as i16 - ax) as i32;
        }
    }
    let sum: i32 = acc.iter().sum();
    sum as f32 / (Q14 * D as f32)
}

/// Projeta um vetor complexo de volta ao círculo unitário: `θ = arg(z)`.
#[inline]
pub fn project_unit(re: &[f32; D], im: &[f32; D]) -> Phases {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = BamPhase::from_radians(im[j].atan2(re[j])).0;
    }
    out
}

/// Fasor unitário `e^{iθ}` em `(re, im)` a partir de fases BAM.
#[inline]
pub fn unit_phasor(phases: &Phases) -> ([f32; D], [f32; D]) {
    let lut = BamLut::global();
    let mut re = [0.0f32; D];
    let mut im = [0.0f32; D];
    for j in 0..D {
        let (s, c) = lut.sin_cos(BamPhase(phases[j]));
        re[j] = c;
        im[j] = s;
    }
    (re, im)
}

// ---------------------------------------------------------------------------
// Codebooks Q15 + âncoras (cópia limpa e cópia ACF perturbada)
// ---------------------------------------------------------------------------

/// Codebook com a fase e os fasores Q15 pré-computados (evita `sin_cos` no
/// caminho quente).
#[derive(Clone, Debug)]
pub struct Q15Atom {
    pub re: [i16; D],
    pub im: [i16; D],
}

impl Q15Atom {
    fn from_phases(phases: &Phases) -> Self {
        let lut = BamLut::global();
        let mut re = [0i16; D];
        let mut im = [0i16; D];
        for j in 0..D {
            let (s, c) = lut.sin_cos_q15(BamPhase(phases[j]));
            re[j] = c;
            im[j] = s;
        }
        Self { re, im }
    }
}

/// Um vocabulário de papel: cópia limpa, cópia ACF e as duas em Q15.
#[derive(Clone, Debug)]
pub struct AtomBook {
    pub clean: Vec<BVec>,
    pub acf: Vec<BVec>,
    pub clean_q: Vec<Q15Atom>,
    pub acf_q: Vec<Q15Atom>,
}

impl AtomBook {
    fn random(n: usize, rng: &mut ChaCha8Rng, with_q15: bool) -> Self {
        let clean = random_codebook(n, rng);
        let acf = jitter_codebook(&clean, rng, JITTER_FRAC);
        let (clean_q, acf_q) = if with_q15 {
            (
                clean
                    .iter()
                    .map(|v| Q15Atom::from_phases(v.as_slice()))
                    .collect(),
                acf.iter()
                    .map(|v| Q15Atom::from_phases(v.as_slice()))
                    .collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        Self {
            clean,
            acf,
            clean_q,
            acf_q,
        }
    }

    fn memory_bytes(&self) -> usize {
        let one = std::mem::size_of::<BVec>();
        let q = std::mem::size_of::<Q15Atom>();
        self.clean.len() * one
            + self.acf.len() * one
            + self.clean_q.len() * q
            + self.acf_q.len() * q
    }
}

/// Vocabulários de papel. `ent` serve sujeito **e** objeto; `verb` é a relação;
/// `modu` é o modificador.
#[derive(Clone, Debug)]
pub struct Codebooks {
    pub ent: AtomBook,
    pub verb: AtomBook,
    pub modu: AtomBook,
}

impl Codebooks {
    pub fn build(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        // Só `ent` e `modu` participam do cleanup complexo; `verb` é usado no
        // codec de sentença (fase), então dispensa a tabela Q15.
        Self {
            ent: AtomBook::random(N_ENT, &mut rng, true),
            verb: AtomBook::random(N_VERB, &mut rng, false),
            modu: AtomBook::random(N_MOD, &mut rng, true),
        }
    }

    pub fn clean(&self, role: usize) -> &[BVec] {
        match role {
            ROLE_S | ROLE_O => &self.ent.clean,
            ROLE_V => &self.verb.clean,
            _ => &self.modu.clean,
        }
    }

    pub fn acf(&self, role: usize) -> &[BVec] {
        match role {
            ROLE_S | ROLE_O => &self.ent.acf,
            ROLE_V => &self.verb.acf,
            _ => &self.modu.acf,
        }
    }

    pub fn clean_q(&self, role: usize) -> &[Q15Atom] {
        match role {
            ROLE_S | ROLE_O => &self.ent.clean_q,
            ROLE_V => &self.verb.clean_q,
            _ => &self.modu.clean_q,
        }
    }

    pub fn acf_q(&self, role: usize) -> &[Q15Atom] {
        match role {
            ROLE_S | ROLE_O => &self.ent.acf_q,
            ROLE_V => &self.verb.acf_q,
            _ => &self.modu.acf_q,
        }
    }

    pub fn memory_bytes(&self) -> usize {
        self.ent.memory_bytes() + self.verb.memory_bytes() + self.modu.memory_bytes()
    }
}

fn random_codebook(n: usize, rng: &mut ChaCha8Rng) -> Vec<BVec> {
    (0..n)
        .map(|_| {
            let mut p = [0.0f32; D];
            for x in p.iter_mut() {
                *x = rng.gen::<f32>() * TAU;
            }
            BVec::from_radians(&p).expect("D")
        })
        .collect()
}

/// Perturba cada fasor uniformemente em `±frac·π` — a cópia "suja" do ACF.
fn jitter_codebook(base: &[BVec], rng: &mut ChaCha8Rng, frac: f32) -> Vec<BVec> {
    let amp = frac * PI;
    base.iter()
        .map(|v| {
            let mut p = [0u16; D];
            for (j, o) in p.iter_mut().enumerate() {
                let d = rng.gen_range(-amp..amp);
                *o = BamPhase::from_radians(BamPhase(v.as_slice()[j]).to_radians() + d).0;
            }
            BVec::from_phases(p)
        })
        .collect()
}

/// Âncoras fixas de papel.
#[derive(Clone, Debug)]
pub struct Anchors {
    pub s: BVec,
    pub v: BVec,
    pub o: BVec,
    pub m: BVec,
    /// Fasores unitários das âncoras (evita `sin_cos` no caminho quente).
    pub s_re: [f32; D],
    pub s_im: [f32; D],
    pub v_re: [f32; D],
    pub v_im: [f32; D],
    pub o_re: [f32; D],
    pub o_im: [f32; D],
    pub m_re: [f32; D],
    pub m_im: [f32; D],
}

impl Anchors {
    pub fn build(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xA0C_4000);
        let a = random_codebook(4, &mut rng);
        let (s_re, s_im) = unit_phasor(a[0].as_slice());
        let (v_re, v_im) = unit_phasor(a[1].as_slice());
        let (o_re, o_im) = unit_phasor(a[2].as_slice());
        let (m_re, m_im) = unit_phasor(a[3].as_slice());
        Self {
            s: a[0],
            v: a[1],
            o: a[2],
            m: a[3],
            s_re,
            s_im,
            v_re,
            v_im,
            o_re,
            o_im,
            m_re,
            m_im,
        }
    }

    pub fn re(&self, role: usize) -> &[f32; D] {
        match role {
            ROLE_S => &self.s_re,
            ROLE_V => &self.v_re,
            ROLE_O => &self.o_re,
            _ => &self.m_re,
        }
    }

    pub fn im(&self, role: usize) -> &[f32; D] {
        match role {
            ROLE_S => &self.s_im,
            ROLE_V => &self.v_im,
            ROLE_O => &self.o_im,
            _ => &self.m_im,
        }
    }

    pub fn of(&self, role: usize) -> &BVec {
        match role {
            ROLE_S => &self.s,
            ROLE_V => &self.v,
            ROLE_O => &self.o,
            _ => &self.m,
        }
    }
}

/// Termo `A_role ⊗ ρ^role(filler)` (uma fase pura).
#[inline]
pub fn role_term(anchor: &BVec, filler: &BVec, k: usize) -> Phases {
    bind(anchor.as_slice(), &rho(filler.as_slice(), k))
}

/// Fasor unitário do termo, escrito em buffers pré-alocados.
#[inline]
fn term_phasor_into(
    anchor: &BVec,
    filler: &BVec,
    k: usize,
    out_re: &mut [f32; D],
    out_im: &mut [f32; D],
) {
    let ph = role_term(anchor, filler, k);
    let lut = BamLut::global();
    for j in 0..D {
        let (s, c) = lut.sin_cos(BamPhase(ph[j]));
        out_re[j] = c;
        out_im[j] = s;
    }
}

// ---------------------------------------------------------------------------
// Cleanup: fase (sentença) e complexo (memória)
// ---------------------------------------------------------------------------

/// Resultado de um *cleanup memory*.
#[derive(Clone, Copy, Debug)]
pub struct Cleanup {
    pub index: usize,
    pub sim: f32,
    pub margin: f32,
}

/// Varre o codebook de fase (vetorizado) e devolve o vencedor + margem.
pub fn cleanup_phase(cue: &Phases, book: &[BVec], sims: &mut [f32; N_ENT]) -> Cleanup {
    let mut best = 0usize;
    let mut best_sim = f32::NEG_INFINITY;
    let mut second = f32::NEG_INFINITY;
    for (i, atom) in book.iter().enumerate() {
        let s = tri_sim(cue, atom.as_slice());
        sims[i] = s;
        if s > best_sim {
            second = best_sim;
            best_sim = s;
            best = i;
        } else if s > second {
            second = s;
        }
    }
    Cleanup {
        index: best,
        sim: best_sim,
        margin: best_sim - second,
    }
}

/// Cleanup **complexo**: `Re⟨z, átomo⟩ / (D·Q15)`.
///
/// `z` já vem normalizado por `K · (Q15 / 2^shift)` (ver
/// [`Shard::normalize_into`]); o sinal de um fator verdadeiro fica em `≈ 1` e o
/// ruído de `M−1` fatos cai por `√D`.
pub fn cleanup_complex(
    cue_re: &[f32; D],
    cue_im: &[f32; D],
    book: &[Q15Atom],
    sims: &mut [f32; N_ENT],
) -> Cleanup {
    // Q5 no cue: sinal ~32, ruído ~60; o produto com Q15 cabe em i32 em D=160.
    // O ganho é a soma inteira (vetorizável) sem perda mensurável de recall.
    const CUE_Q: f32 = 32.0;
    let mut cr = [0i16; D];
    let mut ci = [0i16; D];
    cue_q5_into(cue_re, cue_im, &mut cr, &mut ci);
    let inv = 1.0 / (D as f32 * Q15 * CUE_Q);
    let mut best = 0usize;
    let mut best_sim = f32::NEG_INFINITY;
    let mut second = f32::NEG_INFINITY;
    for (i, atom) in book.iter().enumerate() {
        let mut acc = 0i32;
        for j in 0..D {
            acc += cr[j] as i32 * atom.re[j] as i32 + ci[j] as i32 * atom.im[j] as i32;
        }
        let s = acc as f32 * inv;
        sims[i] = s;
        if s > best_sim {
            second = best_sim;
            best_sim = s;
            best = i;
        } else if s > second {
            second = s;
        }
    }
    Cleanup {
        index: best,
        sim: best_sim,
        margin: best_sim - second,
    }
}

/// Converte o cue complexo para Q5 (mesma escala do cleanup).
#[inline]
fn cue_q5_into(cue_re: &[f32; D], cue_im: &[f32; D], cr: &mut [i16; D], ci: &mut [i16; D]) {
    const CUE_Q: f32 = 32.0;
    for j in 0..D {
        cr[j] = (cue_re[j] * CUE_Q).clamp(-256.0, 256.0) as i16;
        ci[j] = (cue_im[j] * CUE_Q).clamp(-256.0, 256.0) as i16;
    }
}

// ---------------------------------------------------------------------------
// Shard holográfico (acumulador de `K` traços, 1 termo por fato)
// ---------------------------------------------------------------------------

/// Um acumulador: `≤ 48` fatos `key ⊗ value`, `K` traços com fase por
/// `(fato, traço)`. Guarda-se a fase `key ⊗ value` (produto, 1 termo por fato)
/// — é exatamente o regime que o E1b mediu.
#[derive(Clone, Debug)]
pub struct Shard {
    traces: usize,
    terms: usize,
    tre: Vec<[i16; D]>,
    tim: Vec<[i16; D]>,
    acc_re: Vec<[i32; D]>,
    acc_im: Vec<[i32; D]>,
    shift: u32,
    psi: Vec<u16>,
    subj: Vec<u16>,
    verb: Vec<u16>,
    obj: Vec<u16>,
    modu: Vec<u16>,
    rng: u64,
}

impl Shard {
    pub fn new(traces: usize, terms: usize, seed: u64) -> Self {
        Self {
            traces,
            terms,
            tre: vec![[0i16; D]; traces],
            tim: vec![[0i16; D]; traces],
            acc_re: vec![[0i32; D]; traces],
            acc_im: vec![[0i32; D]; traces],
            shift: 0,
            psi: Vec::new(),
            subj: Vec::new(),
            verb: Vec::new(),
            obj: Vec::new(),
            modu: Vec::new(),
            rng: seed,
        }
    }

    pub fn len(&self) -> usize {
        self.subj.len()
    }

    pub fn is_empty(&self) -> bool {
        self.subj.is_empty()
    }

    pub fn traces(&self) -> usize {
        self.traces
    }

    pub fn terms(&self) -> usize {
        self.terms
    }

    /// Fator de escala entre o acumulador `i16` e o fasor Q15.
    pub fn unit_q15(&self) -> f32 {
        Q15 / (1u32 << self.shift) as f32
    }

    /// Normaliza o complexo bruto do unbinding para a escala dos fatores
    /// (`sinal ≈ 1` por fator, sem depender do `shift` do shard).
    pub fn normalize_into(&self, z_re: &mut [f32; D], z_im: &mut [f32; D]) {
        let inv = 1.0 / (self.traces as f32 * self.unit_q15());
        for j in 0..D {
            z_re[j] *= inv;
            z_im[j] *= inv;
        }
    }

    /// Grava um fato como **bundle de `terms` fasores** (um por papel do
    /// predicado), cada um com `K` traços coerentes.
    ///
    /// O valor não é o produto de fases dos papéis (isso exigiria busca sobre
    /// o codebook); é a superposição `e^{i(term_o)} + e^{i(term_m)}`, que o
    /// unbinding entrega como soma complexa e o cleanup separa com o ganho
    /// `√D` do produto interno.
    pub fn add_terms(&mut self, key: &Phases, values: &[Phases], ids: (u16, u16, u16, u16)) {
        debug_assert_eq!(values.len(), self.terms);
        let lut = BamLut::global();
        for (ti, value) in values.iter().enumerate() {
            for t in 0..self.traces {
                // O primeiro termo/traço é a referência de fase; os demais são
                // fases independentes por `(fato, termo, traço)`.
                let psi = if ti == 0 && t == 0 {
                    0u16
                } else {
                    (splitmix64(&mut self.rng) >> 48) as u16
                };
                self.psi.push(psi);
                for j in 0..D {
                    let th = key[j].wrapping_add(value[j]).wrapping_add(psi);
                    let (s, c) = lut.sin_cos_q15(BamPhase(th));
                    self.acc_re[t][j] += c as i32;
                    self.acc_im[t][j] += s as i32;
                }
            }
        }
        self.subj.push(ids.0);
        self.verb.push(ids.1);
        self.obj.push(ids.2);
        self.modu.push(ids.3);
    }

    /// Requantiza os traços para `i16` com 12 bits de folga e libera os
    /// acumuladores de construção (fim da fase de escrita).
    pub fn finalize(&mut self) {
        let max = self
            .acc_re
            .iter()
            .chain(self.acc_im.iter())
            .flat_map(|a| a.iter())
            .map(|v| v.unsigned_abs())
            .max()
            .unwrap_or(0);
        let bits = 32 - max.leading_zeros();
        self.shift = bits.saturating_sub(12);
        for t in 0..self.traces {
            for j in 0..D {
                self.tre[t][j] = (self.acc_re[t][j] >> self.shift) as i16;
                self.tim[t][j] = (self.acc_im[t][j] >> self.shift) as i16;
            }
        }
        self.acc_re = Vec::new();
        self.acc_im = Vec::new();
        self.subj.shrink_to_fit();
        self.verb.shrink_to_fit();
        self.obj.shrink_to_fit();
        self.modu.shrink_to_fit();
        self.psi.shrink_to_fit();
    }

    /// Índice do fato com `(subj, verb)` — o endereço sintático do shard.
    pub fn find(&self, subj: u16, verb: u16) -> Option<usize> {
        self.subj
            .iter()
            .zip(&self.verb)
            .position(|(&s, &v)| s == subj && v == verb)
    }

    /// Desvincula `key` do fato `fact` nos `K` traços (coerente) e devolve o
    /// complexo `z` — ainda **não** projetado.
    pub fn unbind(&self, fact: usize, key: &Phases, z_re: &mut [f32; D], z_im: &mut [f32; D]) {
        z_re.fill(0.0);
        z_im.fill(0.0);
        let inv = 1.0 / Q14;
        for term in 0..self.terms {
            for t in 0..self.traces {
                let psi = self.psi[(fact * self.terms + term) * self.traces + t];
                let tr = &self.tre[t];
                let ti = &self.tim[t];
                for lane in 0..D / LANES {
                    let base = lane * LANES;
                    let mut sa = [0i16; LANES];
                    let mut ca = [0i16; LANES];
                    for l in 0..LANES {
                        let phi = key[base + l].wrapping_add(psi);
                        sa[l] = phi as i16;
                        ca[l] = phi.wrapping_add(QUARTER) as i16;
                    }
                    let s = poly_sin_q14(&sa);
                    let c = poly_sin_q14(&ca);
                    for l in 0..LANES {
                        let cc = c[l] as f32 * inv;
                        let ss = s[l] as f32 * inv;
                        let mr = tr[base + l] as f32;
                        let mi = ti[base + l] as f32;
                        z_re[base + l] += mr * cc + mi * ss;
                        z_im[base + l] += mi * cc - mr * ss;
                    }
                }
            }
        }
    }

    pub fn memory_bytes(&self) -> usize {
        self.tre.capacity() * std::mem::size_of::<[i16; D]>()
            + self.tim.capacity() * std::mem::size_of::<[i16; D]>()
            + self.psi.capacity() * 2
            + (self.subj.capacity()
                + self.verb.capacity()
                + self.obj.capacity()
                + self.modu.capacity())
                * 2
    }
}

// ---------------------------------------------------------------------------
// Memória PLH: 24 shards roteados por (sujeito, verbo)
// ---------------------------------------------------------------------------

/// Roteador determinístico do papel de endereço: `(sujeito, verbo) → shard`.
#[inline]
pub fn shard_of(subj: usize, verb: usize) -> usize {
    (subj.wrapping_mul(7).wrapping_add(verb.wrapping_mul(5))) % SHARDS
}

#[derive(Clone, Debug)]
pub struct PlhMemory {
    shards: Vec<Shard>,
}

impl PlhMemory {
    pub fn new(traces: usize, terms: usize, seed: u64) -> Self {
        Self {
            shards: (0..SHARDS)
                .map(|i| {
                    Shard::new(
                        traces,
                        terms,
                        seed ^ (i as u64 + 1).wrapping_mul(0x9E37_79B9),
                    )
                })
                .collect(),
        }
    }

    pub fn add_terms(
        &mut self,
        subj: u16,
        verb: u16,
        obj: u16,
        modu: u16,
        key: &Phases,
        values: &[Phases],
    ) {
        let idx = shard_of(subj as usize, verb as usize);
        self.shards[idx].add_terms(key, values, (subj, verb, obj, modu));
    }

    pub fn finalize(&mut self) {
        for s in &mut self.shards {
            s.finalize();
        }
    }

    pub fn shard(&self, idx: usize) -> &Shard {
        &self.shards[idx]
    }

    pub fn max_load(&self) -> usize {
        self.shards.iter().map(Shard::len).max().unwrap_or(0)
    }

    pub fn memory_bytes(&self) -> usize {
        self.shards.iter().map(Shard::memory_bytes).sum()
    }
}

// ---------------------------------------------------------------------------
// Ressonador ACF de 2 fatores (predicado OBJ ⊕ MOD do fato)
// ---------------------------------------------------------------------------

/// Scratch pré-alocado do caminho quente (nada aqui aloca depois do `new`).
#[derive(Debug)]
pub struct QueryScratch {
    pub z_re: [f32; D],
    pub z_im: [f32; D],
    pub res_re: [f32; D],
    pub res_im: [f32; D],
    pub cue_re: [f32; D],
    pub cue_im: [f32; D],
    pub term_re: [f32; D],
    pub term_im: [f32; D],
    pub sims: [f32; N_ENT],
}

impl QueryScratch {
    pub fn new() -> Self {
        Self {
            z_re: [0.0; D],
            z_im: [0.0; D],
            res_re: [0.0; D],
            res_im: [0.0; D],
            cue_re: [0.0; D],
            cue_im: [0.0; D],
            term_re: [0.0; D],
            term_im: [0.0; D],
            sims: [0.0; N_ENT],
        }
    }
}

impl Default for QueryScratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Fatores do predicado (2 fatores) devolvidos pelo ressonador.
#[derive(Clone, Copy, Debug)]
pub struct PairFactors {
    pub obj: usize,
    pub modu: usize,
    pub sim_obj: f32,
    pub sim_mod: f32,
    pub margin_obj: f32,
    pub margin_mod: f32,
    pub iters: u8,
    pub converged: bool,
    pub validated: bool,
}

/// Escreve em `(out_re, out_im)` o papel desvinculado de `z − other`, já
/// projetado pela âncora e despermutado por `ρ^{-k}`.
// 8 argumentos: parâmetros de projeção independentes; agrupar em struct seria churn sem benefício.
#[allow(clippy::too_many_arguments)]
fn role_cue_into(
    z_re: &[f32; D],
    z_im: &[f32; D],
    anchor_re: &[f32; D],
    anchor_im: &[f32; D],
    k: usize,
    other: Option<(&[f32; D], &[f32; D])>,
    out_re: &mut [f32; D],
    out_im: &mut [f32; D],
) {
    let k = k % D;
    for j in 0..D {
        let mut re = z_re[j];
        let mut im = z_im[j];
        if let Some((or, oi)) = other {
            // O valor é um **bundle** (soma de fasores): isolar o papel é
            // subtrair o re-encode unitário do outro fator.
            re -= or[j];
            im -= oi[j];
        }
        // (re + i·im)·(c − i·s)
        let ur = re * anchor_re[j] + im * anchor_im[j];
        let ui = im * anchor_re[j] - re * anchor_im[j];
        out_re[j] = ur;
        out_im[j] = ui;
    }
    // `ρ^{-k}` é uma permutação de posições: aplica no vetor já desvinculado.
    if k != 0 {
        let mut tmp_re = [0.0f32; D];
        let mut tmp_im = [0.0f32; D];
        tmp_re.copy_from_slice(out_re);
        tmp_im.copy_from_slice(out_im);
        for j in 0..D {
            out_re[j] = tmp_re[(j + D - k) % D];
            out_im[j] = tmp_im[(j + D - k) % D];
        }
    }
}

/// Ressonador de `(OBJ, MOD)` com projeção *re-encode* entre passos.
///
/// Cada passo subtrai do predicado o **átomo limpo** do outro fator
/// (`re-encode`), desvincula o papel próprio e faz cleanup na **cópia ACF**;
/// no fim valida o vencedor na **cópia limpa**.
pub fn resonate_pair(sc: &mut QueryScratch, cb: &Codebooks, a: &Anchors) -> PairFactors {
    // Init: unbind each role anchor. The other factor is a unit phasor in the
    // bundle; its projection onto a random atom is ~1/sqrt(D), so the cleanup
    // already resolves the role with SNR ~6 sigma.
    role_cue_into(
        &sc.z_re,
        &sc.z_im,
        &a.o_re,
        &a.o_im,
        ROLE_O,
        None,
        &mut sc.cue_re,
        &mut sc.cue_im,
    );
    let co = cleanup_complex(&sc.cue_re, &sc.cue_im, cb.acf_q(ROLE_O), &mut sc.sims);
    role_cue_into(
        &sc.z_re,
        &sc.z_im,
        &a.m_re,
        &a.m_im,
        ROLE_M,
        None,
        &mut sc.cue_re,
        &mut sc.cue_im,
    );
    let cm = cleanup_complex(&sc.cue_re, &sc.cue_im, cb.acf_q(ROLE_M), &mut sc.sims);
    let mut o = co.index;
    let mut m = cm.index;

    // Re-encode: subtract the clean atom of the other factor, unbind own role,
    // cleanup on the ACF copy. The subtraction is exact because the value is a
    // sum (bundle), not a product of phases.
    let mut iters = 0u8;
    let mut converged = false;
    for _ in 0..PAIR_ITERS {
        iters = iters.saturating_add(1);
        let prev = (o, m);

        term_phasor_into(
            &a.m,
            &cb.modu.clean[m],
            ROLE_M,
            &mut sc.term_re,
            &mut sc.term_im,
        );
        role_cue_into(
            &sc.z_re,
            &sc.z_im,
            &a.o_re,
            &a.o_im,
            ROLE_O,
            Some((&sc.term_re, &sc.term_im)),
            &mut sc.cue_re,
            &mut sc.cue_im,
        );
        o = cleanup_complex(&sc.cue_re, &sc.cue_im, cb.acf_q(ROLE_O), &mut sc.sims).index;

        term_phasor_into(
            &a.o,
            &cb.ent.clean[o],
            ROLE_O,
            &mut sc.term_re,
            &mut sc.term_im,
        );
        role_cue_into(
            &sc.z_re,
            &sc.z_im,
            &a.m_re,
            &a.m_im,
            ROLE_M,
            Some((&sc.term_re, &sc.term_im)),
            &mut sc.cue_re,
            &mut sc.cue_im,
        );
        m = cleanup_complex(&sc.cue_re, &sc.cue_im, cb.acf_q(ROLE_M), &mut sc.sims).index;

        if (o, m) == prev {
            converged = true;
            break;
        }
    }

    // Validation on the clean codebook.
    term_phasor_into(
        &a.m,
        &cb.modu.clean[m],
        ROLE_M,
        &mut sc.term_re,
        &mut sc.term_im,
    );
    role_cue_into(
        &sc.z_re,
        &sc.z_im,
        &a.o_re,
        &a.o_im,
        ROLE_O,
        Some((&sc.term_re, &sc.term_im)),
        &mut sc.cue_re,
        &mut sc.cue_im,
    );
    let co = cleanup_complex(&sc.cue_re, &sc.cue_im, cb.clean_q(ROLE_O), &mut sc.sims);

    term_phasor_into(
        &a.o,
        &cb.ent.clean[o],
        ROLE_O,
        &mut sc.term_re,
        &mut sc.term_im,
    );
    role_cue_into(
        &sc.z_re,
        &sc.z_im,
        &a.m_re,
        &a.m_im,
        ROLE_M,
        Some((&sc.term_re, &sc.term_im)),
        &mut sc.cue_re,
        &mut sc.cue_im,
    );
    let cm = cleanup_complex(&sc.cue_re, &sc.cue_im, cb.clean_q(ROLE_M), &mut sc.sims);

    let validated = co.index == o && cm.index == m;
    PairFactors {
        obj: o,
        modu: m,
        sim_obj: co.sim,
        sim_mod: cm.sim,
        margin_obj: co.margin,
        margin_mod: cm.margin,
        iters,
        converged,
        validated,
    }
}

/// Recalcula as similaridades de todos os átomos de OBJ e MOD para a leitura
/// final (fora do caminho quente; usado pelos gates de SNR/abstenção).
pub fn evidence_sims(
    sc: &mut QueryScratch,
    cb: &Codebooks,
    a: &Anchors,
    p: &PairFactors,
    obj_sims: &mut [f32; N_ENT],
    mod_sims: &mut [f32; N_MOD],
) {
    term_phasor_into(
        &a.m,
        &cb.modu.clean[p.modu],
        ROLE_M,
        &mut sc.term_re,
        &mut sc.term_im,
    );
    role_cue_into(
        &sc.z_re,
        &sc.z_im,
        &a.o_re,
        &a.o_im,
        ROLE_O,
        Some((&sc.term_re, &sc.term_im)),
        &mut sc.cue_re,
        &mut sc.cue_im,
    );
    let _ = cleanup_complex(
        &sc.cue_re,
        &sc.cue_im,
        cb.ent.clean_q.as_slice(),
        &mut sc.sims,
    );
    obj_sims[..N_ENT].copy_from_slice(&sc.sims[..N_ENT]);

    term_phasor_into(
        &a.o,
        &cb.ent.clean[p.obj],
        ROLE_O,
        &mut sc.term_re,
        &mut sc.term_im,
    );
    role_cue_into(
        &sc.z_re,
        &sc.z_im,
        &a.m_re,
        &a.m_im,
        ROLE_M,
        Some((&sc.term_re, &sc.term_im)),
        &mut sc.cue_re,
        &mut sc.cue_im,
    );
    let _ = cleanup_complex(
        &sc.cue_re,
        &sc.cue_im,
        cb.modu.clean_q.as_slice(),
        &mut sc.sims,
    );
    mod_sims[..N_MOD].copy_from_slice(&sc.sims[..N_MOD]);
}

// ---------------------------------------------------------------------------
// Codec de sentença (⊕ de 4 papéis) + ressonador ACF de 4 fatores
// ---------------------------------------------------------------------------

/// Superposição complexa de fasores unitários (o `⊕` do código de sentença).
#[derive(Clone, Copy, Debug)]
pub struct Bundle {
    pub re: [f32; D],
    pub im: [f32; D],
}

impl Bundle {
    pub fn zero() -> Self {
        Self {
            re: [0.0; D],
            im: [0.0; D],
        }
    }

    pub fn add_term(&mut self, phases: &Phases) {
        let (re, im) = unit_phasor(phases);
        for j in 0..D {
            self.re[j] += re[j];
            self.im[j] += im[j];
        }
    }

    /// `arg(Ψ ⊗ conj(anchor))` — desvincula a âncora e projeta ao círculo.
    pub fn project_role(&self, anchor: &BVec) -> Phases {
        let lut = BamLut::global();
        let mut re = [0.0f32; D];
        let mut im = [0.0f32; D];
        for j in 0..D {
            let (s, c) = lut.sin_cos(BamPhase(anchor.as_slice()[j]));
            re[j] = self.re[j] * c + self.im[j] * s;
            im[j] = self.im[j] * c - self.re[j] * s;
        }
        project_unit(&re, &im)
    }
}

/// `Ψ = A_S⊗ρ^0(s) ⊕ A_V⊗ρ^1(v) ⊕ A_O⊗ρ^2(o) ⊕ A_M⊗ρ^3(m)`.
pub fn encode_sentence(cb: &Codebooks, a: &Anchors, ids: [usize; 4]) -> Bundle {
    let mut b = Bundle::zero();
    b.add_term(&role_term(a.of(ROLE_S), &cb.ent.clean[ids[0]], ROLE_S));
    b.add_term(&role_term(a.of(ROLE_V), &cb.verb.clean[ids[1]], ROLE_V));
    b.add_term(&role_term(a.of(ROLE_O), &cb.ent.clean[ids[2]], ROLE_O));
    b.add_term(&role_term(a.of(ROLE_M), &cb.modu.clean[ids[3]], ROLE_M));
    b
}

/// Fatores de uma sentença decodificada.
#[derive(Clone, Copy, Debug, Default)]
pub struct Factors {
    pub ids: [usize; 4],
    pub sims: [f32; 4],
    pub margins: [f32; 4],
    pub iters: u8,
    pub converged: bool,
    pub validated: bool,
}

impl Factors {
    pub fn min_sim(&self) -> f32 {
        self.sims.iter().copied().fold(f32::INFINITY, f32::min)
    }

    pub fn min_margin(&self) -> f32 {
        self.margins.iter().copied().fold(f32::INFINITY, f32::min)
    }
}

/// Fatora `Ψ` nos quatro papéis.
pub fn factorize_sentence(psi: &Bundle, cb: &Codebooks, a: &Anchors) -> Factors {
    let mut f = Factors::default();
    let mut last: [Phases; 4] = [[0u16; D]; 4];
    let mut sims = [0.0f32; N_ENT];

    for r in 0..4 {
        let cue = unrho(&psi.project_role(a.of(r)), r);
        last[r] = cue;
        let c = cleanup_phase(&cue, cb.acf(r), &mut sims);
        f.ids[r] = c.index;
        f.sims[r] = c.sim;
        f.margins[r] = c.margin;
    }

    let mut converged = false;
    for it in 0..RES_ITERS {
        f.iters = it as u8 + 1;
        let mut changed = false;
        for r in 0..4 {
            let mut rr = [0.0f32; D];
            let mut ri = [0.0f32; D];
            for r2 in 0..4 {
                if r2 == r {
                    continue;
                }
                let term = role_term(a.of(r2), &cb.clean(r2)[f.ids[r2]], r2);
                let (tr, ti) = unit_phasor(&term);
                for j in 0..D {
                    rr[j] += tr[j];
                    ri[j] += ti[j];
                }
            }
            let res = Bundle {
                re: std::array::from_fn(|j| psi.re[j] - rr[j]),
                im: std::array::from_fn(|j| psi.im[j] - ri[j]),
            };
            let cue = unrho(&res.project_role(a.of(r)), r);
            last[r] = cue;
            let c = cleanup_phase(&cue, cb.acf(r), &mut sims);
            if c.index != f.ids[r] {
                changed = true;
                f.ids[r] = c.index;
            }
            f.sims[r] = c.sim;
            f.margins[r] = c.margin;
        }
        if !changed {
            converged = true;
            break;
        }
    }

    let mut validated = true;
    for r in 0..4 {
        let c = cleanup_phase(&last[r], cb.clean(r), &mut sims);
        if c.index != f.ids[r] {
            validated = false;
        }
    }
    f.converged = converged;
    f.validated = validated;
    f
}

/// Erro relativo `‖Ψ − reencode(ids)‖ / ‖Ψ‖` — usado no gate de profundidade.
pub fn reencode_residual(psi: &Bundle, cb: &Codebooks, a: &Anchors, ids: [usize; 4]) -> f32 {
    let hat = encode_sentence(cb, a, ids);
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for j in 0..D {
        let dr = psi.re[j] - hat.re[j];
        let di = psi.im[j] - hat.im[j];
        num += (dr * dr + di * di) as f64;
        den += (psi.re[j] * psi.re[j] + psi.im[j] * psi.im[j]) as f64;
    }
    (num / den.max(1e-12)).sqrt() as f32
}

/// Adiciona ruído de fase uniforme `±σ√3` (mesma σ do gate de abstenção).
pub fn add_phase_noise(psi: &mut Bundle, sigma: f32, rng: &mut ChaCha8Rng) {
    if sigma <= 0.0 {
        return;
    }
    let amp = sigma * 1.732_050_8;
    for j in 0..D {
        let d = rng.gen_range(-amp..amp);
        let (s, c) = (d.sin(), d.cos());
        let (re, im) = (psi.re[j], psi.im[j]);
        psi.re[j] = re * c - im * s;
        psi.im[j] = re * s + im * c;
    }
}

// ---------------------------------------------------------------------------
// Abstenção calibrada
// ---------------------------------------------------------------------------

/// Evidência da leitura: pior similaridade e pior margem entre os fatores.
#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    pub min_sim: f32,
    pub min_margin: f32,
}

impl Evidence {
    pub fn from_factors(f: &Factors) -> Self {
        Self {
            min_sim: f.min_sim(),
            min_margin: f.min_margin(),
        }
    }

    pub fn from_pair(p: &PairFactors) -> Self {
        Self {
            min_sim: p.sim_obj.min(p.sim_mod),
            min_margin: p.margin_obj.min(p.margin_mod),
        }
    }
}

/// Abstenção seletiva: o readout só vale se a evidência observada o sustenta.
#[inline]
pub fn should_abstain(e: &Evidence) -> bool {
    e.min_sim < SIM_FLOOR || e.min_margin < MARGIN_FLOOR
}

// ---------------------------------------------------------------------------
// Construção do mundo
// ---------------------------------------------------------------------------

/// Um fato do mundo: `(sujeito, verbo, objeto, modificador)`.
#[derive(Clone, Copy, Debug)]
pub struct Fact {
    pub ids: [u16; 4],
}

impl Fact {
    pub fn key(&self, cb: &Codebooks, a: &Anchors) -> Phases {
        let ks = role_term(a.of(ROLE_S), &cb.ent.clean[self.ids[0] as usize], ROLE_S);
        let kv = role_term(a.of(ROLE_V), &cb.verb.clean[self.ids[1] as usize], ROLE_V);
        bind(&ks, &kv)
    }

    /// Os dois fasores do predicado, guardados como **bundle** (soma), não
    /// como produto de fases. É o que preserva o ganho `√D` no cleanup.
    pub fn value_terms(&self, cb: &Codebooks, a: &Anchors) -> [Phases; 2] {
        [
            role_term(a.of(ROLE_O), &cb.ent.clean[self.ids[2] as usize], ROLE_O),
            role_term(a.of(ROLE_M), &cb.modu.clean[self.ids[3] as usize], ROLE_M),
        ]
    }
}

/// Mundo determinístico de `n` fatos com `(sujeito, verbo)` únicos.
pub fn build_world(n: usize) -> Vec<Fact> {
    (0..n)
        .map(|f| {
            let s = f % N_ENT;
            let v = (f / N_ENT) % N_VERB;
            let o = (f * 37 + 11) % N_ENT;
            let m = (f * 5 + 3) % N_MOD;
            Fact {
                ids: [s as u16, v as u16, o as u16, m as u16],
            }
        })
        .collect()
}

/// Constrói a memória sharded a partir do mundo (`≤ 48` fatos por acumulador).
pub fn build_sharded(
    facts: &[Fact],
    cb: &Codebooks,
    a: &Anchors,
    traces: usize,
    seed: u64,
) -> PlhMemory {
    let mut mem = PlhMemory::new(traces, 2, seed);
    for f in facts {
        let terms = f.value_terms(cb, a);
        mem.add_terms(
            f.ids[0],
            f.ids[1],
            f.ids[2],
            f.ids[3],
            &f.key(cb, a),
            &terms,
        );
    }
    mem.finalize();
    mem
}

/// Constrói **uma** superposição plana com todos os fatos (o controle que o
/// E1b mostrou colapsar) — o mesmo `Shard`, sem sharding.
pub fn build_flat(facts: &[Fact], cb: &Codebooks, a: &Anchors, traces: usize) -> Shard {
    let mut s = Shard::new(traces, 2, 0xF1A7_0001);
    for f in facts {
        s.add_terms(
            &f.key(cb, a),
            &f.value_terms(cb, a),
            (f.ids[0], f.ids[1], f.ids[2], f.ids[3]),
        );
    }
    s.finalize();
    s
}

// ---------------------------------------------------------------------------
// Caminho quente
// ---------------------------------------------------------------------------

/// Caminho quente sharded: resolve o fato no shard roteado, desvincula,
/// normaliza pelo número de traços e roda o ressonador ACF. **Zero alocação.**
pub fn hot_query(
    mem: &PlhMemory,
    cb: &Codebooks,
    a: &Anchors,
    f: &Fact,
    sc: &mut QueryScratch,
) -> Option<PairFactors> {
    let idx = shard_of(f.ids[0] as usize, f.ids[1] as usize);
    let shard = &mem.shards[idx];
    let fact = shard.find(f.ids[0], f.ids[1])?;
    let key = f.key(cb, a);
    shard.unbind(fact, &key, &mut sc.z_re, &mut sc.z_im);
    shard.normalize_into(&mut sc.z_re, &mut sc.z_im);
    Some(resonate_pair(sc, cb, a))
}

/// Controle: a mesma pipeline sobre **uma** superposição plana.
pub fn hot_query_flat(
    flat: &Shard,
    cb: &Codebooks,
    a: &Anchors,
    f: &Fact,
    sc: &mut QueryScratch,
) -> Option<PairFactors> {
    let fact = flat.find(f.ids[0], f.ids[1])?;
    let key = f.key(cb, a);
    flat.unbind(fact, &key, &mut sc.z_re, &mut sc.z_im);
    flat.normalize_into(&mut sc.z_re, &mut sc.z_im);
    Some(resonate_pair(sc, cb, a))
}

// ---------------------------------------------------------------------------
// Acoplamento com a hierarquia de renormalização (64 → 16 → 4)
// ---------------------------------------------------------------------------

/// Resultado do torque top-down macro → micro.
#[derive(Clone, Copy, Debug)]
pub struct RgResult {
    pub before: f32,
    pub after: f32,
    pub alignment: f32,
    pub macro_order: f32,
    pub goal: f32,
}

#[inline]
fn wrap_signed(x: f32) -> f32 {
    let mut v = x % TAU;
    if v > PI {
        v -= TAU;
    } else if v < -PI {
        v += TAU;
    }
    v
}

fn mean_abs_error(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = 0.0f32;
    for (&x, &y) in a.iter().zip(b) {
        acc += wrap_signed(x - y).abs();
    }
    acc / a.len().max(1) as f32
}

/// Mede o efeito do torque top-down da escala macro (`4`) sobre a micro (`64`).
///
/// `clean` é o campo coerente; `noisy` é a mesma cena com ruído de fase. A
/// hierarquia extrai o objetivo macro de `clean` e o impõe a `noisy`; o erro
/// angular micro deve cair. É a demonstração de que a intenção macro
/// desambigua a micro — não uma promessa de ganho ilimitado.
pub fn rg_topdown(clean: &[f32], noisy: &[f32], gain: f32) -> RgResult {
    assert_eq!(clean.len(), 64);
    assert_eq!(noisy.len(), 64);
    let mut rg = RenormalizationHierarchy::new(64).expect("64 e multiplo de 16");
    rg.set_gain(0.0);
    rg.forward(clean).expect("clean");
    let goal = rg.macro_phase();
    let macro_order = rg.macro_order();

    rg.forward(noisy).expect("noisy");
    let before = mean_abs_error(rg.micro_phases(), clean);

    rg.set_goal(goal);
    rg.set_gain(gain);
    rg.forward(noisy).expect("noisy goal");
    let after = mean_abs_error(rg.micro_phases(), clean);

    RgResult {
        before,
        after,
        alignment: rg.alignment(),
        macro_order,
        goal,
    }
}
