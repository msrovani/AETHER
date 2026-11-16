//! E2 — lógica como Ising com **certificado de factibilidade** (5 estágios).
//!
//! O E3b mostrou o limite honesto: `ΔE` sozinho **não** prediz acerto em
//! otimização livre, mas onde existe restrição **contável** (`violações == 0`)
//! a precisão é 100%. O E2 constrói o caminho que produz essa contagem.
//!
//! # Pipeline (determinístico, zero-alocação no caminho quente)
//!
//! ```text
//! (1) presolve simbólico CNF        unidades + literais falhos (consequência lógica)
//! (2) persistência (roof dual)      dominância 1ª ordem sobre o QUBO (preserva o ótimo)
//! (3) decomposição                  componentes conexos do grafo de cláusulas
//! (4) BSB + Lagrange                multiplicadores por cláusula violada
//! (5) certificado                   (E, ΔE, violações, proveniência)
//! ```
//!
//! # Por que o gadget exato (e não a penalidade por pares)
//!
//! A "penalidade quadrática" usual de 3-SAT pune pares de literais falsos: uma
//! cláusula com **um** literal verdadeiro já recebe penalidade 1 (de 3). Isso
//! distorce a paisagem exatamente nas instâncias apertadas — o efeito medido em
//! arXiv:2511.21046. Aqui cada cláusula de 3 literais vira um gadget **exato**
//! com dois spins auxiliares (`u = ¬a ∧ ¬b`, `w = u ∧ ¬c`, penalidade `= w`),
//! todos quadráticos:
//!
//! ```text
//! P(a,b,c,u,w) = 1 − a − b + ab + 2au + 2bu − uc − 2uw + 2w + 2cw
//! ```
//!
//! `min P = 0` sse a cláusula é satisfeita, `1` sse é violada — verificado por
//! enumeração exaustiva em teste. Cláusulas de 2 e 1 literais (após o presolve)
//! usam `(1−a)(1−b)` e `(1−a)`, que já são quadráticas/lineares.
//!
//! # O que o certificado garante
//!
//! **Factibilidade**, não verdade: `violations == 0` significa que a atribuição
//! avalia todas as cláusulas originais como verdadeiras. Não significa que o
//! mundo seja assim. A convenção de energia é a do solver
//! (`H = −½·sᵀJs − hᵀs`), com o offset constante do gadget omitido — o offset
//! não muda argmin nem gap.
//!
//! # Zero alocação
//!
//! [`LogicPipeline::new`] aloca todos os buffers; [`LogicPipeline::resolve_into`]
//! não aloca (os `Vec` de trabalho têm capacidade fixada e são indexados por
//! comprimento, nunca `push`ados no caminho quente). O teste `e2_logic_gate`
//! conta as alocações da thread com um alocador instrumentado.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::time::Instant;

use crate::core::certificate::{energy, Certificate, Provenance};
use crate::core::presolve::persistency_qumo_into;
use crate::error::{AetherError, Result};

/// Aceleração inicial da dinâmica bSB (mesma de `thermodynamic_solver`).
const A0: f64 = 1.0;
/// Abaixo deste desvio-padrão de `J`, `c0` cai no *clamp* documentado.
const STD_MIN: f64 = 1e-12;
/// Tolerância estrita para aceitar um flip como redutor de energia.
const FLIP_EPS: f64 = 1e-12;

/// Cláusula de até três literais: `(si·x_i) ∨ (sj·x_j) ∨ (sk·x_k)`.
///
/// `k == usize::MAX` significa "sem terceiro literal" (cláusula de 2 ou 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clause3 {
    /// Primeira variável.
    pub i: usize,
    /// Sinal do primeiro literal (`+1` positivo, `-1` negativo).
    pub si: i8,
    /// Segunda variável.
    pub j: usize,
    /// Sinal do segundo literal.
    pub sj: i8,
    /// Terceira variável (`usize::MAX` = ausente).
    pub k: usize,
    /// Sinal do terceiro literal.
    pub sk: i8,
}

impl Clause3 {
    /// Cláusula de três literais.
    pub const fn new3(i: usize, si: i8, j: usize, sj: i8, k: usize, sk: i8) -> Self {
        Self {
            i,
            si,
            j,
            sj,
            k,
            sk,
        }
    }

    /// Cláusula de dois literais.
    pub const fn new2(i: usize, si: i8, j: usize, sj: i8) -> Self {
        Self {
            i,
            si,
            j,
            sj,
            k: usize::MAX,
            sk: 0,
        }
    }

    /// Literal `idx` (`0..len()`).
    #[inline]
    pub fn lit(&self, idx: usize) -> (usize, i8) {
        match idx {
            0 => (self.i, self.si),
            1 => (self.j, self.sj),
            _ => (self.k, self.sk),
        }
    }

    /// Número de literais (1, 2 ou 3).
    #[inline]
    pub fn len(&self) -> usize {
        if self.k == usize::MAX {
            2
        } else {
            3
        }
    }

    /// `true` se não há literais (nunca ocorre em uso normal).
    #[inline]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// `true` se a atribuição `s` satisfaz a cláusula.
    #[inline]
    pub fn satisfied(&self, s: &[i8]) -> bool {
        s[self.i] == self.si
            || s[self.j] == self.sj
            || (self.k != usize::MAX && s[self.k] == self.sk)
    }

    /// `true` se algum literal já está **fixado como verdadeiro**.
    #[inline]
    pub fn satisfied_by_fixed(&self, fixed: &[Option<i8>]) -> bool {
        fixed[self.i] == Some(self.si)
            || fixed[self.j] == Some(self.sj)
            || (self.k != usize::MAX && fixed[self.k] == Some(self.sk))
    }
}

/// Cláusulas de 3-SAT violadas por `s` (contagem **exata**).
pub fn violations_3(clauses: &[Clause3], s: &[i8]) -> u32 {
    clauses.iter().filter(|c| !c.satisfied(s)).count() as u32
}

/// Estatísticas do último [`LogicPipeline::resolve_into`].
#[derive(Clone, Copy, Debug, Default)]
pub struct StageStats {
    /// Variáveis fixadas no estágio simbólico.
    pub fixed_symbolic: usize,
    /// Variáveis fixadas pela persistência.
    pub fixed_persistency: usize,
    /// Variáveis residuais após (1)+(2).
    pub n_free: usize,
    /// Componentes conexos resolvidos.
    pub components: usize,
    /// Maior componente.
    pub largest_component: usize,
    /// Rodadas de Lagrange executadas.
    pub lagrange_rounds: usize,
    /// Execuções do kernel BSB (componentes × reinícios × rodadas).
    pub bsb_runs: usize,
    /// Movimentos do reparo de factibilidade (min-conflicts com tabu).
    pub repair_moves: usize,
    /// Tempo dos estágios (1)–(3).
    pub symbolic_us: f64,
    /// Tempo do estágio (1) isolado (presolve).
    pub presolve_us: f64,
    /// Tempo do estágio (2) isolado (persistência).
    pub persistency_us: f64,
    /// Tempo do estágio (3) isolado (decomposição).
    pub decompose_us: f64,
    /// Sondagens de literal falho efetivamente executadas.
    pub probes: usize,
    /// Tempo do estágio (4).
    pub solve_us: f64,
    /// `H(s*)` na convenção do solver (offset do gadget omitido).
    pub energy: f64,
    /// `min_i ΔH_i` sobre as variáveis livres (`∞` se não há nenhuma).
    pub gap: f64,
    /// Cláusulas originais violadas.
    pub violations: u32,
    /// Resposta decisiva (`violations == 0`).
    pub decisive: bool,
}

/// Kernel bSB compacto (sub-QUBOs de tamanho variável, sem realocar).
///
/// Replica **exatamente** a dinâmica de
/// [`crate::core::thermodynamic_solver::ThermodynamicIsingSolver`]: mesmas
/// condições iniciais (`U(−0.1, 0.1)`), mesmo `c0 = 0.5/(√N·std(J))`, Euler
/// simplético com paredes inelásticas, elitismo e polimento guloso. A diferença
/// é só o dimensionamento: os buffers são `n_max` e cada componente usa o
/// prefixo `0..n`. Um teste compara os dois kernels bit-a-bit na instância
/// completa. A duplicação existe para resolver **sub-QUBOs** (estágio 3) sem um
/// pool de solvers nem padding que dilua `c0`.
#[derive(Debug)]
pub struct BsbCore {
    n_max: usize,
    steps: usize,
    dt: f64,
    a0: f64,
    x: Vec<f64>,
    y: Vec<f64>,
    hx: Vec<f64>,
    s: Vec<i8>,
    best: Vec<i8>,
    g: Vec<f64>,
    s_try: Vec<i8>,
    g_try: Vec<f64>,
}

impl BsbCore {
    /// Aloca os buffers para até `n_max` spins.
    pub fn new(n_max: usize, steps: usize, dt: f64) -> Self {
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.05 };
        Self {
            n_max,
            steps,
            dt,
            a0: A0,
            x: vec![0.0; n_max],
            y: vec![0.0; n_max],
            hx: vec![0.0; n_max],
            s: vec![1; n_max],
            best: vec![1; n_max],
            g: vec![0.0; n_max],
            s_try: vec![1; n_max],
            g_try: vec![0.0; n_max],
        }
    }

    /// Relaxa `n` spins (prefixo dos buffers) e escreve o argmin em `states`.
    ///
    /// `j` é `n × n` row-major; `h` tem `n`. Devolve `H(states)`.
    pub fn relax_into(
        &mut self,
        j: &[f64],
        h: &[f64],
        n: usize,
        seed: u64,
        states: &mut [i8],
    ) -> f64 {
        assert!(n <= self.n_max, "BsbCore: n > n_max");
        assert!(j.len() >= n * n && h.len() >= n && states.len() >= n);
        if n == 0 {
            return 0.0;
        }
        let steps = self.steps;
        let dt = self.dt;
        let a0 = self.a0;

        // --- estado inicial (mesma ordem de sorteio do solver auditado) ---
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        for i in 0..n {
            self.x[i] = (rng.gen::<f64>() - 0.5) * 0.2;
            self.y[i] = (rng.gen::<f64>() - 0.5) * 0.2;
        }
        for i in 0..n {
            let s = sign(self.x[i]);
            states[i] = s;
            self.best[i] = s;
        }
        let e0 = row_energy(j, n, h, states);
        let mut e_best = e0;

        // --- c0 por instância ---
        let mut sum = 0.0;
        let mut sumsq = 0.0;
        for v in &j[..n * n] {
            sum += *v;
            sumsq += *v * *v;
        }
        let count = (n * n) as f64;
        let mean = sum / count;
        let var = (sumsq / count - mean * mean).max(0.0);
        let std = var.sqrt();
        let inv_sqrt_n = 1.0 / (n as f64).sqrt();
        let c0 = if std < STD_MIN {
            0.5 * inv_sqrt_n
        } else {
            0.5 * inv_sqrt_n / std
        };

        // --- integração ---
        for k in 0..steps {
            let a = a0 * ((k + 1) as f64) / (steps as f64);
            let drift = a0 - a;
            for i in 0..n {
                let row = &j[i * n..(i + 1) * n];
                self.hx[i] = dot_f64(row, &self.x[..n]) + h[i];
            }
            for i in 0..n {
                self.y[i] += dt * (-drift * self.x[i] + c0 * self.hx[i]);
                let xn = self.x[i] + dt * a0 * self.y[i];
                if xn > 1.0 {
                    self.x[i] = 1.0;
                    self.y[i] = 0.0;
                } else if xn < -1.0 {
                    self.x[i] = -1.0;
                    self.y[i] = 0.0;
                } else {
                    self.x[i] = xn;
                }
            }
            for i in 0..n {
                self.s[i] = sign(self.x[i]);
            }
            {
                let sf = &mut self.g[..n];
                for i in 0..n {
                    sf[i] = f64::from(self.s[i]);
                }
                let mut e = 0.0;
                for i in 0..n {
                    let row = &j[i * n..(i + 1) * n];
                    e -= 0.5 * sf[i] * dot_f64(row, sf);
                }
                for i in 0..n {
                    e -= h[i] * sf[i];
                }
                if e < e_best {
                    e_best = e;
                    for i in 0..n {
                        self.best[i] = self.s[i];
                    }
                }
            }
        }

        // --- polimento guloso determinístico ---
        for i in 0..n {
            self.s[i] = self.best[i];
        }
        let e = descend(n, j, h, &mut self.s[..n], &mut self.g[..n]);

        // --- fuga determinística (só se o polimento não bateu H(s_0)) ---
        if e >= e0 {
            for i in 0..n {
                self.best[i] = self.s[i];
            }
            let mut best_e = e;
            for i in 0..n {
                self.s_try[..n].copy_from_slice(&self.best[..n]);
                self.s_try[i] = -self.s_try[i];
                let e_try = descend(n, j, h, &mut self.s_try[..n], &mut self.g_try[..n]);
                if e_try < best_e {
                    best_e = e_try;
                    self.s[..n].copy_from_slice(&self.s_try[..n]);
                }
            }
        }

        states[..n].copy_from_slice(&self.s[..n]);
        row_energy(j, n, h, states)
    }
}

#[inline]
fn sign(v: f64) -> i8 {
    if v >= 0.0 {
        1
    } else {
        -1
    }
}

#[inline]
fn row_energy(jb: &[f64], n: usize, hb: &[f64], s: &[i8]) -> f64 {
    let mut acc = 0.0;
    for i in 0..n {
        let row = &jb[i * n..(i + 1) * n];
        acc -= 0.5 * f64::from(s[i]) * dot_i8(row, s);
    }
    for (hi, si) in hb.iter().zip(s.iter()) {
        acc -= *hi * f64::from(*si);
    }
    acc
}

/// Produto interno com 16 acumuladores (idêntico ao kernel auditado, para
/// garantir bit-a-bit o mesmo resultado).
#[inline]
fn dot_f64(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    let mut acc = [0.0f64; 16];
    let chunks = n / 16;
    for c in 0..chunks {
        let x = &a[c * 16..c * 16 + 16];
        let y = &b[c * 16..c * 16 + 16];
        for k in 0..16 {
            acc[k] += x[k] * y[k];
        }
    }
    let mut sum = 0.0;
    for v in acc {
        sum += v;
    }
    for i in chunks * 16..n {
        sum += a[i] * b[i];
    }
    sum
}

#[inline]
fn dot_i8(a: &[f64], s: &[i8]) -> f64 {
    let n = a.len().min(s.len());
    let mut acc = [0.0f64; 16];
    let chunks = n / 16;
    for c in 0..chunks {
        let x = &a[c * 16..c * 16 + 16];
        let y = &s[c * 16..c * 16 + 16];
        for k in 0..16 {
            acc[k] += x[k] * f64::from(y[k]);
        }
    }
    let mut sum = 0.0;
    for v in acc {
        sum += v;
    }
    for i in chunks * 16..n {
        sum += a[i] * f64::from(s[i]);
    }
    sum
}

#[inline]
fn fill_g(jb: &[f64], n: usize, s: &[i8], g: &mut [f64]) {
    for i in 0..n {
        let row = &jb[i * n..(i + 1) * n];
        g[i] = dot_i8(row, s);
    }
}

fn descend(n: usize, jb: &[f64], hb: &[f64], s: &mut [i8], g: &mut [f64]) -> f64 {
    fill_g(jb, n, s, g);
    let max_flips = 8 * n;
    for _ in 0..max_flips {
        let mut pick = usize::MAX;
        let mut pick_delta = -FLIP_EPS;
        for i in 0..n {
            let delta = 2.0 * f64::from(s[i]) * (g[i] + hb[i]);
            if delta < pick_delta {
                pick_delta = delta;
                pick = i;
            }
        }
        if pick == usize::MAX {
            break;
        }
        let delta_s = -2.0 * f64::from(s[pick]);
        s[pick] = -s[pick];
        for k in 0..n {
            g[k] += jb[k * n + pick] * delta_s;
        }
    }
    row_energy(jb, n, hb, s)
}

// ---------------------------------------------------------------------------
// PoB (pseudo-Boolean) em bits literais
// ---------------------------------------------------------------------------

/// Bit literal: `neg == false` ⇒ `x_var`; `neg == true` ⇒ `1 − x_var`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BitRef {
    var: u32,
    neg: bool,
}

/// Termo de grau ≤ 2 em bits literais, escalado por `lambda[clause]`.
#[derive(Clone, Copy, Debug)]
struct PoBTerm {
    coeff: f64,
    clause: u32,
    deg: u8,
    a: BitRef,
    b: BitRef,
}

impl PoBTerm {
    fn linear(coeff: f64, clause: u32, a: BitRef) -> Self {
        Self {
            coeff,
            clause,
            deg: 1,
            a,
            b: BitRef::default(),
        }
    }
    fn quad(coeff: f64, clause: u32, a: BitRef, b: BitRef) -> Self {
        Self {
            coeff,
            clause,
            deg: 2,
            a,
            b,
        }
    }
}

// ---------------------------------------------------------------------------
// Presolve simbólico de 3-SAT — versão dirigida por ocorrências
// ---------------------------------------------------------------------------

/// Estado de uma cláusula sob as fixações correntes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CState {
    /// Já satisfeita por um literal fixado.
    Sat,
    /// Tautologia (`(x ∨ ¬x ∨ …)`): sempre satisfeita.
    Taut,
    /// Duas ou mais variáveis livres distintas.
    Free,
    /// Exatamente uma variável livre: `(v, s)` é unidade.
    Unit(usize, i8),
    /// Nenhuma livre e nenhuma verdadeira.
    Conflict,
}

/// Estado de `c` sob `fixed` (mesma semântica de `propagate3`, escrita uma vez).
///
/// Conta **variáveis livres distintas** (não ocorrências): uma cláusula com o
/// mesmo literal repetido é uma unidade, e `(x ∨ ¬x ∨ …)` é tautologia.
#[inline]
fn clause_state(c: &Clause3, fixed: &[Option<i8>]) -> CState {
    let mut nfree = 0usize;
    let mut seen_v = [usize::MAX; 3];
    let mut seen_s = [0i8; 3];
    for idx in 0..c.len() {
        let (v, s) = c.lit(idx);
        match fixed[v] {
            Some(val) => {
                if val == s {
                    return CState::Sat;
                }
            }
            None => {
                let mut dup = false;
                for k in 0..nfree {
                    if seen_v[k] == v {
                        dup = true;
                        if seen_s[k] != s {
                            return CState::Taut;
                        }
                    }
                }
                if !dup {
                    seen_v[nfree] = v;
                    seen_s[nfree] = s;
                    nfree += 1;
                }
            }
        }
    }
    match nfree {
        0 => CState::Conflict,
        1 => CState::Unit(seen_v[0], seen_s[0]),
        _ => CState::Free,
    }
}

/// Orçamento de sondagens de literal falho por consulta.
///
/// A sondagem completa custa `2n` propagações e dominava o passe simbólico
/// (94 µs em n=20). Só sondamos variáveis em cláusulas **quase-unitárias**
/// (exatamente 2 variáveis livres) e com teto fixo: em 3-SAT aleatório no
/// limiar não há candidatas, então o custo cai para a propagação de unidades.
pub const PROBE_MAX: usize = 24;

/// Workspace do presolve simbólico: **listas de ocorrências** + *trail*.
///
/// Substitui a varredura completa por rodada (`O(m)` por rodada, `O(m·fixações)`
/// no total) por uma propagação dirigida: ao fixar `v`, só as cláusulas que
/// contêm `v` ou `¬v` são reexaminadas. A sondagem de literal falho usa
/// *trail* — desfaz apenas o que a suposição tocou, em vez de copiar o estado
/// inteiro e repropagar do zero.
///
/// Tudo é alocado em [`Prop3::new`]; [`Prop3::presolve_into`] não aloca.
#[derive(Debug)]
struct Prop3 {
    /// Início da lista de ocorrências de cada literal (`2*v + (s>0)`).
    occ_start: Vec<usize>,
    /// Índices de cláusula por literal.
    occ_list: Vec<usize>,
    /// Cursor de construção da CSR.
    occ_pos: Vec<usize>,
    /// Variáveis fixadas (para desfazer sondagens).
    trail: Vec<usize>,
    /// Fila de propagação (variáveis cujo valor mudou).
    pq: Vec<usize>,
    /// Candidatas a sondagem.
    cand: Vec<bool>,
}

impl Prop3 {
    fn new(n_max: usize, m_max: usize) -> Self {
        Self {
            occ_start: vec![0; 2 * n_max + 1],
            occ_list: vec![0; 3 * m_max],
            occ_pos: vec![0; 2 * n_max],
            // Sondagens desfazem e refixam: `trail`/`pq` podem receber até
            // `(PROBE_MAX + 1)·n_max` empilhamentos sem realocar.
            trail: Vec::with_capacity((PROBE_MAX + 1) * n_max),
            pq: Vec::with_capacity((PROBE_MAX + 1) * n_max),
            cand: vec![false; n_max],
        }
    }

    #[inline]
    fn lit_of(v: usize, s: i8) -> usize {
        2 * v + usize::from(s > 0)
    }

    /// Constrói a CSR de ocorrências (uma passada `O(m)`), sem alocar.
    fn build(&mut self, n: usize, clauses: &[Clause3]) {
        self.occ_start[..=2 * n].fill(0);
        for c in clauses {
            for idx in 0..c.len() {
                let (v, s) = c.lit(idx);
                self.occ_start[Self::lit_of(v, s) + 1] += 1;
            }
        }
        for k in 1..=2 * n {
            self.occ_start[k] += self.occ_start[k - 1];
        }
        for k in 0..2 * n {
            self.occ_pos[k] = self.occ_start[k];
        }
        for (ci, c) in clauses.iter().enumerate() {
            for idx in 0..c.len() {
                let (v, s) = c.lit(idx);
                let l = Self::lit_of(v, s);
                let at = self.occ_pos[l];
                self.occ_list[at] = ci;
                self.occ_pos[l] = at + 1;
            }
        }
    }

    /// Fixa `v = s`, registra no *trail* e agenda a propagação.
    ///
    /// `None` = conflito (já estava no valor oposto).
    #[inline]
    fn fix(&mut self, fixed: &mut [Option<i8>], v: usize, s: i8) -> Option<bool> {
        match fixed[v] {
            Some(x) if x == s => Some(false),
            Some(_) => None,
            None => {
                fixed[v] = Some(s);
                self.trail.push(v);
                self.pq.push(v);
                Some(true)
            }
        }
    }

    /// Propaga a partir da fila (sem varredura inicial): `O(afetadas)`.
    fn propagate_from(&mut self, clauses: &[Clause3], fixed: &mut [Option<i8>]) -> bool {
        while let Some(v) = self.pq.pop() {
            for s in [1i8, -1i8] {
                let l = Self::lit_of(v, s);
                let mut k = self.occ_start[l];
                let end = self.occ_start[l + 1];
                while k < end {
                    let ci = self.occ_list[k];
                    match clause_state(&clauses[ci], fixed) {
                        CState::Conflict => return true,
                        CState::Unit(w, sw) if self.fix(fixed, w, sw).is_none() => return true,
                        _ => {}
                    }
                    k += 1;
                }
            }
        }
        false
    }

    /// Varredura **inicial** `O(m)` + propagação pela fila.
    ///
    /// Só é chamada no começo e depois de uma fixação feita fora do `fix`
    /// (a sondagem usa [`Self::propagate_from`], que é `O(afetadas)`).
    fn propagate_all(&mut self, clauses: &[Clause3], fixed: &mut [Option<i8>]) -> bool {
        self.pq.clear();
        for c in clauses.iter() {
            match clause_state(c, fixed) {
                CState::Conflict => return true,
                CState::Unit(v, s) if self.fix(fixed, v, s).is_none() => return true,
                _ => {}
            }
        }
        self.propagate_from(clauses, fixed)
    }

    /// Marca como candidatas as variáveis livres de cláusulas com 2 livres.
    fn collect_candidates(&mut self, n: usize, clauses: &[Clause3], fixed: &[Option<i8>]) {
        self.cand[..n].fill(false);
        for c in clauses {
            if !matches!(clause_state(c, fixed), CState::Free) {
                continue;
            }
            let mut ids = [usize::MAX; 3];
            let mut nfree = 0usize;
            for idx in 0..c.len() {
                let (v, _s) = c.lit(idx);
                if fixed[v].is_none() && !ids[..nfree].contains(&v) {
                    ids[nfree] = v;
                    nfree += 1;
                }
            }
            if nfree == 2 {
                self.cand[ids[0]] = true;
                self.cand[ids[1]] = true;
            }
        }
    }

    /// Desfaz até o checkpoint (sondagem).
    fn rollback(&mut self, fixed: &mut [Option<i8>], checkpoint: usize) {
        for i in (checkpoint..self.trail.len()).rev() {
            let v = self.trail[i];
            fixed[v] = None;
        }
        self.trail.truncate(checkpoint);
        self.pq.clear();
    }

    /// Presolve de 3-SAT: unidades + **literais falhos** (`l ⇒ conflito` ⇒ `¬l`).
    ///
    /// Toda fixação é consequência lógica, logo é *sound* por construção.
    /// Retorna `(conflito, rodadas, sondagens)`.
    fn presolve_into(
        &mut self,
        n: usize,
        clauses: &[Clause3],
        fixed: &mut [Option<i8>],
        max_probes: usize,
    ) -> (bool, usize, usize) {
        for f in fixed[..n].iter_mut() {
            *f = None;
        }
        self.trail.clear();
        self.build(n, clauses);

        let mut rounds = 0usize;
        let mut probes = 0usize;
        if self.propagate_all(clauses, fixed) {
            return (true, 1, 0);
        }
        loop {
            rounds += 1;
            if probes >= max_probes {
                break;
            }
            self.collect_candidates(n, clauses, fixed);
            let mut changed = false;
            for v in 0..n {
                if fixed[v].is_some() || !self.cand[v] {
                    continue;
                }
                for try_val in [1i8, -1i8] {
                    if probes >= max_probes {
                        break;
                    }
                    probes += 1;
                    let checkpoint = self.trail.len();
                    let conflict = match self.fix(fixed, v, try_val) {
                        None => true,
                        Some(_) => self.propagate_from(clauses, fixed),
                    };
                    self.rollback(fixed, checkpoint);
                    if conflict {
                        let forced = -try_val;
                        if fixed[v] != Some(forced) {
                            // propaga o que a fixação nova implica
                            if self.fix(fixed, v, forced).is_none()
                                || self.propagate_from(clauses, fixed)
                            {
                                return (true, rounds, probes);
                            }
                            changed = true;
                        }
                        break;
                    }
                }
            }
            if !changed || rounds > 64 {
                break;
            }
        }
        (false, rounds, probes)
    }
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// Pipeline lógico E2 — buffers fixos, resolução sem alocação.
#[derive(Debug)]
pub struct LogicPipeline {
    n_max: usize,
    m_max: usize,
    n_exp_max: usize,
    rounds: usize,
    restarts: usize,
    eta: f64,
    lambda0: f64,

    fixed3: Vec<Option<i8>>,
    /// Presolve simbólico dirigido por ocorrências (buffers próprios).
    prop: Prop3,
    res_of: Vec<usize>,
    res_clauses: Vec<Clause3>,
    /// Índice da cláusula **original** de cada cláusula residual (para o λ).
    res_orig: Vec<usize>,
    terms: Vec<PoBTerm>,

    n_exp: usize,
    fixed_exp: Vec<Option<i8>>,
    free: Vec<bool>,
    jq: Vec<f64>,
    hq: Vec<f64>,
    lin: Vec<f64>,
    qsum: Vec<f64>,
    g: Vec<f64>,
    /// `Σ_j |J_ij|` do último emissão (teste esparso de dominância).
    jabs: Vec<f64>,
    /// Índices de `jq` escritos na última emissão (limpeza esparsa).
    touched: Vec<u32>,
    residual: Vec<usize>,
    exp_state: Vec<i8>,

    parent: Vec<usize>,
    counts: Vec<usize>,
    offsets: Vec<usize>,
    cursor: Vec<usize>,
    comp_vars: Vec<usize>,

    jc: Vec<f64>,
    hc: Vec<f64>,
    sub_state: Vec<i8>,
    sub_best: Vec<i8>,

    lambda: Vec<f64>,
    bsb: BsbCore,

    best_orig: Vec<i8>,
    cur_orig: Vec<i8>,
    /// Marcas de tabu do reparo de factibilidade (1 por variável).
    tabu: Vec<u32>,

    /// Estatísticas do último `resolve_into`.
    pub last: StageStats,
}

impl LogicPipeline {
    /// Aloca tudo. `n_max` e `m_max` limitam as instâncias aceitas.
    ///
    /// `rounds` é o número de rodadas de Lagrange; `eta` o fator multiplicativo
    /// aplicado ao peso de cada cláusula violada.
    pub fn new(
        n_max: usize,
        m_max: usize,
        steps: usize,
        restarts: usize,
        rounds: usize,
        eta: f64,
    ) -> Self {
        let n_exp_max = n_max + 2 * m_max;
        Self {
            n_max,
            m_max,
            n_exp_max,
            rounds: rounds.max(1),
            restarts: restarts.max(1),
            eta: if eta.is_finite() && eta > 0.0 {
                eta
            } else {
                0.5
            },
            lambda0: 1.0,
            fixed3: vec![None; n_max],
            prop: Prop3::new(n_max, m_max),
            res_of: vec![usize::MAX; n_max],
            res_clauses: vec![Clause3::new3(0, 1, 0, 1, 0, 1); m_max],
            res_orig: vec![0; m_max],
            terms: Vec::with_capacity(9 * m_max + 4),
            n_exp: 0,
            fixed_exp: vec![None; n_exp_max],
            free: vec![false; n_exp_max],
            jq: vec![0.0; n_exp_max * n_exp_max],
            hq: vec![0.0; n_exp_max],
            lin: vec![0.0; n_exp_max],
            qsum: vec![0.0; n_exp_max],
            g: vec![0.0; n_exp_max],
            jabs: vec![0.0; n_exp_max],
            touched: Vec::with_capacity(18 * m_max + 8),
            residual: Vec::with_capacity(n_exp_max),
            exp_state: vec![1; n_exp_max],
            parent: vec![0; n_exp_max],
            counts: vec![0; n_exp_max],
            offsets: vec![0; n_exp_max + 1],
            cursor: vec![0; n_exp_max],
            comp_vars: vec![0; n_exp_max],
            jc: vec![0.0; n_exp_max * n_exp_max],
            hc: vec![0.0; n_exp_max],
            sub_state: vec![1; n_exp_max],
            sub_best: vec![1; n_exp_max],
            lambda: vec![1.0; m_max],
            bsb: BsbCore::new(n_exp_max, steps, 0.05),
            best_orig: vec![1; n_max],
            cur_orig: vec![1; n_max],
            tabu: vec![0; n_max],
            last: StageStats::default(),
        }
    }

    /// Limite de variáveis originais.
    pub fn n_max(&self) -> usize {
        self.n_max
    }
    /// Limite de cláusulas.
    pub fn m_max(&self) -> usize {
        self.m_max
    }
    /// Maior número de spins do QUBO expandido (`n_max + 2·m_max`).
    pub fn n_exp_max(&self) -> usize {
        self.n_exp_max
    }
    /// Bytes dos buffers grandes (diagnóstico de pegada).
    pub fn memory_bytes(&self) -> usize {
        (self.jq.len() + self.jc.len() + self.hq.len() + self.hc.len()) * 8
            + self.terms.capacity() * std::mem::size_of::<PoBTerm>()
            + self.fixed3.len() * std::mem::size_of::<Option<i8>>()
            + (self.prop.occ_start.len() + self.prop.occ_list.len() + self.prop.occ_pos.len())
                * std::mem::size_of::<usize>()
    }

    /// Resolve `clauses` (sobre `n` variáveis) e emite `(resposta, certificado)`.
    ///
    /// `out` recebe a atribuição em `{-1,+1}` (`out[..n]`); `cert` recebe o
    /// atestado. Zero alocação de heap.
    pub fn resolve_into(
        &mut self,
        n: usize,
        clauses: &[Clause3],
        seed: u64,
        out: &mut [i8],
        cert: &mut Certificate,
    ) -> Result<()> {
        if n > self.n_max {
            return Err(AetherError::Config("logic: n > n_max".into()));
        }
        if clauses.len() > self.m_max {
            return Err(AetherError::Config("logic: clausulas > m_max".into()));
        }
        if out.len() < n {
            return Err(AetherError::shape([n], [out.len()]));
        }
        let m = clauses.len();
        self.last = StageStats::default();
        let t_symbolic = Instant::now();

        // ---------- (1) presolve simbólico ----------
        let t_pre = Instant::now();
        // `mem::take` é O(1) e evita dois empréstimos mutáveis de `self`.
        let mut fixed3 = std::mem::take(&mut self.fixed3);
        let (conflict, _, probes) = self.prop.presolve_into(n, clauses, &mut fixed3, PROBE_MAX);
        self.fixed3 = fixed3;
        self.last.presolve_us = t_pre.elapsed().as_secs_f64() * 1e6;
        self.last.probes = probes;
        if conflict {
            for (i, o) in out[..n].iter_mut().enumerate() {
                *o = self.fixed3[i].unwrap_or(1);
            }
            *cert = Certificate::new(
                f64::NAN,
                0.0,
                u32::try_from(m.max(1)).unwrap_or(u32::MAX),
                0.0,
                Provenance::Inferred,
            );
            self.last.violations = cert.violations;
            self.last.decisive = false;
            self.last.symbolic_us = t_symbolic.elapsed().as_secs_f64() * 1e6;
            return Ok(());
        }

        // remapeia variáveis livres para ids residuais
        for r in self.res_of[..n].iter_mut() {
            *r = usize::MAX;
        }
        let mut n_res = 0usize;
        for v in 0..n {
            if self.fixed3[v].is_none() {
                self.res_of[v] = n_res;
                n_res += 1;
            }
        }
        self.last.fixed_symbolic = n - n_res;

        // cláusulas residuais (satisfeitas saem; literais fixos-falsos saem)
        let mut n_res_clauses = 0usize;
        for (orig_ci, c) in clauses.iter().enumerate() {
            if c.satisfied_by_fixed(&self.fixed3[..n]) {
                continue;
            }
            let mut kept = [0usize; 3];
            let mut signs = [0i8; 3];
            let mut nk = 0usize;
            for idx in 0..c.len() {
                let (v, s) = c.lit(idx);
                if self.fixed3[v].is_none() {
                    kept[nk] = self.res_of[v];
                    signs[nk] = s;
                    nk += 1;
                }
            }
            if nk == 0 {
                // contradição que o presolve deveria ter visto
                for (i, o) in out[..n].iter_mut().enumerate() {
                    *o = self.fixed3[i].unwrap_or(1);
                }
                *cert = Certificate::new(
                    f64::NAN,
                    0.0,
                    u32::try_from(m.max(1)).unwrap_or(u32::MAX),
                    0.0,
                    Provenance::Inferred,
                );
                self.last.violations = cert.violations;
                self.last.decisive = false;
                self.last.symbolic_us = t_symbolic.elapsed().as_secs_f64() * 1e6;
                return Ok(());
            }
            self.res_clauses[n_res_clauses] = match nk {
                1 => Clause3::new2(kept[0], signs[0], kept[0], signs[0]),
                2 => Clause3::new2(kept[0], signs[0], kept[1], signs[1]),
                _ => Clause3::new3(kept[0], signs[0], kept[1], signs[1], kept[2], signs[2]),
            };
            self.res_orig[n_res_clauses] = orig_ci;
            n_res_clauses += 1;
        }

        // ---------- constrói termos PoB (gadgets exatos) ----------
        self.terms.clear();
        let mut aux_next = n_res;
        for r in 0..n_res_clauses {
            let c = self.res_clauses[r];
            let ci = u32::try_from(self.res_orig[r]).unwrap_or(u32::MAX);
            let a = BitRef {
                var: u32::try_from(c.i).unwrap_or(u32::MAX),
                neg: c.si < 0,
            };
            let b = BitRef {
                var: u32::try_from(c.j).unwrap_or(u32::MAX),
                neg: c.sj < 0,
            };
            if c.len() == 1 {
                // (1 − a)
                self.terms.push(PoBTerm::linear(-1.0, ci, a));
                continue;
            }
            if c.len() == 2 {
                // (1 − a)(1 − b) = 1 − a − b + ab
                self.terms.push(PoBTerm::linear(-1.0, ci, a));
                self.terms.push(PoBTerm::linear(-1.0, ci, b));
                self.terms.push(PoBTerm::quad(1.0, ci, a, b));
                continue;
            }
            // Gadget exato de 3 literais.
            let d = BitRef {
                var: u32::try_from(c.k).unwrap_or(u32::MAX),
                neg: c.sk < 0,
            };
            let u = BitRef {
                var: u32::try_from(aux_next).unwrap_or(u32::MAX),
                neg: false,
            };
            let w = BitRef {
                var: u32::try_from(aux_next + 1).unwrap_or(u32::MAX),
                neg: false,
            };
            aux_next += 2;
            // P = 1 − a − b + ab + 2au + 2bu − ud + 2w − 2uw + 2dw
            self.terms.push(PoBTerm::linear(-1.0, ci, a));
            self.terms.push(PoBTerm::linear(-1.0, ci, b));
            self.terms.push(PoBTerm::linear(2.0, ci, w));
            self.terms.push(PoBTerm::quad(1.0, ci, a, b));
            self.terms.push(PoBTerm::quad(2.0, ci, a, u));
            self.terms.push(PoBTerm::quad(2.0, ci, b, u));
            self.terms.push(PoBTerm::quad(-1.0, ci, u, d));
            self.terms.push(PoBTerm::quad(-2.0, ci, u, w));
            self.terms.push(PoBTerm::quad(2.0, ci, d, w));
        }
        let n_exp = aux_next;
        self.n_exp = n_exp;

        // ---------- (2) persistência de 1ª ordem (λ = 1) ----------
        self.lambda[..m].fill(self.lambda0);
        // Estado limpo: nenhuma fixação da instância anterior pode contaminar
        // a posiform desta (soundness entre chamadas do mesmo pipeline).
        self.fixed_exp[..n_exp].fill(None);
        let t_pers = Instant::now();
        // Teste esparso primeiro: se nenhuma dominância é possível, a etapa
        // densa é pulada com resultado idêntico (zero fixações).
        if self.dominance_may_fire(n_exp) {
            self.emit_qubo(n_exp);
            let _ = persistency_qumo_into(
                n_exp,
                &self.jq[..n_exp * n_exp],
                &self.hq[..n_exp],
                &mut self.fixed_exp,
                &mut self.lin,
                &mut self.residual,
            );
        }
        self.last.persistency_us = t_pers.elapsed().as_secs_f64() * 1e6;
        for v in 0..n_exp {
            self.free[v] = self.fixed_exp[v].is_none();
            self.exp_state[v] = self.fixed_exp[v].unwrap_or(1);
        }
        let n_free = self.free[..n_exp].iter().filter(|f| **f).count();
        self.last.fixed_persistency = n_exp - n_free;
        self.last.n_free = n_free;

        // ---------- (3) decomposição em componentes conexos ----------
        let t_dec = Instant::now();
        for (i, p) in self.parent[..n_exp].iter_mut().enumerate() {
            *p = i;
        }
        let mut aux = n_res;
        for r in 0..n_res_clauses {
            let c = self.res_clauses[r];
            let mut ids = [c.i, c.j, c.i, c.i, c.i];
            let mut cnt = 2usize;
            if c.len() >= 3 && c.k != usize::MAX {
                ids[2] = c.k;
                ids[3] = aux;
                ids[4] = aux + 1;
                aux += 2;
                cnt = 5;
            }
            // Cadeia conecta o mesmo que o produto cartesiano, com bem menos
            // `union` (2 ou 3 em vez de 1 ou 10).
            if cnt == 2 {
                union(&mut self.parent, ids[0], ids[1]);
            } else {
                union(&mut self.parent, ids[0], ids[1]);
                union(&mut self.parent, ids[2], ids[3]);
                union(&mut self.parent, ids[4], ids[0]);
            }
        }

        // agrupa variáveis livres por raiz (counting sort, sem alocar)
        self.counts[..n_exp].fill(0);
        for v in 0..n_exp {
            if self.free[v] {
                let r = find(&mut self.parent, v);
                self.counts[r] += 1;
            }
        }
        let mut acc = 0usize;
        for v in 0..n_exp {
            self.offsets[v] = acc;
            acc += self.counts[v];
        }
        self.offsets[n_exp] = acc;
        self.cursor[..n_exp].copy_from_slice(&self.offsets[..n_exp]);
        for v in 0..n_exp {
            if self.free[v] {
                let r = find(&mut self.parent, v);
                let at = self.cursor[r];
                self.comp_vars[at] = v;
                self.cursor[r] = at + 1;
            }
        }

        // ---------- (4)+(5) BSB + Lagrange + certificado ----------
        self.last.decompose_us = t_dec.elapsed().as_secs_f64() * 1e6;
        let t_solve = Instant::now();
        self.last.symbolic_us = t_symbolic.elapsed().as_secs_f64() * 1e6;

        let mut best_viol = u32::MAX;
        let mut best_energy = f64::INFINITY;
        let mut rounds_run = 0usize;
        let mut decisive_rounds = 0usize;
        let mut comps_last = 0usize;
        let mut largest_last = 0usize;
        let mut bsb_runs = 0usize;

        for round in 0..self.rounds {
            rounds_run += 1;
            self.emit_qubo(n_exp);

            // --- resolve cada componente ---
            let mut comps = 0usize;
            let mut largest = 0usize;
            for r in 0..n_exp {
                let s = self.counts[r];
                if s == 0 {
                    continue;
                }
                comps += 1;
                largest = largest.max(s);
                let start = self.offsets[r];
                if s == 1 {
                    let v = self.comp_vars[start];
                    // mínimo exato de −h·s com um spin
                    self.exp_state[v] = if self.hq[v] > 0.0 { 1 } else { -1 };
                    continue;
                }
                for a in 0..s {
                    let va = self.comp_vars[start + a];
                    self.hc[a] = self.hq[va];
                    for b in 0..s {
                        let vb = self.comp_vars[start + b];
                        self.jc[a * s + b] = self.jq[va * n_exp + vb];
                    }
                }
                let mut local_best = f64::INFINITY;
                for restart in 0..self.restarts {
                    let sd = seed
                        ^ (round as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        ^ (restart as u64).wrapping_mul(0x85EB_CA77_C2B2_AE63)
                        ^ (start as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
                    let e = self.bsb.relax_into(
                        &self.jc[..s * s],
                        &self.hc[..s],
                        s,
                        sd,
                        &mut self.sub_state[..s],
                    );
                    bsb_runs += 1;
                    if e < local_best {
                        local_best = e;
                        self.sub_best[..s].copy_from_slice(&self.sub_state[..s]);
                    }
                }
                for a in 0..s {
                    let v = self.comp_vars[start + a];
                    self.exp_state[v] = self.sub_best[a];
                }
            }
            comps_last = comps;
            largest_last = largest;

            // --- monta a atribuição original ---
            for v in 0..n {
                self.cur_orig[v] = match self.fixed3[v] {
                    Some(s) => s,
                    None => self.exp_state[self.res_of[v]],
                };
            }
            // --- reparo de factibilidade (min-conflicts com tabu) ---
            let viol = self.repair_into(n, clauses);
            // devolve o reparo ao estado expandido (aux ficam como estão: E/ΔE
            // passam a ser diagnóstico, a decisão é a contagem exata)
            for v in 0..n {
                if self.fixed3[v].is_none() {
                    self.exp_state[self.res_of[v]] = self.cur_orig[v];
                }
            }
            let e = qubo_energy(
                &self.jq[..n_exp * n_exp],
                &self.hq[..n_exp],
                n_exp,
                &self.exp_state[..n_exp],
            );
            if viol < best_viol || (viol == best_viol && e < best_energy) {
                best_viol = viol;
                best_energy = e;
                self.best_orig[..n].copy_from_slice(&self.cur_orig[..n]);
            }
            if viol == 0 {
                decisive_rounds += 1;
                // já decidido: mais rodadas só gastariam tempo
                break;
            }
            // --- osciladores de Lagrange: sobe o peso das cláusulas violadas ---
            for (ci, c) in clauses.iter().enumerate() {
                if !c.satisfied(&self.cur_orig[..n]) {
                    let l = &mut self.lambda[ci];
                    *l = (*l * (1.0 + self.eta)).min(1e9);
                }
            }
        }

        // ---------- (5) certificado ----------
        let free_gap = gap_free(
            &self.jq[..n_exp * n_exp],
            &self.hq[..n_exp],
            n_exp,
            &self.exp_state[..n_exp],
            &self.free[..n_exp],
            &mut self.g[..n_exp],
        );
        // sem variáveis livres não há flip possível: o gap é vacuamente positivo.
        let gap = if free_gap.is_finite() { free_gap } else { 1.0 };
        let consensus = if rounds_run > 0 {
            decisive_rounds as f64 / rounds_run as f64
        } else {
            0.0
        };

        out[..n].copy_from_slice(&self.best_orig[..n]);
        *cert = Certificate::new(best_energy, gap, best_viol, consensus, Provenance::Inferred);
        self.last.components = comps_last;
        self.last.largest_component = largest_last;
        self.last.lagrange_rounds = rounds_run;
        self.last.bsb_runs = bsb_runs;
        self.last.solve_us = t_solve.elapsed().as_secs_f64() * 1e6;
        self.last.energy = best_energy;
        self.last.gap = gap;
        self.last.violations = best_viol;
        self.last.decisive = best_viol == 0;
        Ok(())
    }

    /// Reparo de factibilidade determinístico (min-conflicts com tabu).
    ///
    /// O estágio (4) do dossiê pede que os osciladores de Lagrange "escapem de
    /// mínimos inviáveis". O solver devolve um mínimo local do QUBO que pode
    /// ficar a 1–3 cláusulas da factibilidade; este passo curto usa a
    /// **contagem exata** de violações (não a energia) para fechar essa
    /// distância: escolhe uma cláusula violada, avalia o flip de cada literal e
    /// aceita o que mais reduz violações, com tabu para não ciclar.
    ///
    /// É `O(movimentos · m)` com `movimentos ≤ 64·n` e não aloca. Devolve as
    /// violações restantes.
    fn repair_into(&mut self, n: usize, clauses: &[Clause3]) -> u32 {
        let max_moves = (64 * n).max(16);
        let ttl = (n / 2 + 1) as u32;
        let m = clauses.len();
        self.tabu[..n].fill(0);
        let mut clock = 1u32;
        let mut cursor = 0usize;
        let mut moves = 0usize;
        loop {
            let viol = violations_3(clauses, &self.cur_orig[..n]);
            if viol == 0 || moves >= max_moves {
                self.last.repair_moves += moves;
                return viol;
            }
            // primeira cláusula violada a partir do cursor
            let mut chosen = usize::MAX;
            for t in 0..m {
                let idx = (cursor + t) % m;
                if !clauses[idx].satisfied(&self.cur_orig[..n]) {
                    chosen = idx;
                    cursor = idx + 1;
                    break;
                }
            }
            if chosen == usize::MAX {
                self.last.repair_moves += moves;
                return 0;
            }
            let c = clauses[chosen];
            let mut best_v = usize::MAX;
            let mut best_i = usize::MAX;
            for k in 0..c.len() {
                let (v, _) = c.lit(k);
                if self.tabu[v] >= clock {
                    continue;
                }
                self.cur_orig[v] = -self.cur_orig[v];
                let vv = violations_3(clauses, &self.cur_orig[..n]) as usize;
                self.cur_orig[v] = -self.cur_orig[v];
                if vv < best_v {
                    best_v = vv;
                    best_i = v;
                }
            }
            if best_i == usize::MAX {
                // todos tabu: libera o primeiro literal da cláusula
                best_i = c.i;
            }
            self.cur_orig[best_i] = -self.cur_orig[best_i];
            self.tabu[best_i] = clock + ttl;
            clock = clock.wrapping_add(1);
            if clock == 0 {
                self.tabu[..n].fill(0);
                clock = 1;
            }
            moves += 1;
        }
    }

    /// Emite o QUBO expandido (spin) com os pesos correntes de `lambda`.
    ///
    /// A limpeza é **esparsa**: só os índices escritos na emissão anterior são
    /// zerados (`touched`). Para o QUBO expandido (≈ `5m` spins) isso troca um
    /// `fill` de `n_exp²` doubles por `O(#termos)`.
    fn emit_qubo(&mut self, n_exp: usize) {
        let nn = n_exp * n_exp;
        for &idx in self.touched.iter() {
            self.jq[idx as usize] = 0.0;
        }
        self.touched.clear();
        self.hq[..n_exp].fill(0.0);
        self.lin[..n_exp].fill(0.0);
        self.qsum[..n_exp].fill(0.0);

        for ti in 0..self.terms.len() {
            let t = self.terms[ti];
            let w = self.lambda[t.clause as usize];
            let c = t.coeff * w;
            if t.deg == 1 {
                emit_bit1(&mut self.lin[..n_exp], &self.fixed_exp[..n_exp], t.a, c);
            } else {
                emit_bit2(
                    Some(&mut self.jq[..nn]),
                    &mut self.lin[..n_exp],
                    &mut self.qsum[..n_exp],
                    &mut self.jabs[..n_exp],
                    Some(&mut self.touched),
                    &self.fixed_exp[..n_exp],
                    n_exp,
                    t.a,
                    t.b,
                    c,
                );
            }
        }
        for v in 0..n_exp {
            self.hq[v] = -(self.lin[v] * 0.5 + self.qsum[v] * 0.25);
        }
    }

    /// Teste **esparso** de "a dominância pode disparar?" antes de densificar.
    ///
    /// A persistência de 1ª ordem só fixa `i` se `h_i + Σ_j|J_ij| < 0` (ou o
    /// simétrico). Como `h_i` e `Σ_j|J_ij|` são **aditivos sobre os termos**,
    /// dá para calculá-los em `O(#termos)` — sem construir a matriz densa.
    /// Se nenhum spin pode ser fixado, a etapa de persistência é pulada com
    /// resultado **idêntico** (ela fixaria zero) e sem o custo `O(n_exp²)`.
    fn dominance_may_fire(&mut self, n_exp: usize) -> bool {
        self.lin[..n_exp].fill(0.0);
        self.qsum[..n_exp].fill(0.0);
        self.jabs[..n_exp].fill(0.0);
        for ti in 0..self.terms.len() {
            let t = self.terms[ti];
            let c = t.coeff * self.lambda[t.clause as usize];
            if t.deg == 1 {
                let x = t.a.var as usize;
                if t.a.neg {
                    self.lin[x] -= c;
                } else {
                    self.lin[x] += c;
                }
            } else {
                emit_bit2(
                    None,
                    &mut self.lin[..n_exp],
                    &mut self.qsum[..n_exp],
                    &mut self.jabs[..n_exp],
                    None,
                    &self.fixed_exp[..n_exp],
                    n_exp,
                    t.a,
                    t.b,
                    c,
                );
            }
        }
        for v in 0..n_exp {
            let h = -(self.lin[v] * 0.5 + self.qsum[v] * 0.25);
            let s = self.jabs[v];
            if h + s < 0.0 || h - s > 0.0 {
                return true;
            }
        }
        false
    }
}

/// Soma um termo linear em bit literal, expandindo a negação.
fn emit_bit1(lin: &mut [f64], fixed: &[Option<i8>], a: BitRef, c: f64) {
    let idx = a.var as usize;
    if fixed[idx].is_none() {
        if a.neg {
            lin[idx] -= c;
        } else {
            lin[idx] += c;
        }
    }
}

/// Soma um termo quadrático em bits literais, expandindo negações.
///
/// `(1−b_i)·b_j` etc. são distribuídos; o resultado são monômios de grau ≤ 2
/// em bits **positivos**, convertidos para spin em [`LogicPipeline::emit_qubo`].
///
/// `jq`/`touched` são opcionais: sem eles a função só acumula `lin/qsum/jabs`
/// (é o caminho do teste esparso de dominância, sem construir a matriz densa).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::needless_option_as_deref)]
fn emit_bit2(
    mut jq: Option<&mut [f64]>,
    lin: &mut [f64],
    qsum: &mut [f64],
    jabs: &mut [f64],
    mut touched: Option<&mut Vec<u32>>,
    fixed: &[Option<i8>],
    n: usize,
    a: BitRef,
    b: BitRef,
    c: f64,
) {
    match (a.neg, b.neg) {
        (false, false) => add_b_monomial(
            jq.as_deref_mut(),
            lin,
            qsum,
            jabs,
            touched.as_deref_mut(),
            fixed,
            n,
            a.var as usize,
            Some(b.var as usize),
            c,
        ),
        (true, false) | (false, true) => {
            // c·(1−x)·y = c·y − c·x·y
            let (neg_var, pos_var) = if a.neg {
                (a.var as usize, b.var as usize)
            } else {
                (b.var as usize, a.var as usize)
            };
            add_b_monomial(
                jq.as_deref_mut(),
                lin,
                qsum,
                jabs,
                touched.as_deref_mut(),
                fixed,
                n,
                pos_var,
                None,
                c,
            );
            add_b_monomial(
                jq.as_deref_mut(),
                lin,
                qsum,
                jabs,
                touched.as_deref_mut(),
                fixed,
                n,
                neg_var,
                Some(pos_var),
                -c,
            );
        }
        (true, true) => {
            // c·(1−x)(1−y) = c − c·x − c·y + c·x·y  (constante descartada)
            let x = a.var as usize;
            let y = b.var as usize;
            add_b_monomial(
                jq.as_deref_mut(),
                lin,
                qsum,
                jabs,
                touched.as_deref_mut(),
                fixed,
                n,
                x,
                None,
                -c,
            );
            add_b_monomial(
                jq.as_deref_mut(),
                lin,
                qsum,
                jabs,
                touched.as_deref_mut(),
                fixed,
                n,
                y,
                None,
                -c,
            );
            add_b_monomial(
                jq.as_deref_mut(),
                lin,
                qsum,
                jabs,
                touched.as_deref_mut(),
                fixed,
                n,
                x,
                Some(y),
                c,
            );
        }
    }
}

/// Acrescenta `c · b_x` (se `y == None`) ou `c · b_x b_y` já em spins.
#[allow(clippy::too_many_arguments)]
fn add_b_monomial(
    jq: Option<&mut [f64]>,
    lin: &mut [f64],
    qsum: &mut [f64],
    jabs: &mut [f64],
    touched: Option<&mut Vec<u32>>,
    fixed: &[Option<i8>],
    n: usize,
    x: usize,
    y: Option<usize>,
    c: f64,
) {
    let bx = fixed[x].map(|s| if s == 1 { 1.0 } else { 0.0 });
    match y {
        None => {
            if bx.is_none() {
                lin[x] += c;
            }
        }
        Some(y) => {
            if x == y {
                // b² = b
                if bx.is_none() {
                    lin[x] += c;
                }
                return;
            }
            let by = fixed[y].map(|s| if s == 1 { 1.0 } else { 0.0 });
            match (bx, by) {
                (Some(_), Some(_)) => {}
                (Some(vx), None) => lin[y] += c * vx,
                (None, Some(vy)) => lin[x] += c * vy,
                (None, None) => {
                    let (xy, yx) = (x * n + y, y * n + x);
                    if let Some(j) = jq {
                        j[xy] -= c * 0.25;
                        j[yx] -= c * 0.25;
                    }
                    if let Some(t) = touched {
                        t.push(xy as u32);
                        t.push(yx as u32);
                    }
                    qsum[x] += c;
                    qsum[y] += c;
                    jabs[x] += c.abs() * 0.25;
                    jabs[y] += c.abs() * 0.25;
                }
            }
        }
    }
}

#[inline]
fn find(parent: &mut [usize], mut v: usize) -> usize {
    while parent[v] != v {
        let g = parent[parent[v]];
        parent[v] = g;
        v = g;
    }
    v
}

#[inline]
fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        parent[rb] = ra;
    }
}

/// Gap local sobre as variáveis livres **relevantes** (`∞` se não houver).
///
/// Uma variável sem campo e sem acoplamento não muda energia nem violações:
/// incluí-la produziria `ΔE = 0` e uma abstenção falsa. Só variáveis com termo
/// incidente entram no gap.
fn gap_free(j: &[f64], h: &[f64], n: usize, s: &[i8], free: &[bool], g: &mut [f64]) -> f64 {
    fill_g(j, n, s, g);
    let mut best = f64::INFINITY;
    for i in 0..n {
        if !free[i] {
            continue;
        }
        let mut relevant = h[i].abs() > 1e-12;
        if !relevant {
            for k in 0..n {
                if j[i * n + k].abs() > 1e-12 {
                    relevant = true;
                    break;
                }
            }
        }
        if !relevant {
            continue;
        }
        let d = 2.0 * f64::from(s[i]) * (g[i] + h[i]);
        if d < best {
            best = d;
        }
    }
    best
}

/// Energia `H = −½ sᵀJs − hᵀs` usada pelo certificado.
pub fn qubo_energy(j: &[f64], h: &[f64], n: usize, s: &[i8]) -> f64 {
    energy(j, h, n, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enumera todas as atribuições de (a,b,c,u,w) e confirma que o gadget é
    /// exato: penalidade 0 sse a cláusula é satisfeita, 1 sse violada.
    #[test]
    fn gadget_de_tres_literais_e_exato() {
        // P em bits, avaliado diretamente pela fórmula do módulo.
        fn p(a: i32, b: i32, c: i32, u: i32, w: i32) -> i32 {
            1 - a - b + a * b + 2 * a * u + 2 * b * u - u * c - 2 * u * w + 2 * w + 2 * c * w
        }
        let mut worst_sat = 0;
        let mut worst_viol = 0;
        for a in 0..=1 {
            for b in 0..=1 {
                for c in 0..=1 {
                    let sat = a + b + c > 0;
                    let best = (0..=1)
                        .flat_map(|u| (0..=1).map(move |w| (u, w)))
                        .map(|(u, w)| p(a, b, c, u, w))
                        .min()
                        .expect("min");
                    if sat {
                        worst_sat = worst_sat.max(best);
                    } else {
                        worst_viol = worst_viol.max(best.min(1));
                    }
                    assert_eq!(best, i32::from(!sat), "a={a} b={b} c={c}");
                }
            }
        }
        assert_eq!(worst_sat, 0);
        assert_eq!(worst_viol, 1);
    }

    /// A conversão bit→spin + a emissão de termos preserva a penalidade.
    #[test]
    fn emissao_de_termos_preserva_penalidade() {
        let n_exp = 5usize; // a,b,c,u,w
        let mut p = LogicPipeline::new(3, 1, 4, 1, 1, 0.5);
        p.terms.clear();
        // uma cláusula (a ∨ b ∨ c) com literais positivos, clause 0
        let a = BitRef { var: 0, neg: false };
        let b = BitRef { var: 1, neg: false };
        let c = BitRef { var: 2, neg: false };
        let u = BitRef { var: 3, neg: false };
        let w = BitRef { var: 4, neg: false };
        p.terms.push(PoBTerm::linear(-1.0, 0, a));
        p.terms.push(PoBTerm::linear(-1.0, 0, b));
        p.terms.push(PoBTerm::linear(2.0, 0, w));
        p.terms.push(PoBTerm::quad(1.0, 0, a, b));
        p.terms.push(PoBTerm::quad(2.0, 0, a, u));
        p.terms.push(PoBTerm::quad(2.0, 0, b, u));
        p.terms.push(PoBTerm::quad(-1.0, 0, u, c));
        p.terms.push(PoBTerm::quad(-2.0, 0, u, w));
        p.terms.push(PoBTerm::quad(2.0, 0, c, w));
        for item in p.fixed_exp.iter_mut().take(n_exp) {
            *item = None;
        }
        p.emit_qubo(n_exp);
        // O QUBO representa `P_termos − C`, onde `P_termos` omite a constante
        // `1` do gadget e `C = ΣL/2 + ΣQ/4` é a constante da expansão bit→spin.
        let c_const = (-1.0 - 1.0 + 2.0) / 2.0 + (1.0 + 2.0 + 2.0 - 1.0 - 2.0 + 2.0) / 4.0;
        for mask in 0..(1u32 << n_exp) {
            let mut s = [0i8; 5];
            let mut bits = [0i32; 5];
            for v in 0..n_exp {
                let bit = ((mask >> v) & 1) as i32;
                bits[v] = bit;
                s[v] = if bit == 1 { 1 } else { -1 };
            }
            let e = qubo_energy(&p.jq[..n_exp * n_exp], &p.hq[..n_exp], n_exp, &s);
            let p_terms = f64::from(
                -bits[0] - bits[1]
                    + 2 * bits[4]
                    + bits[0] * bits[1]
                    + 2 * bits[0] * bits[3]
                    + 2 * bits[1] * bits[3]
                    - bits[3] * bits[2]
                    - 2 * bits[3] * bits[4]
                    + 2 * bits[2] * bits[4],
            );
            let want = p_terms - c_const;
            assert!((e - want).abs() < 1e-9, "mask={mask} e={e} want={want}");
        }
    }

    /// O kernel compacto reproduz o solver auditado bit-a-bit.
    #[test]
    fn bsb_compacto_igual_ao_solver_auditado() {
        use crate::core::thermodynamic_solver::{
            IsingLandscape, RelaxOutcome, ThermodynamicIsingSolver,
        };
        use ndarray::{Array2, Array3};

        let n = 24usize;
        let steps = 20usize;
        let mut j = vec![0.0f64; n * n];
        let mut h = vec![0.0f64; n];
        let mut s = 0xA37Eu64;
        for i in 0..n {
            for k in (i + 1)..n {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                let v = ((s >> 40) as f64 / (1u64 << 24) as f64) * 2.0 - 1.0;
                j[i * n + k] = v;
                j[k * n + i] = v;
            }
            h[i] = ((s & 0xFF) as f64 / 255.0) - 0.5;
        }

        let mut core = BsbCore::new(n, steps, 0.05);
        let mut mine = vec![0i8; n];
        let e_mine = core.relax_into(&j, &h, n, 42, &mut mine);

        let j3 = Array3::from_shape_vec((1, n, n), j).expect("j");
        let h2 = Array2::from_shape_vec((1, n), h).expect("h");
        let mut solver = ThermodynamicIsingSolver::new(1, n, steps, 0.05);
        let mut out = RelaxOutcome::with_capacity(1, n, steps);
        solver.reset(42);
        solver
            .relax_into(IsingLandscape::new(j3.view(), h2.view()), &mut out)
            .expect("relax");
        let theirs: Vec<i8> = (0..n).map(|i| out.states[[0, i]]).collect();
        assert_eq!(mine, theirs, "estados divergiram");
        assert!((e_mine - out.best_energy[0]).abs() < 1e-9);
    }

    #[test]
    fn presolve_nao_e_insound() {
        // (x) ∧ (¬x ∨ y) ∧ (¬y) é insatisfatível e o presolve tem de ver isso.
        let clauses = [
            Clause3::new3(0, 1, 0, 1, 0, 1),
            Clause3::new3(0, -1, 1, 1, 1, 1),
            Clause3::new3(1, -1, 1, -1, 1, -1),
        ];
        let mut prop = Prop3::new(2, 3);
        let mut fixed = vec![None; 2];
        let (conflict, _, _) = prop.presolve_into(2, &clauses, &mut fixed, PROBE_MAX);
        assert!(conflict);
    }

    #[test]
    fn resolve_acha_solucao_de_tres_sat() {
        // (x0 ∨ x1 ∨ x2) ∧ (¬x0 ∨ ¬x1) ∧ (¬x2)
        let clauses = [
            Clause3::new3(0, 1, 1, 1, 2, 1),
            Clause3::new3(0, -1, 1, -1, 1, -1),
            Clause3::new3(2, -1, 2, -1, 2, -1),
        ];
        let mut p = LogicPipeline::new(3, 3, 40, 4, 4, 0.5);
        let mut out = vec![0i8; 3];
        let mut cert = Certificate::new(0.0, 0.0, 0, 0.0, Provenance::Inferred);
        p.resolve_into(3, &clauses, 7, &mut out, &mut cert)
            .expect("resolve");
        assert_eq!(violations_3(&clauses, &out), 0, "out={out:?}");
        assert!(cert.is_coherent(), "cert={cert:?}");
        assert!(p.last.decisive);
    }
}
