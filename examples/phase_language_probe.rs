//! E1 — *Phase-Language Head* (PLH) na borda: memória **sharded** + ressonador ACF.
//!
//! O E1b mediu a lei do acumulador holográfico: `M_max ≈ 0,4·D` fatos
//! (`D=160, K=6` ⇒ 64 fatos, `recall@1 ≥ 99%`). Empilhar 1.000 fatos numa única
//! superposição plana colapsa o `recall@1` para ~2%. Este experimento monta a
//! cabeça de linguagem **em cima da lei**, não contra ela:
//!
//! 1. **Memória sharded por papel sintático** — 24 shards com `≤ 48` fatos cada
//!    (75% do teto medido). O shard de um fato é função determinística do par
//!    `(sujeito, verbo)`; cada shard é um acumulador de `K` traços em BAM
//!    (`u16`, `|z| = 1` exato) com fase `ψ` por `(fato, traço)`.
//! 2. **Binding não-comutativo com âncoras fixas** — `ρ^k` é um deslocamento
//!    cíclico de posições (permutação), não uma rotação de fase: por isso
//!    `ρ^a(x) ⊗ ρ^b(y) ≠ ρ^b(x) ⊗ ρ^a(y)`. O código de sentença é
//!    `Ψ = A_S⊗ρ^0(s) ⊕ A_V⊗ρ^1(v) ⊕ A_O⊗ρ^2(o) ⊕ A_M⊗ρ^3(m)`.
//! 3. **Ressonador ACF** (*Asymmetric Codebook Factorizer*) — fatora `Ψ` entre
//!    os papéis por *cleanup* iterativo com projeção *re-encode* (átomo limpo):
//!    decisões na cópia **perturbada** do codebook (`±4% de π`, `ChaCha8Rng`),
//!    validação na cópia **limpa**.
//!
//! Gates medidos: escala sharded (1.000 fatos), não-comutatividade estrita
//! (A→B vs B→A), SNR/latência/0-alloc e abstenção calibrada.
//!
//! ```text
//! cargo run --release --example phase_language_probe
//! cargo run --release --example phase_language_probe -- --quick
//! ```
//!
//! Saída: tabelas + `target/aether_e1_plh.json`.
//!
//! ## Por que o cleanup é complexo (e não projetado)
//!
//! O unbinding de um fato devolve `z ≈ e^{i·value} + ruído` com ruído de
//! superposição `≈ √(M/K)` **por posição**. Projetar `arg(z)` antes do cleanup
//! jogaria fora o ganho de `√D` (a similaridade média sobre `D` posições é que
//! separa sinal de crosstalk — o mesmo motivo pelo qual o `BamHolographicStore`
//! compara `z` com o codebook em `(re, im)`). Aqui todo cleanup é complexo:
//! `sim = (1/D)·Re⟨cue, e^{iθ_atom}⟩`.
//!
//! ## Invariantes de borda
//!
//! * `#![deny(unsafe_code)]` — a única exceção é o `CountingAllocator`
//!   (`#[allow(unsafe_code)]`), que é *instrumentação de medição* exigida pela
//!   linguagem (`GlobalAlloc`); o caminho de inferência não tem `unsafe`.
//! * `≤ 48` fatos por acumulador — **nunca** há superposição plana > 48.
//! * `0 B` alocados no caminho de consulta (unbinding + ressonador + cleanup).
//! * Todo vetor **guardado** é fase `u16` (`|z| = 1` estrito); o unbinding
//!   produz um complexo e o ressonador o mantém em `(re, im)` até o cleanup.

#![deny(unsafe_code)]
#![allow(clippy::needless_range_loop)]

use std::f32::consts::TAU;
use std::hint::black_box;
use std::time::Instant;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use aether_engine::core::bam_phase::{poly_sin_q14, BamHolographicVector, BamLut, BamPhase};
use aether_engine::core::silicon_entropy::splitmix64;

// ---------------------------------------------------------------------------
// Instrumentação de alocação (única `unsafe` do arquivo, e é do medidor)
// ---------------------------------------------------------------------------

#[allow(unsafe_code)]
mod alloc_probe {
    //! Contador de alocações por thread. `GlobalAlloc` exige `unsafe`; nada
    //! aqui participa da inferência — só conta chamadas do alocador global.

    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static ALLOCS: Cell<usize> = const { Cell::new(0) };
    }

    struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if ARMED.with(Cell::get) {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if ARMED.with(Cell::get) {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if ARMED.with(Cell::get) {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
            unsafe { System.realloc(ptr, layout, new_size) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[global_allocator]
    static GLOBAL: CountingAllocator = CountingAllocator;

    pub fn arm() {
        ALLOCS.with(|c| c.set(0));
        ARMED.with(|a| a.set(true));
    }

    pub fn disarm() -> usize {
        ARMED.with(|a| a.set(false));
        ALLOCS.with(Cell::get)
    }
}

// ---------------------------------------------------------------------------
// Constantes do experimento
// ---------------------------------------------------------------------------

/// Dimensão do espaço de fase (mesma do E1b).
const D: usize = 160;
/// Fasores por lote vetorizado (um `ymm` de `u16`).
const LANES: usize = 16;
/// Um quarto de volta em BAM.
const QUARTER: u16 = 0x4000;
/// Escala Q14 do seno polinomial (`1.0 = 16384`).
const Q14: f32 = 16_384.0;

/// Traços por acumulador (`K` do E1b).
const K_TRACES: usize = 6;
/// Shards (relações sintáticas endereçáveis).
const SHARDS: usize = 24;
/// Teto absoluto de fatos por acumulador (75% de `0,4·D = 64`).
const SHARD_CAP: usize = 48;
/// Fatos do gate de escala.
const N_FACTS: usize = 1000;
/// Vocabulário de entidades (sujeito **e** objeto — o mesmo codebook, para que
/// `A→B` e `B→A` possam ser comparados com os mesmos átomos).
const N_ENT: usize = 64;
/// Verbos (= relação).
const N_VERB: usize = 16;
/// Modificadores.
const N_MOD: usize = 8;
/// Jitter angular da cópia ACF do codebook (`±4% de π`).
const JITTER_FRAC: f32 = 0.04;
/// Iterações do ressonador de sentença (4 fatores).
const RES_ITERS: usize = 3;
/// Iterações do ressonador do predicado (2 fatores), no caminho quente.
const PAIR_ITERS: usize = 2;
/// Consultas do gate de escala.
const QUERIES: usize = 400;
/// Piso de evidência para abstenção (escolhido *a priori*, não no teste).
const SIM_FLOOR: f32 = 0.55;
/// Piso de margem (1º − 2º) para abstenção.
const MARGIN_FLOOR: f32 = 0.08;

/// Papéis sintáticos na ordem de `ρ^k`.
const ROLE_S: usize = 0;
const ROLE_V: usize = 1;
const ROLE_O: usize = 2;
const ROLE_M: usize = 3;

/// Fase de um fasor (`u16`, `[0, 2π) ↔ [0, 65536)`).
type Phases = [u16; D];
/// Vetor holográfico BAM de `D` fasores unitários.
type BVec = BamHolographicVector<D>;

/// Superposição complexa de fasores (o `⊕` do código de sentença e o `z` do
/// unbinding). Sinal limpo tem magnitude 1 por posição.
#[derive(Clone, Copy, Debug)]
struct Cplx {
    re: [f32; D],
    im: [f32; D],
}

impl Cplx {
    fn zero() -> Self {
        Self {
            re: [0.0; D],
            im: [0.0; D],
        }
    }

    fn add_assign(&mut self, o: &Cplx) {
        for j in 0..D {
            self.re[j] += o.re[j];
            self.im[j] += o.im[j];
        }
    }

    fn sub_assign(&mut self, o: &Cplx) {
        for j in 0..D {
            self.re[j] -= o.re[j];
            self.im[j] -= o.im[j];
        }
    }
}

// ---------------------------------------------------------------------------
// Álgebra de fase: ρ (permutação), binding, similaridade, fasores
// ---------------------------------------------------------------------------

/// `ρ^k(v)[j] = v[(j+k) mod D]` — deslocamento cíclico **de posições**.
///
/// É uma permutação, não uma rotação global: `ρ` quebra a comutatividade do
/// binding (`ρ^a(x) ⊗ ρ^b(y) ≠ ρ^b(x) ⊗ ρ^a(y)`) e não cancela no unbinding.
#[inline]
fn rho(v: &Phases, k: usize) -> Phases {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = v[(j + k) % D];
    }
    out
}

/// `ρ^{-k}` — a inversa exata da permutação acima.
#[inline]
fn unrho(v: &Phases, k: usize) -> Phases {
    let k = k % D;
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = v[(j + D - k) % D];
    }
    out
}

/// Binding FHRR: produto de Hadamard = soma de fases (`|z| = 1` preservado).
#[inline]
fn bind(a: &Phases, b: &Phases) -> Phases {
    let mut out = [0u16; D];
    for (j, o) in out.iter_mut().enumerate() {
        *o = a[j].wrapping_add(b[j]);
    }
    out
}

/// `(1/D)·Σ tri(θ_a − θ_b)` em Q14, com `tri(x) = 1 − 2|x|/π`.
///
/// Usada só onde já se tem fase pura (diagnóstico e o controle comutativo).
#[inline]
fn tri_sim(a: &Phases, b: &Phases) -> f32 {
    let mut acc = [0i32; LANES];
    for (ca, cb) in a.chunks_exact(LANES).zip(b.chunks_exact(LANES)) {
        for l in 0..LANES {
            let s = ca[l].wrapping_sub(cb[l]) as i16;
            let ax = s ^ (s >> 15); // |s|
            acc[l] += (Q14 as i16 - ax) as i32;
        }
    }
    let sum: i32 = acc.iter().sum();
    sum as f32 / (Q14 * D as f32)
}

/// Fasor unitário `e^{iθ}` em `(re, im)` — `poly_sin_q14`, vetorizado.
#[inline]
fn unit_phasor(phases: &Phases) -> Cplx {
    let mut out = Cplx::zero();
    let inv = 1.0 / Q14;
    for lane in 0..D / LANES {
        let base = lane * LANES;
        let mut sa = [0i16; LANES];
        let mut ca = [0i16; LANES];
        for l in 0..LANES {
            let p = phases[base + l];
            sa[l] = p as i16;
            ca[l] = p.wrapping_add(QUARTER) as i16;
        }
        let s = poly_sin_q14(&sa);
        let c = poly_sin_q14(&ca);
        for l in 0..LANES {
            out.im[base + l] = s[l] as f32 * inv;
            out.re[base + l] = c[l] as f32 * inv;
        }
    }
    out
}

/// `(1/D)·Re⟨cue, e^{iθ_atom}⟩` — similaridade no domínio complexo.
#[inline]
fn cplx_sim(cue: &Cplx, cos: &[f32; D], sin: &[f32; D]) -> f32 {
    let mut acc = [0.0f32; LANES];
    for lane in 0..D / LANES {
        let base = lane * LANES;
        for l in 0..LANES {
            acc[l] += cue.re[base + l] * cos[base + l] + cue.im[base + l] * sin[base + l];
        }
    }
    let sum: f32 = acc.iter().sum();
    sum / D as f32
}

// ---------------------------------------------------------------------------
// Codebooks + âncoras (cópia limpa e cópia ACF perturbada)
// ---------------------------------------------------------------------------

/// Um vocabulário de papel: fases (escrita) + `cos/sin` pré-computados
/// (cleanup quente).
#[derive(Debug)]
struct Book {
    phases: Vec<BVec>,
    cos: Vec<[f32; D]>,
    sin: Vec<[f32; D]>,
}

impl Book {
    fn new(phases: Vec<BVec>) -> Self {
        let cos = phases
            .iter()
            .map(|v| {
                let p = unit_phasor(v.as_slice());
                p.re
            })
            .collect();
        let sin = phases
            .iter()
            .map(|v| unit_phasor(v.as_slice()).im)
            .collect();
        Self { phases, cos, sin }
    }

    fn len(&self) -> usize {
        self.phases.len()
    }

    fn memory_bytes(&self) -> usize {
        self.phases.len() * std::mem::size_of::<BVec>()
            + self.cos.len() * std::mem::size_of::<[f32; D]>()
            + self.sin.len() * std::mem::size_of::<[f32; D]>()
    }
}

/// Vocabulários de papel. `ent` serve sujeito **e** objeto; `verb` é a relação;
/// `modu` é o modificador.
#[derive(Debug)]
struct Codebooks {
    ent: Book,
    verb: Book,
    modu: Book,
    ent_acf: Book,
    verb_acf: Book,
    modu_acf: Book,
}

impl Codebooks {
    fn build(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let ent = random_codebook(N_ENT, &mut rng);
        let verb = random_codebook(N_VERB, &mut rng);
        let modu = random_codebook(N_MOD, &mut rng);
        // ACF: cópia perturbada determinística (~4% de π por fasor).
        let ent_acf = jitter_codebook(&ent, &mut rng, JITTER_FRAC);
        let verb_acf = jitter_codebook(&verb, &mut rng, JITTER_FRAC);
        let modu_acf = jitter_codebook(&modu, &mut rng, JITTER_FRAC);
        Self {
            ent: Book::new(ent),
            verb: Book::new(verb),
            modu: Book::new(modu),
            ent_acf: Book::new(ent_acf),
            verb_acf: Book::new(verb_acf),
            modu_acf: Book::new(modu_acf),
        }
    }

    fn clean(&self, role: usize) -> &Book {
        match role {
            ROLE_S | ROLE_O => &self.ent,
            ROLE_V => &self.verb,
            _ => &self.modu,
        }
    }

    fn acf(&self, role: usize) -> &Book {
        match role {
            ROLE_S | ROLE_O => &self.ent_acf,
            ROLE_V => &self.verb_acf,
            _ => &self.modu_acf,
        }
    }

    fn memory_bytes(&self) -> usize {
        self.ent.memory_bytes()
            + self.verb.memory_bytes()
            + self.modu.memory_bytes()
            + self.ent_acf.memory_bytes()
            + self.verb_acf.memory_bytes()
            + self.modu_acf.memory_bytes()
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
    let amp = frac * std::f32::consts::PI;
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
#[derive(Debug)]
struct Anchors {
    s: BVec,
    v: BVec,
    o: BVec,
    m: BVec,
}

impl Anchors {
    fn build(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xA0C_4000);
        let a = random_codebook(4, &mut rng);
        Self {
            s: a[0].clone(),
            v: a[1].clone(),
            o: a[2].clone(),
            m: a[3].clone(),
        }
    }

    fn of(&self, role: usize) -> &BVec {
        match role {
            ROLE_S => &self.s,
            ROLE_V => &self.v,
            ROLE_O => &self.o,
            _ => &self.m,
        }
    }

    fn memory_bytes(&self) -> usize {
        4 * std::mem::size_of::<BVec>()
    }
}

/// Termo `A_role ⊗ ρ^role(filler)` (uma fase pura).
fn role_term(anchor: &BVec, filler: &BVec, k: usize) -> Phases {
    bind(anchor.as_slice(), &rho(filler.as_slice(), k))
}

// ---------------------------------------------------------------------------
// Cleanup complexo
// ---------------------------------------------------------------------------

/// Resultado de um *cleanup memory*.
#[derive(Clone, Copy, Debug)]
struct Cleanup {
    index: usize,
    sim: f32,
    margin: f32,
}

/// Varre o codebook inteiro e devolve o vencedor + margem. Zero alocação.
fn cleanup(cue: &Cplx, book: &Book, sims: &mut [f32; N_ENT]) -> Cleanup {
    let mut best = 0usize;
    let mut best_sim = f32::NEG_INFINITY;
    let mut second = f32::NEG_INFINITY;
    for i in 0..book.len() {
        let s = cplx_sim(cue, &book.cos[i], &book.sin[i]);
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

// ---------------------------------------------------------------------------
// Shard holográfico (acumulador de `K` traços, 1 termo por fato)
// ---------------------------------------------------------------------------

/// Um acumulador: `≤ 48` fatos `key ⊗ value`, `K` traços com fase por
/// `(fato, traço)`. Guarda-se a fase `key ⊗ value` (produto, 1 termo por fato)
/// — é exatamente o regime que o E1b mediu; a forma em superposição
/// (`⊕` de papéis) é reconstruída pelo ressonador na leitura.
#[derive(Debug)]
struct Shard {
    traces: usize,
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
    fn new(traces: usize, seed: u64) -> Self {
        Self {
            traces,
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

    fn len(&self) -> usize {
        self.subj.len()
    }

    fn add(&mut self, key: &Phases, value: &Phases, ids: (u16, u16, u16, u16)) {
        let lut = BamLut::global();
        for t in 0..self.traces {
            let psi = if t == 0 {
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
        self.subj.push(ids.0);
        self.verb.push(ids.1);
        self.obj.push(ids.2);
        self.modu.push(ids.3);
    }

    /// Requantiza os traços para `i16` com 12 bits de folga e libera os
    /// acumuladores de construção (fim da fase de escrita).
    fn finalize(&mut self) {
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
    fn find(&self, subj: u16, verb: u16) -> Option<usize> {
        self.subj
            .iter()
            .zip(&self.verb)
            .position(|(&s, &v)| s == subj && v == verb)
    }

    /// Desvincula `key` do fato `fact` nos `K` traços e devolve `z` **já
    /// normalizado** para que o sinal limpo tenha magnitude 1 por posição.
    fn unbind(&self, fact: usize, key: &Phases, out: &mut Cplx) {
        out.re.fill(0.0);
        out.im.fill(0.0);
        // trace ≈ 32767·2^{-shift}·Σ_k cos ; poly_sin devolve Q14.
        let denom = self.traces as f32 * 32_767.0 / ((1u32 << self.shift) as f32);
        let inv = 1.0 / (denom * Q14);
        for t in 0..self.traces {
            let psi = self.psi[fact * self.traces + t];
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
                    // (mr + i·mi)·(cc − i·ss)
                    out.re[base + l] += mr * cc + mi * ss;
                    out.im[base + l] += mi * cc - mr * ss;
                }
            }
        }
    }

    fn memory_bytes(&self) -> usize {
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
///
/// Sem lookup: a mesma função é aplicada na escrita e na leitura, então a
/// consulta só toca **um** shard (working set de 1 acumulador).
#[inline]
fn shard_of(subj: usize, verb: usize) -> usize {
    (subj.wrapping_mul(7).wrapping_add(verb.wrapping_mul(5))) % SHARDS
}

#[derive(Debug)]
struct PlhMemory {
    shards: Vec<Shard>,
}

impl PlhMemory {
    fn new(traces: usize, seed: u64) -> Self {
        Self {
            shards: (0..SHARDS)
                .map(|i| Shard::new(traces, seed ^ (i as u64 + 1).wrapping_mul(0x9E37_79B9)))
                .collect(),
        }
    }

    /// Grava um fato no shard endereçado por `(subj, verb)`.
    fn add(&mut self, subj: u16, verb: u16, obj: u16, modu: u16, key: &Phases, value: &Phases) {
        let idx = shard_of(subj as usize, verb as usize);
        self.shards[idx].add(key, value, (subj, verb, obj, modu));
    }

    fn finalize(&mut self) {
        for s in &mut self.shards {
            s.finalize();
        }
    }

    fn max_load(&self) -> usize {
        self.shards.iter().map(Shard::len).max().unwrap_or(0)
    }

    fn memory_bytes(&self) -> usize {
        self.shards.iter().map(Shard::memory_bytes).sum()
    }
}

// ---------------------------------------------------------------------------
// Ressonador ACF de 2 fatores (predicado OBJ ⊕ MOD do fato)
// ---------------------------------------------------------------------------

/// Scratch pré-alocado do caminho quente (nada aqui aloca depois do `new`).
#[derive(Debug)]
struct QueryScratch {
    z: Cplx,
    f_obj: Cplx,
    f_mod: Cplx,
    rest: Cplx,
    sims: [f32; N_ENT],
}

impl QueryScratch {
    fn new() -> Self {
        Self {
            z: Cplx::zero(),
            f_obj: Cplx::zero(),
            f_mod: Cplx::zero(),
            rest: Cplx::zero(),
            sims: [0.0; N_ENT],
        }
    }
}

/// Fatores do predicado (2 fatores) devolvidos pelo ressonador.
#[derive(Clone, Copy, Debug)]
struct PairFactors {
    obj: usize,
    modu: usize,
    sim_obj: f32,
    sim_mod: f32,
    margin_obj: f32,
    margin_mod: f32,
    iters: u8,
    converged: bool,
    validated: bool,
}

/// Desvincula um papel de um complexo: `f = ρ^{-k}( z ⊙ conj(A ⊗ other) )`.
///
/// `other` é a fase do outro fator já re-codificada (átomo **limpo**).
fn unbind_role(z: &Cplx, anchor: &BVec, k: usize, other: Option<&Phases>, out: &mut Cplx) {
    let amp = anchor.as_slice();
    let lut = BamLut::global();
    for j in 0..D {
        let mut phi = amp[j];
        if let Some(o) = other {
            phi = phi.wrapping_add(o[j]);
        }
        let (s, c) = lut.sin_cos(BamPhase(phi));
        // z · (c − i·s)
        out.re[j] = z.re[j] * c + z.im[j] * s;
        out.im[j] = z.im[j] * c - z.re[j] * s;
    }
    // ρ^{-k}: desfaz a permutação do papel.
    let k = k % D;
    let tmp = *out;
    for j in 0..D {
        let src = (j + D - k) % D;
        out.re[j] = tmp.re[src];
        out.im[j] = tmp.im[src];
    }
}

/// Ressonador de `(OBJ, MOD)` com projeção *re-encode* entre passos.
///
/// Cada passo subtrai do predicado o **átomo limpo** do outro fator
/// (`re-encode`), desvincula o papel próprio e faz cleanup na **cópia ACF**;
/// no fim valida o vencedor na **cópia limpa**.
fn resonate_pair(value: &Cplx, cb: &Codebooks, a: &Anchors, sc: &mut QueryScratch) -> PairFactors {
    // init: cada fator isolado (sem subtração).
    unbind_role(value, &a.o, ROLE_O, None, &mut sc.f_obj);
    unbind_role(value, &a.m, ROLE_M, None, &mut sc.f_mod);
    let mut o = cleanup(&sc.f_obj, &cb.ent_acf, &mut sc.sims).index;
    let mut m = cleanup(&sc.f_mod, &cb.modu_acf, &mut sc.sims).index;

    let mut iters = 0u8;
    let mut converged = false;
    for it in 0..PAIR_ITERS {
        iters = it as u8 + 1;
        let prev = (o, m);

        // OBJ: subtrai o re-encode limpo do MOD.
        let term_m = unit_phasor(&bind(
            a.m.as_slice(),
            &rho(cb.modu.phases[m].as_slice(), ROLE_M),
        ));
        sc.rest = *value;
        sc.rest.sub_assign(&term_m);
        let rest = sc.rest;
        unbind_role(&rest, &a.o, ROLE_O, None, &mut sc.f_obj);
        o = cleanup(&sc.f_obj, &cb.ent_acf, &mut sc.sims).index;

        // MOD: subtrai o re-encode limpo do OBJ (já atualizado).
        let term_o = unit_phasor(&bind(
            a.o.as_slice(),
            &rho(cb.ent.phases[o].as_slice(), ROLE_O),
        ));
        sc.rest = *value;
        sc.rest.sub_assign(&term_o);
        let rest = sc.rest;
        unbind_role(&rest, &a.m, ROLE_M, None, &mut sc.f_mod);
        m = cleanup(&sc.f_mod, &cb.modu_acf, &mut sc.sims).index;

        if (o, m) == prev {
            converged = true;
            break;
        }
    }

    // Consistência final: OBJ recalculado com o MOD final.
    let term_m = unit_phasor(&bind(
        a.m.as_slice(),
        &rho(cb.modu.phases[m].as_slice(), ROLE_M),
    ));
    sc.rest = *value;
    sc.rest.sub_assign(&term_m);
    let rest = sc.rest;
    unbind_role(&rest, &a.o, ROLE_O, None, &mut sc.f_obj);

    // Validação na cópia limpa (o certificado do fator).
    let co = cleanup(&sc.f_obj, &cb.ent, &mut sc.sims);
    let cm = cleanup(&sc.f_mod, &cb.modu, &mut sc.sims);
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

// ---------------------------------------------------------------------------
// Codec de sentença (⊕ de 4 papéis) + ressonador ACF de 4 fatores
// ---------------------------------------------------------------------------

/// `Ψ = A_S⊗ρ^0(s) ⊕ A_V⊗ρ^1(v) ⊕ A_O⊗ρ^2(o) ⊕ A_M⊗ρ^3(m)`.
fn encode_sentence(cb: &Codebooks, a: &Anchors, ids: [usize; 4]) -> Cplx {
    let mut psi = Cplx::zero();
    psi.add_assign(&unit_phasor(&role_term(
        a.of(ROLE_S),
        &cb.ent.phases[ids[0]],
        ROLE_S,
    )));
    psi.add_assign(&unit_phasor(&role_term(
        a.of(ROLE_V),
        &cb.verb.phases[ids[1]],
        ROLE_V,
    )));
    psi.add_assign(&unit_phasor(&role_term(
        a.of(ROLE_O),
        &cb.ent.phases[ids[2]],
        ROLE_O,
    )));
    psi.add_assign(&unit_phasor(&role_term(
        a.of(ROLE_M),
        &cb.modu.phases[ids[3]],
        ROLE_M,
    )));
    psi
}

/// Fatores de uma sentença decodificada.
#[derive(Clone, Copy, Debug, Default)]
struct Factors {
    ids: [usize; 4],
    sims: [f32; 4],
    margins: [f32; 4],
    iters: u8,
    converged: bool,
    validated: bool,
}

impl Factors {
    fn min_sim(&self) -> f32 {
        self.sims.iter().copied().fold(f32::INFINITY, f32::min)
    }

    fn min_margin(&self) -> f32 {
        self.margins.iter().copied().fold(f32::INFINITY, f32::min)
    }
}

/// Fatora `Ψ` nos quatro papéis.
///
/// * init: desvincula cada âncora e faz cleanup na cópia **ACF**;
/// * itera: subtrai o `re-encode` (**átomo limpo**) dos outros três papéis,
///   desvincula o papel próprio e faz cleanup na cópia ACF —
///   *re-encode projection* entre passos encadeados;
/// * valida: o vencedor de cada papel tem que sobreviver ao cleanup na cópia
///   **limpa**.
fn factorize_sentence(psi: &Cplx, cb: &Codebooks, a: &Anchors) -> Factors {
    let mut f = Factors::default();
    let mut last: [Cplx; 4] = [Cplx::zero(); 4];
    let mut sims = [0.0f32; N_ENT];

    for r in 0..4 {
        unbind_role(psi, a.of(r), r, None, &mut last[r]);
        let c = cleanup(&last[r], cb.acf(r), &mut sims);
        f.ids[r] = c.index;
        f.sims[r] = c.sim;
        f.margins[r] = c.margin;
    }

    let mut converged = false;
    for it in 0..RES_ITERS {
        f.iters = it as u8 + 1;
        let mut changed = false;
        for r in 0..4 {
            // residual = Ψ − Σ_{r'≠r} reencode(átomo limpo, r')
            let mut res = *psi;
            for r2 in 0..4 {
                if r2 == r {
                    continue;
                }
                let term = unit_phasor(&role_term(
                    a.of(r2),
                    &cb.clean(r2).phases[f.ids[r2]],
                    r2,
                ));
                res.sub_assign(&term);
            }
            unbind_role(&res, a.of(r), r, None, &mut last[r]);
            let c = cleanup(&last[r], cb.acf(r), &mut sims);
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
        let c = cleanup(&last[r], cb.clean(r), &mut sims);
        if c.index != f.ids[r] {
            validated = false;
        }
    }
    f.converged = converged;
    f.validated = validated;
    f
}

// ---------------------------------------------------------------------------
// Abstenção calibrada
// ---------------------------------------------------------------------------

/// Evidência da leitura: pior similaridade e pior margem entre os fatores.
#[derive(Clone, Copy, Debug)]
struct Evidence {
    min_sim: f32,
    min_margin: f32,
}

impl Evidence {
    fn from_factors(f: &Factors) -> Self {
        Self {
            min_sim: f.min_sim(),
            min_margin: f.min_margin(),
        }
    }

    fn from_pair(p: &PairFactors) -> Self {
        Self {
            min_sim: p.sim_obj.min(p.sim_mod),
            min_margin: p.margin_obj.min(p.margin_mod),
        }
    }
}

/// Abstenção seletiva: o readout só vale se a evidência observada o sustenta.
#[inline]
fn should_abstain(e: &Evidence) -> bool {
    e.min_sim < SIM_FLOOR || e.min_margin < MARGIN_FLOOR
}

// ---------------------------------------------------------------------------
// Construção do mundo
// ---------------------------------------------------------------------------

/// Um fato do mundo: `(sujeito, verbo, objeto, modificador)`.
#[derive(Clone, Copy, Debug)]
struct Fact {
    ids: [u16; 4],
}

impl Fact {
    fn key(&self, cb: &Codebooks, a: &Anchors) -> Phases {
        let ks = role_term(a.of(ROLE_S), &cb.ent.phases[self.ids[0] as usize], ROLE_S);
        let kv = role_term(a.of(ROLE_V), &cb.verb.phases[self.ids[1] as usize], ROLE_V);
        bind(&ks, &kv)
    }

    fn value(&self, cb: &Codebooks, a: &Anchors) -> Phases {
        let ko = role_term(a.of(ROLE_O), &cb.ent.phases[self.ids[2] as usize], ROLE_O);
        let km = role_term(a.of(ROLE_M), &cb.modu.phases[self.ids[3] as usize], ROLE_M);
        bind(&ko, &km)
    }
}

/// Mundo determinístico de `n` fatos com `(sujeito, verbo)` únicos.
fn build_world(n: usize) -> Vec<Fact> {
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

/// Constrói a memória sharded a partir do mundo (≤ 48 fatos por acumulador).
fn build_sharded(
    facts: &[Fact],
    cb: &Codebooks,
    a: &Anchors,
    traces: usize,
    seed: u64,
) -> PlhMemory {
    let mut mem = PlhMemory::new(traces, seed);
    for f in facts {
        mem.add(
            f.ids[0],
            f.ids[1],
            f.ids[2],
            f.ids[3],
            &f.key(cb, a),
            &f.value(cb, a),
        );
    }
    mem.finalize();
    mem
}

/// Constrói **uma** superposição plana com todos os fatos (o controle que o
/// E1b mostrou colapsar) — o mesmo `Shard`, sem sharding.
fn build_flat(facts: &[Fact], cb: &Codebooks, a: &Anchors, traces: usize) -> Shard {
    let mut s = Shard::new(traces, 0xF1A7_0001);
    for f in facts {
        s.add(
            &f.key(cb, a),
            &f.value(cb, a),
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
/// roda o ressonador ACF. **Zero alocação.**
fn hot_query(
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
    shard.unbind(fact, &key, &mut sc.z);
    let z = sc.z;
    Some(resonate_pair(&z, cb, a, sc))
}

/// Controle: a mesma pipeline sobre **uma** superposição plana.
fn hot_query_flat(
    flat: &Shard,
    cb: &Codebooks,
    a: &Anchors,
    f: &Fact,
    sc: &mut QueryScratch,
) -> Option<PairFactors> {
    let fact = flat.find(f.ids[0], f.ids[1])?;
    let key = f.key(cb, a);
    flat.unbind(fact, &key, &mut sc.z);
    let z = sc.z;
    Some(resonate_pair(&z, cb, a, sc))
}

// ---------------------------------------------------------------------------
// Gates
// ---------------------------------------------------------------------------

/// Resultado agregado do gate de escala.
#[derive(Clone, Copy, Debug, Default)]
struct ScaleResult {
    recall: f64,
    snr_obj: f64,
    snr_mod: f64,
    mean_us: f64,
    p50_us: f64,
    p99_us: f64,
    allocs: usize,
    acf_validation: f64,
    converged: f64,
    false_abstain: f64,
}

/// `(μ_sinal − μ_ruído) / σ_ruído`.
fn snr(signal: &[f32], noise: &[f32]) -> f64 {
    if signal.is_empty() || noise.is_empty() {
        return f64::NAN;
    }
    let ms = signal.iter().map(|&x| x as f64).sum::<f64>() / signal.len() as f64;
    let mn = noise.iter().map(|&x| x as f64).sum::<f64>() / noise.len() as f64;
    let var = noise
        .iter()
        .map(|&x| {
            let d = x as f64 - mn;
            d * d
        })
        .sum::<f64>()
        / noise.len() as f64;
    let sd = var.sqrt();
    if sd <= 1e-12 {
        f64::INFINITY
    } else {
        (ms - mn) / sd
    }
}

/// Gate 1/3: 1.000 fatos sharded, com SNR, latência e 0-alloc no caminho quente.
fn gate_scale(
    mem: &PlhMemory,
    facts: &[Fact],
    cb: &Codebooks,
    a: &Anchors,
    n_queries: usize,
) -> ScaleResult {
    let mut sc = QueryScratch::new();
    let mut picks = Vec::with_capacity(n_queries);
    for k in 0..n_queries {
        picks.push((k * facts.len()) / n_queries.max(1));
    }

    let mut lat = Vec::with_capacity(n_queries);
    let mut sig_obj: Vec<f32> = Vec::with_capacity(n_queries);
    let mut sig_mod: Vec<f32> = Vec::with_capacity(n_queries);
    let mut noise_obj: Vec<f32> = Vec::with_capacity(n_queries * N_ENT);
    let mut noise_mod: Vec<f32> = Vec::with_capacity(n_queries * N_MOD);
    let mut hits = 0usize;
    let mut validated = 0usize;
    let mut converged = 0usize;
    let mut false_abstain = 0usize;

    // warmup: inicializa a LUT e aquece o caminho (fora da contagem)
    for _ in 0..16 {
        black_box(hot_query(mem, cb, a, &facts[picks[0]], &mut sc));
    }

    alloc_probe::arm();
    for &pi in &picks {
        let f = &facts[pi];
        let t0 = Instant::now();
        let answer = hot_query(mem, cb, a, f, &mut sc);
        let dt = t0.elapsed().as_nanos() as f64 / 1000.0;
        lat.push(dt);

        let truth_obj = f.ids[2] as usize;
        let truth_mod = f.ids[3] as usize;
        match answer {
            Some(p) => {
                if p.obj == truth_obj && p.modu == truth_mod {
                    hits += 1;
                }
                if p.validated {
                    validated += 1;
                }
                if p.converged {
                    converged += 1;
                }
                if should_abstain(&Evidence::from_pair(&p)) {
                    false_abstain += 1;
                }
                let _ = cleanup(&sc.f_obj, &cb.ent, &mut sc.sims);
                sig_obj.push(sc.sims[truth_obj]);
                for (i, s) in sc.sims.iter().enumerate().take(N_ENT) {
                    if i != truth_obj {
                        noise_obj.push(*s);
                    }
                }
                let _ = cleanup(&sc.f_mod, &cb.modu, &mut sc.sims);
                sig_mod.push(sc.sims[truth_mod]);
                for (i, s) in sc.sims.iter().enumerate().take(N_MOD) {
                    if i != truth_mod {
                        noise_mod.push(*s);
                    }
                }
            }
            None => false_abstain += 1,
        }
    }
    let allocs = alloc_probe::disarm();

    lat.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mean = lat.iter().sum::<f64>() / lat.len() as f64;
    let p = |q: f64| lat[((lat.len() as f64 * q) as usize).min(lat.len() - 1)];

    ScaleResult {
        recall: hits as f64 / picks.len() as f64,
        snr_obj: snr(&sig_obj, &noise_obj),
        snr_mod: snr(&sig_mod, &noise_mod),
        mean_us: mean,
        p50_us: p(0.50),
        p99_us: p(0.99),
        allocs,
        acf_validation: validated as f64 / picks.len() as f64,
        converged: converged as f64 / picks.len() as f64,
        false_abstain: false_abstain as f64 / picks.len() as f64,
    }
}

/// Recall@1 do controle plano (mesma pipeline, um único acumulador).
fn flat_recall(flat: &Shard, facts: &[Fact], cb: &Codebooks, a: &Anchors, n_queries: usize) -> f64 {
    let mut sc = QueryScratch::new();
    let mut hits = 0usize;
    for k in 0..n_queries {
        let pi = (k * facts.len()) / n_queries.max(1);
        let f = &facts[pi];
        if let Some(p) = hot_query_flat(flat, cb, a, f, &mut sc) {
            if p.obj == f.ids[2] as usize && p.modu == f.ids[3] as usize {
                hits += 1;
            }
        }
    }
    hits as f64 / n_queries as f64
}

/// Resultado do gate de não-comutatividade.
#[derive(Clone, Copy, Debug, Default)]
struct CommutResult {
    pairs: usize,
    separated: usize,
    decoded_forward: usize,
    decoded_reverse: usize,
    mean_sim: f64,
    control_sim: f64,
    acf_validated: usize,
}

/// Gate 2: `A → B` vs `B → A` com o mesmo verbo e o mesmo modificador.
fn gate_non_commutativity(
    cb: &Codebooks,
    a: &Anchors,
    pairs: usize,
    rng: &mut ChaCha8Rng,
) -> CommutResult {
    let mut out = CommutResult::default();
    let mut sim_acc = 0.0f64;
    for k in 0..pairs {
        let x = k % N_ENT;
        let mut y = rng.gen_range(0..N_ENT);
        if y == x {
            y = (y + 1) % N_ENT;
        }
        let v = rng.gen_range(0..N_VERB);
        let m = rng.gen_range(0..N_MOD);

        let fwd = encode_sentence(cb, a, [x, v, y, m]);
        let rev = encode_sentence(cb, a, [y, v, x, m]);

        let mut dot = 0.0f64;
        let mut nx = 0.0f64;
        let mut ny = 0.0f64;
        for j in 0..D {
            dot += (fwd.re[j] * rev.re[j] + fwd.im[j] * rev.im[j]) as f64;
            nx += (fwd.re[j] * fwd.re[j] + fwd.im[j] * fwd.im[j]) as f64;
            ny += (rev.re[j] * rev.re[j] + rev.im[j] * rev.im[j]) as f64;
        }
        let sim = dot / (nx.sqrt() * ny.sqrt()).max(1e-12);
        sim_acc += sim;

        let df = factorize_sentence(&fwd, cb, a);
        let dr = factorize_sentence(&rev, cb, a);

        let fwd_ok = df.ids[ROLE_S] == x
            && df.ids[ROLE_V] == v
            && df.ids[ROLE_O] == y
            && df.ids[ROLE_M] == m;
        let rev_ok = dr.ids[ROLE_S] == y
            && dr.ids[ROLE_V] == v
            && dr.ids[ROLE_O] == x
            && dr.ids[ROLE_M] == m;
        if fwd_ok {
            out.decoded_forward += 1;
        }
        if rev_ok {
            out.decoded_reverse += 1;
        }
        // separação operacional: código distinto **e** leitura trocada correta
        if fwd_ok && rev_ok && sim < 0.75 {
            out.separated += 1;
        }
        if df.validated && dr.validated {
            out.acf_validated += 1;
        }
    }

    // Controle: binding "comutativo" — mesmo papel para os dois argumentos e
    // sem permutação. É o que a construção ρ^k + âncoras distintas evita.
    let control = pairs.min(32);
    let mut ctrl = 0.0f64;
    for k in 0..control {
        let x = k % N_ENT;
        let y = (k * 13 + 1) % N_ENT;
        let v = k % N_VERB;
        let one = bind(a.s.as_slice(), cb.verb.phases[v].as_slice());
        let cx = bind(
            &one,
            &bind(cb.ent.phases[x].as_slice(), cb.ent.phases[y].as_slice()),
        );
        let cy = bind(
            &one,
            &bind(cb.ent.phases[y].as_slice(), cb.ent.phases[x].as_slice()),
        );
        ctrl += tri_sim(&cx, &cy) as f64;
    }
    out.pairs = pairs;
    out.mean_sim = sim_acc / pairs as f64;
    out.control_sim = ctrl / control as f64;
    out
}

/// Linha da tabela de abstenção.
#[derive(Clone, Copy, Debug)]
struct AbstRow {
    label: &'static str,
    n: usize,
    abstain: f64,
    accuracy: f64,
    mean_sim: f64,
    mean_margin: f64,
}

/// Gate 4: abstenção em OOV e sob ruído de fase crescente.
fn gate_abstention(cb: &Codebooks, a: &Anchors, n: usize, rng: &mut ChaCha8Rng) -> Vec<AbstRow> {
    let mut rows = Vec::new();
    let noise_levels: [(f32, &'static str); 5] = [
        (0.0, "limpo"),
        (0.4, "sigma=0,4"),
        (0.8, "sigma=0,8"),
        (1.2, "sigma=1,2"),
        (1.6, "sigma=1,6"),
    ];

    for (sigma, label) in noise_levels {
        let mut abst = 0usize;
        let mut correct = 0usize;
        let mut decided = 0usize;
        let mut sim_acc = 0.0f64;
        let mut mar_acc = 0.0f64;
        for _ in 0..n {
            let ids = [
                rng.gen_range(0..N_ENT),
                rng.gen_range(0..N_VERB),
                rng.gen_range(0..N_ENT),
                rng.gen_range(0..N_MOD),
            ];
            let mut psi = encode_sentence(cb, a, ids);
            if sigma > 0.0 {
                // ruído de fase uniforme com o mesmo σ (amp = σ·√3)
                let amp = sigma * 1.732_050_8;
                for j in 0..D {
                    let d = rng.gen_range(-amp..amp);
                    let (s, c) = (d.sin(), d.cos());
                    let (re, im) = (psi.re[j], psi.im[j]);
                    psi.re[j] = re * c - im * s;
                    psi.im[j] = re * s + im * c;
                }
            }
            let f = factorize_sentence(&psi, cb, a);
            let e = Evidence::from_factors(&f);
            sim_acc += e.min_sim as f64;
            mar_acc += e.min_margin as f64;
            if should_abstain(&e) {
                abst += 1;
            } else {
                decided += 1;
                if f.ids == ids {
                    correct += 1;
                }
            }
        }
        rows.push(AbstRow {
            label,
            n,
            abstain: abst as f64 / n as f64,
            accuracy: if decided > 0 {
                correct as f64 / decided as f64
            } else {
                f64::NAN
            },
            mean_sim: sim_acc / n as f64,
            mean_margin: mar_acc / n as f64,
        });
    }

    // OOV: Ψ montada com átomos que não existem em nenhum codebook.
    let mut abst = 0usize;
    let mut sim_acc = 0.0f64;
    let mut mar_acc = 0.0f64;
    for _ in 0..n {
        let mut b = Cplx::zero();
        for r in 0..4 {
            let mut p = [0.0f32; D];
            for x in p.iter_mut() {
                *x = rng.gen::<f32>() * TAU;
            }
            let atom = BVec::from_radians(&p).expect("D");
            b.add_assign(&unit_phasor(&role_term(a.of(r), &atom, r)));
        }
        let f = factorize_sentence(&b, cb, a);
        let e = Evidence::from_factors(&f);
        sim_acc += e.min_sim as f64;
        mar_acc += e.min_margin as f64;
        if should_abstain(&e) {
            abst += 1;
        }
    }
    rows.push(AbstRow {
        label: "fora do vocabulario",
        n,
        abstain: abst as f64 / n as f64,
        accuracy: f64::NAN,
        mean_sim: sim_acc / n as f64,
        mean_margin: mar_acc / n as f64,
    });

    rows
}

/// Gate 4 (braço memória): unbinding com uma chave **OOV** num fato existente.
///
/// O endereço é real (o shard e o `ψ` são conhecidos) mas a chave não
/// corresponde a nenhum fato — o `z` é ruído puro, e a evidência tem que cair.
fn abstention_memory(
    mem: &PlhMemory,
    facts: &[Fact],
    cb: &Codebooks,
    a: &Anchors,
    n: usize,
    rng: &mut ChaCha8Rng,
) -> (f64, f64) {
    let mut sc = QueryScratch::new();
    let mut abst = 0usize;
    let mut sim_acc = 0.0f64;
    for _ in 0..n {
        let f = &facts[rng.gen_range(0..facts.len())];
        let idx = shard_of(f.ids[0] as usize, f.ids[1] as usize);
        let shard = &mem.shards[idx];
        let fact = match shard.find(f.ids[0], f.ids[1]) {
            Some(x) => x,
            None => continue,
        };
        // chave OOV: dois átomos aleatórios que não pertencem ao codebook
        let mut k1 = [0.0f32; D];
        let mut k2 = [0.0f32; D];
        for j in 0..D {
            k1[j] = rng.gen::<f32>() * TAU;
            k2[j] = rng.gen::<f32>() * TAU;
        }
        let oov = bind(
            &role_term(a.of(ROLE_S), &BVec::from_radians(&k1).expect("D"), ROLE_S),
            &role_term(a.of(ROLE_V), &BVec::from_radians(&k2).expect("D"), ROLE_V),
        );
        shard.unbind(fact, &oov, &mut sc.z);
        let z = sc.z;
        let p = resonate_pair(&z, cb, a, &mut sc);
        let e = Evidence::from_pair(&p);
        sim_acc += e.min_sim as f64;
        if should_abstain(&e) {
            abst += 1;
        }
    }
    (abst as f64 / n as f64, sim_acc / n as f64)
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct Args {
    quick: bool,
    facts: usize,
    queries: usize,
    seed: u64,
}

fn parse_args() -> Args {
    let mut a = Args {
        quick: false,
        facts: N_FACTS,
        queries: QUERIES,
        seed: 0xE1_2026,
    };
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--quick" => {
                a.quick = true;
                a.queries = 120;
            }
            "--facts" => {
                if let Some(v) = argv.get(i + 1).and_then(|v| v.parse().ok()) {
                    a.facts = v;
                    i += 1;
                }
            }
            "--queries" => {
                if let Some(v) = argv.get(i + 1).and_then(|v| v.parse().ok()) {
                    a.queries = v;
                    i += 1;
                }
            }
            "--seed" => {
                if let Some(v) = argv.get(i + 1).and_then(|v| v.parse().ok()) {
                    a.seed = v;
                    i += 1;
                }
            }
            other => eprintln!("argumento ignorado: {other}"),
        }
        i += 1;
    }
    a
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();
    let n_facts = args.facts.min(N_ENT * N_VERB);
    let n_q = args.queries;

    let cb = Codebooks::build(args.seed);
    let a = Anchors::build(args.seed);
    let facts = build_world(n_facts);
    let mem = build_sharded(&facts, &cb, &a, K_TRACES, args.seed);
    let flat = build_flat(&facts, &cb, &a, K_TRACES);

    println!();
    println!("AETHER · E1 — Phase-Language Head sharded + ressonador ACF");
    println!("===========================================================");
    println!(
        "D={D} K={K_TRACES} shards={SHARDS} fatos={} entidades={} verbos={} mods={} seed={:#x}",
        facts.len(),
        N_ENT,
        N_VERB,
        N_MOD,
        args.seed
    );
    println!(
        "teto medido (E1b) = 0,4·D = {} fatos/acumulador · ocupação máxima = {} (≤ {SHARD_CAP})",
        (0.4 * D as f64).round() as usize,
        mem.max_load()
    );
    println!();

    // ---------------- Gate 1 + 3 ----------------
    let scale = gate_scale(&mem, &facts, &cb, &a, n_q);
    let flat = flat_recall(&flat, &facts, &cb, &a, n_q.min(200));

    println!("G1 — escala sharded (D={D}, {K_TRACES} traços)");
    println!("{:<38} {:>12} {:>12}", "", "sharded", "plano");
    println!("{}", "-".repeat(64));
    println!(
        "{:<38} {:>12.4} {:>12.4}",
        "recall@1 (objeto + modificador)", scale.recall, flat
    );
    println!(
        "{:<38} {:>12} {:>12}",
        "fatos por acumulador (máximo)",
        mem.max_load(),
        facts.len()
    );
    println!(
        "{:<38} {:>12} {:>12}",
        "acumuladores independentes", SHARDS, 1
    );
    println!();

    println!("G3 — qualidade e latência (caminho quente sharded)");
    println!(
        "  SNR OBJ {:.2} σ · SNR MOD {:.2} σ",
        scale.snr_obj, scale.snr_mod
    );
    println!(
        "  latência/consulta  média {:.2} µs · p50 {:.2} µs · p99 {:.2} µs",
        scale.mean_us, scale.p50_us, scale.p99_us
    );
    println!(
        "  alocações no hot-path: {} · convergência {:.1}% · validação ACF {:.1}% · falso-abster {:.1}%",
        scale.allocs,
        100.0 * scale.converged,
        100.0 * scale.acf_validation,
        100.0 * scale.false_abstain
    );
    println!();

    // ---------------- Gate 2 ----------------
    let mut rng = ChaCha8Rng::seed_from_u64(args.seed ^ 0xC0_0042);
    let comm = gate_non_commutativity(&cb, &a, if args.quick { 40 } else { 150 }, &mut rng);

    println!("G2 — não-comutatividade estrita (A→B vs B→A)");
    println!("{:<46} {:>10}", "métrica", "valor");
    println!("{}", "-".repeat(58));
    println!("{:<46} {:>10}", "pares testados", comm.pairs);
    println!(
        "{:<46} {:>9.2}%",
        "separação (código distinto + leitura trocada)",
        100.0 * comm.separated as f64 / comm.pairs as f64
    );
    println!(
        "{:<46} {:>9.2}%",
        "decodificação correta A→B",
        100.0 * comm.decoded_forward as f64 / comm.pairs as f64
    );
    println!(
        "{:<46} {:>9.2}%",
        "decodificação correta B→A",
        100.0 * comm.decoded_reverse as f64 / comm.pairs as f64
    );
    println!(
        "{:<46} {:>10.3}",
        "similaridade média Ψ_AB · Ψ_BA", comm.mean_sim
    );
    println!(
        "{:<46} {:>10.3}",
        "controle comutativo (mesmo papel, sem ρ)", comm.control_sim
    );
    println!(
        "{:<46} {:>9.2}%",
        "validação ACF nos dois sentidos",
        100.0 * comm.acf_validated as f64 / comm.pairs as f64
    );
    println!();

    // ---------------- Gate 4 ----------------
    let abst_rows = gate_abstention(&cb, &a, if args.quick { 50 } else { 120 }, &mut rng);
    println!("G4 — abstenção calibrada (piso sim ≥ {SIM_FLOOR:.2}, margem ≥ {MARGIN_FLOOR:.2})");
    println!(
        "{:<22} {:>6} {:>10} {:>10} {:>10} {:>10}",
        "condição", "n", "abstem", "acerto", "sim_min", "margem"
    );
    println!("{}", "-".repeat(74));
    for r in &abst_rows {
        println!(
            "{:<22} {:>6} {:>9.1}% {:>10} {:>10.3} {:>10.3}",
            r.label,
            r.n,
            100.0 * r.abstain,
            if r.accuracy.is_finite() {
                format!("{:.1}%", 100.0 * r.accuracy)
            } else {
                "-".to_string()
            },
            r.mean_sim,
            r.mean_margin
        );
    }
    let (mem_abst, mem_sim) = abstention_memory(&mem, &facts, &cb, &a, 120, &mut rng);
    println!(
        "{:<22} {:>6} {:>9.1}% {:>10} {:>10.3} {:>10.3}",
        "chave OOV (memória)", 120, 100.0 * mem_abst, "-", mem_sim, 0.0
    );
    println!();

    // ---------------- memória ----------------
    let accum_bytes = mem.memory_bytes();
    let codebook_bytes = cb.memory_bytes() + a.memory_bytes();
    let scratch_bytes = std::mem::size_of::<QueryScratch>();
    let total = accum_bytes + codebook_bytes + scratch_bytes;
    println!("pegada de memória (working set)");
    println!(
        "{:<32} {:>10}",
        "acumuladores (24 shards)",
        format!("{} KB", accum_bytes / 1024)
    );
    println!(
        "{:<32} {:>10}",
        "codebooks limpo + ACF + âncoras",
        format!("{} KB", codebook_bytes / 1024)
    );
    println!(
        "{:<32} {:>10}",
        "scratch de consulta",
        format!("{scratch_bytes} B")
    );
    println!("{:<32} {:>10}", "total", format!("{} KB", total / 1024));
    println!();

    // ---------------- gates ----------------
    let g1 = scale.recall >= 0.99;
    let g1_contrast = flat < 0.10;
    let g1_cap = mem.max_load() <= SHARD_CAP;
    let g2 = comm.separated == comm.pairs
        && comm.decoded_forward == comm.pairs
        && comm.decoded_reverse == comm.pairs;
    let g3_snr = scale.snr_obj > 5.0 && scale.snr_mod > 5.0;
    let g3_lat = scale.p50_us < 15.0;
    let g3_alloc = scale.allocs == 0;
    let oov = abst_rows.last().map(|r| r.abstain).unwrap_or(0.0);
    let noisy = abst_rows
        .iter()
        .find(|r| r.label == "sigma=1,6")
        .map(|r| r.abstain)
        .unwrap_or(0.0);
    let clean_row = abst_rows.first().copied().unwrap_or(AbstRow {
        label: "",
        n: 0,
        abstain: 1.0,
        accuracy: 0.0,
        mean_sim: 0.0,
        mean_margin: 0.0,
    });
    let g4_oov = oov > 0.95 && mem_abst > 0.95;
    let g4_noise = noisy > 0.95;
    let g4_clean = clean_row.abstain < 0.05 && clean_row.accuracy >= 0.95;
    let g_mem = total < 500 * 1024;

    println!("gates");
    println!(
        "  G1 recall@1 sharded ≥ 0,99             : {}  ({:.4})",
        pass(g1),
        scale.recall
    );
    println!(
        "  G1 contraste plano < 0,10              : {}  ({:.4})",
        pass(g1_contrast),
        flat
    );
    println!(
        "  G1 ≤ {SHARD_CAP} fatos/acumulador              : {}  (max {})",
        pass(g1_cap),
        mem.max_load()
    );
    println!(
        "  G2 não-comutatividade = 100%           : {}",
        pass(g2)
    );
    println!(
        "  G3 SNR > 5,0 σ (OBJ e MOD)             : {}  ({:.2} / {:.2})",
        pass(g3_snr),
        scale.snr_obj,
        scale.snr_mod
    );
    println!(
        "  G3 p50 < 15 µs por consulta            : {}  ({:.2} µs)",
        pass(g3_lat),
        scale.p50_us
    );
    println!(
        "  G3 0 bytes alocados no hot-path        : {}  ({} allocs)",
        pass(g3_alloc),
        scale.allocs
    );
    println!(
        "  G4 abstenção OOV > 95%                 : {}  (vocab {:.1}% · memória {:.1}%)",
        pass(g4_oov),
        100.0 * oov,
        100.0 * mem_abst
    );
    println!(
        "  G4 abstenção ruído σ=1,6 > 95%         : {}  ({:.1}%)",
        pass(g4_noise),
        100.0 * noisy
    );
    println!(
        "  G4 falso-abster limpo < 5% + acerto    : {}  ({:.1}% / {:.1}%)",
        pass(g4_clean),
        100.0 * clean_row.abstain,
        100.0 * clean_row.accuracy
    );
    println!(
        "  G-working-set < 500 KB                 : {}  ({} KB)",
        pass(g_mem),
        total / 1024
    );
    let all = g1
        && g1_contrast
        && g1_cap
        && g2
        && g3_snr
        && g3_lat
        && g3_alloc
        && g4_oov
        && g4_noise
        && g4_clean
        && g_mem;
    println!("  veredito: {}", pass(all));
    println!();

    // ---------------- artefato ----------------
    std::fs::create_dir_all("target").ok();
    let path = "target/aether_e1_plh.json";
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!(
        "  \"dim\": {D}, \"traces\": {K_TRACES}, \"shards\": {SHARDS}, \"shard_cap\": {SHARD_CAP},\n"
    ));
    json.push_str(&format!(
        "  \"facts\": {}, \"max_load\": {}, \"n_ent\": {N_ENT}, \"n_verb\": {N_VERB}, \"n_mod\": {N_MOD},\n",
        facts.len(),
        mem.max_load()
    ));
    json.push_str(&format!(
        "  \"sharded\": {{\"recall\": {:.4}, \"snr_obj\": {:.3}, \"snr_mod\": {:.3}, \"p50_us\": {:.3}, \"p99_us\": {:.3}, \"mean_us\": {:.3}, \"allocs\": {}, \"acf_validation\": {:.4}, \"converged\": {:.4}, \"false_abstain\": {:.4}}},\n",
        scale.recall,
        scale.snr_obj,
        scale.snr_mod,
        scale.p50_us,
        scale.p99_us,
        scale.mean_us,
        scale.allocs,
        scale.acf_validation,
        scale.converged,
        scale.false_abstain
    ));
    json.push_str(&format!("  \"flat\": {{\"recall\": {flat:.4}}},\n"));
    json.push_str(&format!(
        "  \"noncommutative\": {{\"pairs\": {}, \"separated\": {}, \"forward\": {}, \"reverse\": {}, \"mean_sim\": {:.4}, \"control_sim\": {:.4}}},\n",
        comm.pairs,
        comm.separated,
        comm.decoded_forward,
        comm.decoded_reverse,
        comm.mean_sim,
        comm.control_sim
    ));
    json.push_str(&format!(
        "  \"memory_bytes\": {{\"accumulators\": {accum_bytes}, \"codebooks\": {codebook_bytes}, \"scratch\": {scratch_bytes}, \"total\": {total}}},\n"
    ));
    json.push_str("  \"abstention\": [\n");
    for (i, r) in abst_rows.iter().enumerate() {
        let comma = if i + 1 == abst_rows.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{\"label\": \"{}\", \"n\": {}, \"abstain\": {:.4}, \"accuracy\": {}, \"mean_sim\": {:.4}}}{comma}\n",
            r.label,
            r.n,
            r.abstain,
            if r.accuracy.is_finite() {
                format!("{:.4}", r.accuracy)
            } else {
                "null".to_string()
            },
            r.mean_sim
        ));
    }
    json.push_str("  ],\n");
    json.push_str(&format!("  \"memory_oov_abstain\": {mem_abst:.4}\n"));
    json.push_str("}\n");
    std::fs::write(path, json)?;
    println!("artefato: {path}");

    Ok(())
}
