//! Módulo 5 — Memória associativa oscilatória com acoplamento complexo (ONN).
//!
//! Rede de osciladores com pesos complexos `C_ij = W_ij·e^{i·δ_ij}`: a
//! magnitude `W_ij` guarda a **coerência/peso** da associação entre `i` e `j` e
//! o deslocamento `δ_ij` guarda a relação de fase — é isso que permite gravar
//! padrões contínuos, e não apenas binários `0/π`.
//!
//! Dinâmica de recuperação (exatamente a da especificação):
//!
//! ```text
//! dφ_i/dt = ω0 + (1/N) Σ_j W_ij · sin(φ_j − φ_i − δ_ij)
//!                  + α_clamp · m_i · sin(φ_i^cue + ω0·t − φ_i)
//! ```
//!
//! `m_i = 1` nos nós conhecidos (grampo sensorial) e `m_i = 0` nos ocultos: os
//! osciladores mascarados caem no atrator gravado e são reconstruídos.
//! Energia de fase minimizada implicitamente:
//!
//! ```text
//! E_phase(φ) = −(1/2N) Σ_ij W_ij · cos(φ_j − φ_i − δ_ij)
//! ```
//!
//! # Regra de aprendizado: projeção complexa (pseudo-inversa)
//!
//! A regra hebbiana polar da especificação
//!
//! ```text
//! ΔW_ij = η_W (cos(ψ_j − ψ_i − δ_ij) − γ_W W_ij)
//! Δδ_ij = η_δ · W_ij · sin(ψ_j − ψ_i − δ_ij)
//! ```
//!
//! grava **um** atrator por matriz (uma única `δ_ij` não representa várias
//! relações de fase) e, quando generalizada por superposição de fasores
//! (`C_ij = Σ_μ e^{i(ψ_j^μ−ψ_i^μ)}`), sofre **crosstalk sistemático**: no
//! padrão `ν` o campo residual é `(1/N)Σ_j Σ_{μ≠ν} sin(a_j − a_i)` com
//! `a = ψ^μ − ψ^ν`, que não decai com `N` e desloca o mínimo da energia.
//! Medido: `p0 = 0.169` rad (pior 0.222) em `N = 32`, `P = 3`.
//!
//! Este módulo usa a **regra de projeção** (pseudo-inversa complexa), que
//! zera o crosstalk exatamente. Sejam os fasores `ψ_i^μ = e^{iφ_i^μ}` e
//!
//! ```text
//! G_μν = Σ_i conj(ψ_i^μ) ψ_i^ν            (Gram P×P, hermiteana)
//! C_ij = Σ_{μν} conj(ψ_i^μ) (G⁻¹)_μν ψ_j^ν
//! ```
//!
//! (com `G` regularizada por *ridge* `1e-6·N` e inversa por Gauss-Jordan).
//! Então, para **todo** padrão `ν`,
//!
//! ```text
//! Σ_j conj(C_ij) e^{i(ψ_j^ν−ψ_i^ν)} = 1   ∀i
//! ```
//!
//! ou seja o campo residual em `ψ^ν` é **exatamente zero** e o padrão é ponto
//! fixo da dinâmica — o crosstalk responsável por `p0 = 0.169` desaparece.
//! Para `P = 1` a regra reduz à hebbiana (mesma direção `δ = ψ_j−ψ_i`).
//! Padrões relacionados por rotação rígida são o **mesmo** atrator (a dinâmica
//! é invariante por rotação global) e por isso são deduplicados no `imprint`.
//!
//! # O que cada parâmetro faz (honesto)
//!
//! * `η_δ` (`eta_delta`): taxa da regra `δ ← δ + η_δ·W·sin(arg C − δ)`. A cada
//!   `imprint` a projeção reconstrói `C` e `δ` relaxa `16·cycles` vezes para o
//!   alvo `arg C`. `η_δ = 0` **desliga o aprendizado de fase** e o recall
//!   colapsa (medido: 1.41 rad contra 0.11 rad com `η_δ = 0.08`).
//! * `cycles`: número de passadas de consolidação; como a projeção é exata,
//!   `cycles` controla apenas as passadas de relaxação de `δ` (`16·cycles`).
//! * `γ_W` (`gamma_w`): esquecimento metabólico por episódio. Cada padrão
//!   guardado entra na projeção com peso `λ_μ = (1−γ_W)^{idade_μ}`; como a
//!   propriedade de crosstalk nulo vale para qualquer `λ`, γ_W só modula a
//!   profundidade relativa dos poços (memória recente domina).
//! * `η_W` (`eta_w`): escala global do fasor. O campo é normalizado por um
//!   gauge (`W = |C|/⟨|C|⟩`), então `η_W` não altera a paisagem de atratores;
//!   é mantido por compatibilidade e validado (`≥ 0`, finito).
//!
//! # Capacidade
//!
//! A projeção é exata enquanto os `P` fasores forem linearmente independentes,
//! isto é `P ≤ N−1`. A memória mantém uma **janela deslizante** de
//! `min(N−1, 64)` padrões (FIFO: o mais antigo sai), o que mantém `G`
//! invertível e o custo do `imprint` em `O(P³ + P²N + PN²)`.
//!
//! # Robustez
//!
//! `imprint`/`recall` nunca panicam (tamanho errado, `NaN`, `dt ≤ 0`,
//! `steps = 0` e `n = 0` são no-op documentados); `try_imprint`/`try_recall`
//! devolvem `Err` explícito. `recall_into` não aloca após o warmup.

use std::f32::consts::{PI, TAU};

/// Máximo de padrões guardados, independente de `N` (janela deslizante).
const PATTERN_CAP_MAX: usize = 64;

/// *Ridge* relativo aplicado à diagonal de `G` antes da inversão.
const RIDGE: f64 = 1e-6;

/// Relaxações de fase por ciclo de `imprint` (regra `Δδ`).
const DELTA_SUBSTEPS: usize = 16;

/// Erros de validação da memória de fase.
///
/// As versões infalíveis (`imprint`, `recall`, `recall_into`) transformam
/// qualquer um destes casos em *no-op* seguro; use `try_imprint`/`try_recall`
/// para receber o diagnóstico.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseMemoryError {
    /// A rede não tem osciladores (`n = 0`).
    EmptyNetwork,
    /// `pattern.len()`, `mask.len()` ou `out.len()` diferente de `n`.
    InvalidSize,
    /// Entrada com `NaN` ou infinito.
    NonFiniteInput,
    /// `dt` não positivo ou não finito, ou `steps = 0`.
    InvalidIntegration,
    /// Parâmetros de aprendizado inválidos (`η < 0`, `γ ∉ [0, 1)`, não finitos).
    InvalidConfig,
}

impl std::fmt::Display for PhaseMemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyNetwork => write!(f, "rede vazia (n = 0)"),
            Self::InvalidSize => write!(f, "tamanho de entrada incompatível com n"),
            Self::NonFiniteInput => write!(f, "entrada contém NaN ou infinito"),
            Self::InvalidIntegration => write!(f, "passo de integração inválido"),
            Self::InvalidConfig => write!(f, "parâmetros de aprendizado inválidos"),
        }
    }
}

impl std::error::Error for PhaseMemoryError {}

/// Parâmetros de aprendizado e dinâmica da memória de fase.
#[derive(Clone, Copy, Debug)]
pub struct PhaseMemoryConfig {
    /// Frequência natural dos osciladores (rad/s). `0.0` = osciladores estáticos.
    ///
    /// Com `ω0 ≠ 0` o atrator é um **ciclo limite** que gira nessa taxa; o
    /// grampo acompanha a rotação (`ψ_i^cue + ω0·t`) e a fase acumulada fica
    /// disponível em [`ComplexPhaseMemory::rotation`].
    pub omega0: f32,
    /// Força do grampo sensorial (`α_clamp`).
    pub clamp_strength: f32,
    /// Escala do fasor (`C += η_W·e^{iΔψ}`). Como o campo é normalizado por um
    /// gauge global, `η_W` não altera a paisagem de atratores; mantenha `> 0`.
    pub eta_w: f32,
    /// Taxa de aprendizado da fase `δ` (regra `Δδ = η_δ·W·sin(arg C − δ)`).
    /// `0.0` desliga o aprendizado de fase.
    pub eta_delta: f32,
    /// Esquecimento metabólico **por episódio**: cada padrão guardado pesa
    /// `(1−γ_W)^{idade}` na projeção. Deve estar em `[0, 1)`.
    pub gamma_w: f32,
}

impl Default for PhaseMemoryConfig {
    fn default() -> Self {
        Self {
            omega0: 0.0,
            clamp_strength: 6.0,
            eta_w: 0.08,
            eta_delta: 0.05,
            gamma_w: 0.004,
        }
    }
}

/// Memória associativa oscilatória de acoplamento complexo.
///
/// A memória guarda os `P` padrões como **fasores unitários** (`pat_re/pat_im`)
/// e reconstrói o acoplamento `C` por projeção complexa a cada `imprint`.
/// A dinâmica vive nos buffers planos `weights` (módulo de `C_ij`),
/// `phase_shifts` (argumento) e `phases` (estado corrente).
pub struct ComplexPhaseMemory {
    /// Número de osciladores.
    pub n: usize,
    /// Magnitudes `W_ij` (layout plano, ordem C: índice `i*n + j`).
    pub weights: Vec<f32>,
    /// Deslocamentos de fase `δ_ij ∈ [0, 2π)` (mesmo layout).
    pub phase_shifts: Vec<f32>,
    /// Fases correntes `φ_i ∈ [0, 2π)`.
    pub phases: Vec<f32>,
    /// Rotação acumulada do referencial (`∫ ω0 dt`) durante `recall`/`free_run`.
    pub rotation: f32,
    /// Módulo médio `⟨|C_ij|⟩` do último `imprint` (referência do gauge).
    pub mean_coupling: f32,
    /// Fasores gravados `e^{iψ_i^μ}`, layout plano `μ*n + i`.
    pat_re: Vec<f32>,
    pat_im: Vec<f32>,
    /// Idade (em episódios) de cada padrão armazenado.
    pat_age: Vec<u32>,
    pat_count: usize,
    pat_cap: usize,
    /// Máscara de poda **permanente** por aresta (a projeção não ressuscita).
    pruned: Vec<bool>,
    /// Buffers de trabalho (`f64`) para Gram, inversão e reconstrução.
    gin_re: Vec<f64>,
    gin_im: Vec<f64>,
    gj_re: Vec<f64>,
    gj_im: Vec<f64>,
    c_re: Vec<f64>,
    c_im: Vec<f64>,
    a_re: Vec<f64>,
    a_im: Vec<f64>,
    /// Peso metabólico `λ_μ = (1−γ)^{idade_μ}` de cada padrão.
    lambda: Vec<f64>,
    /// Rascunho do padrão sendo gravado (fasor), para a checagem de duplicata.
    buf_re: Vec<f32>,
    buf_im: Vec<f32>,
    cfg: PhaseMemoryConfig,
}

impl std::fmt::Debug for ComplexPhaseMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComplexPhaseMemory")
            .field("n", &self.n)
            .field("patterns", &self.pat_count)
            .field("cfg", &self.cfg)
            .field("mean_coupling", &self.mean_coupling)
            .finish()
    }
}

#[inline]
fn wrap(x: f32) -> f32 {
    let mut v = x % TAU;
    if v < 0.0 {
        v += TAU;
    }
    v
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

/// Capacidade da janela de padrões: `min(N−1, 64)` (a projeção exige `P < N`).
fn pattern_capacity(n: usize) -> usize {
    n.saturating_sub(1).clamp(1, PATTERN_CAP_MAX)
}

impl ComplexPhaseMemory {
    /// Cria a memória com acoplamento nulo (superfície plana, pronta para o
    /// primeiro imprint). `n = 0` é aceito: a memória fica inerte e
    /// `phase_energy`/`density` devolvem `0.0`.
    pub fn new(n: usize) -> Self {
        Self::with_config(n, PhaseMemoryConfig::default())
    }

    /// Cria a memória com configuração explícita.
    pub fn with_config(n: usize, cfg: PhaseMemoryConfig) -> Self {
        let cap = pattern_capacity(n);
        Self {
            n,
            weights: vec![0.0f32; n * n],
            phase_shifts: vec![0.0f32; n * n],
            phases: vec![0.0f32; n],
            rotation: 0.0,
            mean_coupling: 0.0,
            pat_re: vec![0.0f32; cap * n],
            pat_im: vec![0.0f32; cap * n],
            pat_age: vec![0u32; cap],
            pat_count: 0,
            pat_cap: cap,
            pruned: vec![false; n * n],
            gin_re: vec![0.0f64; cap * cap],
            gin_im: vec![0.0f64; cap * cap],
            gj_re: vec![0.0f64; 2 * cap * cap],
            gj_im: vec![0.0f64; 2 * cap * cap],
            c_re: vec![0.0f64; n * n],
            c_im: vec![0.0f64; n * n],
            a_re: vec![0.0f64; n * cap],
            a_im: vec![0.0f64; n * cap],
            buf_re: vec![0.0f32; n],
            buf_im: vec![0.0f32; n],
            cfg,
        }
    }

    /// Configuração corrente.
    pub fn config(&self) -> &PhaseMemoryConfig {
        &self.cfg
    }

    /// Módulo médio `⟨|C_ij|⟩` do último imprint (referência do gauge:
    /// `weights = |C| / mean_coupling`). `0.0` antes do primeiro imprint.
    pub fn mean_coupling(&self) -> f32 {
        self.mean_coupling
    }

    /// Número de padrões atualmente na janela de memória.
    pub fn stored_patterns(&self) -> usize {
        self.pat_count
    }

    /// Capacidade da janela de padrões (`min(N−1, 64)`).
    pub fn pattern_capacity(&self) -> usize {
        self.pat_cap
    }

    /// Reinicia o estado dinâmico: todas as fases em `phase` e rotação em zero.
    ///
    /// `recall` é *stateful* (os nós ocultos partem do estado corrente, por
    /// continuidade com o Módulo 8); chame isto para garantir que uma
    /// recuperação não dependa da anterior.
    pub fn reset_phases(&mut self, phase: f32) {
        self.phases
            .fill(if phase.is_finite() { wrap(phase) } else { 0.0 });
        self.rotation = 0.0;
    }

    /// Energia de fase do estado atual (diagnóstico).
    ///
    /// `n = 0` devolve `0.0` (nunca `NaN`).
    pub fn phase_energy(&self) -> f32 {
        let n = self.n;
        if n == 0 {
            return 0.0;
        }
        let mut e = 0.0f32;
        for i in 0..n {
            for j in 0..n {
                let idx = i * n + j;
                let dphi = self.phases[j] - self.phases[i] - self.phase_shifts[idx];
                e -= self.weights[idx] * dphi.cos();
            }
        }
        e / (2.0 * n as f32)
    }

    /// Densidade de acoplamento ativo (pesos com módulo acima do limiar).
    ///
    /// `n = 0` devolve `0.0` (nunca `NaN`).
    pub fn density(&self, threshold: f32) -> f32 {
        if self.weights.is_empty() {
            return 0.0;
        }
        let active = self.weights.iter().filter(|w| **w > threshold).count();
        active as f32 / self.weights.len() as f32
    }

    /// Grava um atrator contínuo na memória (regra de projeção complexa).
    ///
    /// Versão infalível: entradas inválidas (tamanho errado, `NaN`, `n = 0`,
    /// `cycles = 0`, configuração inválida) são **no-op** e não envenenam o
    /// estado. Use [`Self::try_imprint`] para receber o diagnóstico.
    pub fn imprint(&mut self, pattern: &[f32], cycles: usize) {
        let _ = self.try_imprint(pattern, cycles);
    }

    /// Grava um atrator contínuo, devolvendo erro de validação explícito.
    ///
    /// O padrão entra na janela de `P` fasores e o acoplamento `C` é
    /// **reconstruído por projeção** (pseudo-inversa complexa). `cycles`
    /// controla as passadas de relaxação de `δ` (`16·cycles`); `cycles = 0` é
    /// no-op (`Ok`, nada é gravado — não é promovido silenciosamente a 1).
    pub fn try_imprint(&mut self, pattern: &[f32], cycles: usize) -> Result<(), PhaseMemoryError> {
        let n = self.n;
        if n == 0 {
            return Err(PhaseMemoryError::EmptyNetwork);
        }
        if pattern.len() != n {
            return Err(PhaseMemoryError::InvalidSize);
        }
        if pattern.iter().any(|p| !p.is_finite()) {
            return Err(PhaseMemoryError::NonFiniteInput);
        }
        let gamma = self.cfg.gamma_w;
        if !self.cfg.eta_w.is_finite()
            || self.cfg.eta_w < 0.0
            || !gamma.is_finite()
            || !(0.0..1.0).contains(&gamma)
        {
            return Err(PhaseMemoryError::InvalidConfig);
        }
        if cycles == 0 {
            return Ok(());
        }

        // Janela de padrões: envelhece, deduplica (rotação rígida = mesmo
        // atrator) e insere com FIFO quando cheia.
        self.store_pattern(pattern);

        let rate = if self.cfg.eta_delta.is_finite() {
            self.cfg.eta_delta.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let relax_steps = cycles.saturating_mul(DELTA_SUBSTEPS).min(8192);
        self.rebuild_coupling(rate, relax_steps);
        Ok(())
    }

    /// Insere o padrão na janela (ou atualiza o duplicado). Idades em episódios.
    fn store_pattern(&mut self, pattern: &[f32]) {
        let n = self.n;
        for i in 0..n {
            self.buf_re[i] = pattern[i].cos();
            self.buf_im[i] = pattern[i].sin();
        }

        // Duplicata a menos de rotação global: |⟨ψ^new, ψ^μ⟩|/N ≈ 1.
        let p = self.pat_count;
        for mu in 0..p {
            let base = mu * n;
            let mut cr = 0.0f32;
            let mut ci = 0.0f32;
            for i in 0..n {
                let (ar, ai) = (self.buf_re[i], self.buf_im[i]);
                let (br, bi) = (self.pat_re[base + i], self.pat_im[base + i]);
                cr += ar * br + ai * bi;
                ci += ai * br - ar * bi;
            }
            if (cr * cr + ci * ci).sqrt() / n as f32 > 1.0 - 1e-5 {
                for i in 0..n {
                    self.pat_re[base + i] = self.buf_re[i];
                    self.pat_im[base + i] = self.buf_im[i];
                }
                self.pat_age[mu] = 0;
                for a in self.pat_age[..p].iter_mut() {
                    if *a != 0 {
                        *a += 1;
                    }
                }
                return;
            }
        }

        for a in self.pat_age[..p].iter_mut() {
            *a += 1;
        }
        let slot = if p == self.pat_cap {
            // FIFO: descarta o mais antigo (índice 0).
            let len = p * n;
            self.pat_re.copy_within(n..len, 0);
            self.pat_im.copy_within(n..len, 0);
            self.pat_age.copy_within(1..p, 0);
            p - 1
        } else {
            self.pat_count = p + 1;
            p
        };
        let base = slot * n;
        for i in 0..n {
            self.pat_re[base + i] = self.buf_re[i];
            self.pat_im[base + i] = self.buf_im[i];
        }
        self.pat_age[slot] = 0;
    }

    /// Reconstrói `C_ij` por projeção complexa e publica `(W, δ)`.
    ///
    /// ```text
    /// G_μν = Σ_i conj(ψ_i^μ) ψ_i^ν · √(λ_μ λ_ν)  (+ ridge na diagonal)
    /// C_ij = Σ_{μν} conj(ψ_i^μ) λ_μ (G⁻¹)_μν ψ_j^ν
    /// ```
    ///
    /// O peso metabólico `λ_μ = (1−γ)^{idade_μ}` apenas modula a profundidade
    /// do poço: a propriedade `Σ_j conj(C_ij)e^{i(ψ_j^ν−ψ_i^ν)} = λ_ν` (campo
    /// residual nulo em todo padrão) vale para qualquer `λ`.
    fn rebuild_coupling(&mut self, rate: f32, relax_steps: usize) {
        let n = self.n;
        let p = self.pat_count;
        let cap = self.pat_cap;
        if p == 0 {
            self.weights.fill(0.0);
            self.phase_shifts.fill(0.0);
            self.mean_coupling = 0.0;
            return;
        }
        let gamma = self.cfg.gamma_w;
        // λ_μ = (1−γ)^idade_μ, em f64.
        for mu in 0..p {
            let lambda = if gamma > 0.0 {
                (1.0 - gamma as f64).powi(self.pat_age[mu] as i32)
            } else {
                1.0
            };
            // guarda λ na diagonal de um buffer auxiliar? Não: aplique direto
            // no cálculo de C usando uma coluna extra? Simples: reusa a_re como
            // vetor de λ (n ≥ 1 garante espaço).
            self.a_re[mu] = lambda;
        }

        // --- Gram G_μν = Σ_i conj(ψ^μ_i) ψ^ν_i -------------------------------
        for mu in 0..p {
            let bm = mu * n;
            for nu in mu..p {
                let bnu = nu * n;
                let mut sr = 0.0f64;
                let mut si = 0.0f64;
                for i in 0..n {
                    let ar = self.pat_re[bm + i] as f64;
                    let ai = self.pat_im[bm + i] as f64;
                    let br = self.pat_re[bnu + i] as f64;
                    let bi = self.pat_im[bnu + i] as f64;
                    sr += ar * br + ai * bi;
                    si += ar * bi - ai * br;
                }
                self.gin_re[mu * p + nu] = sr;
                self.gin_im[mu * p + nu] = si;
                self.gin_re[nu * p + mu] = sr;
                self.gin_im[nu * p + mu] = -si;
            }
        }
        let ridge = RIDGE * n as f64;
        for mu in 0..p {
            self.gin_re[mu * p + mu] += ridge;
        }

        // --- Inversão (Gauss-Jordan com pivô parcial) em [G | I] --------------
        let w = 2 * p;
        for r in 0..p {
            for c in 0..p {
                self.gj_re[r * w + c] = self.gin_re[r * p + c];
                self.gj_im[r * w + c] = self.gin_im[r * p + c];
            }
            for c in 0..p {
                self.gj_re[r * w + p + c] = if r == c { 1.0 } else { 0.0 };
                self.gj_im[r * w + p + c] = 0.0;
            }
        }
        let mut singular = false;
        for k in 0..p {
            // pivô
            let mut piv = k;
            let mut best = 0.0f64;
            for r in k..p {
                let m2 = self.gj_re[r * w + k] * self.gj_re[r * w + k]
                    + self.gj_im[r * w + k] * self.gj_im[r * w + k];
                if m2 > best {
                    best = m2;
                    piv = r;
                }
            }
            if best < 1e-24 {
                singular = true;
                break;
            }
            if piv != k {
                for c in 0..w {
                    self.gj_re.swap(k * w + c, piv * w + c);
                    self.gj_im.swap(k * w + c, piv * w + c);
                }
            }
            // normaliza a linha k por 1/M[k][k]
            let (dr, di) = (self.gj_re[k * w + k], self.gj_im[k * w + k]);
            let den = dr * dr + di * di;
            for c in k..w {
                let (ar, ai) = (self.gj_re[k * w + c], self.gj_im[k * w + c]);
                self.gj_re[k * w + c] = (ar * dr + ai * di) / den;
                self.gj_im[k * w + c] = (ai * dr - ar * di) / den;
            }
            // elimina as demais linhas
            for r in 0..p {
                if r == k {
                    continue;
                }
                let (fr, fi) = (self.gj_re[r * w + k], self.gj_im[r * w + k]);
                if fr == 0.0 && fi == 0.0 {
                    continue;
                }
                for c in k..w {
                    let (ar, ai) = (self.gj_re[r * w + c], self.gj_im[r * w + c]);
                    let pr = fr * self.gj_re[k * w + c] - fi * self.gj_im[k * w + c];
                    let pi = fr * self.gj_im[k * w + c] + fi * self.gj_re[k * w + c];
                    self.gj_re[r * w + c] = ar - pr;
                    self.gj_im[r * w + c] = ai - pi;
                }
            }
        }

        if singular {
            // Não deveria acontecer (P < N, ridge, dedupe); degrada para o
            // acoplamento nulo em vez de publicar lixo.
            self.weights.fill(0.0);
            self.phase_shifts.fill(0.0);
            self.mean_coupling = 0.0;
            return;
        }

        // G⁻¹ (lado direito) já ponderada por λ_μ.
        for mu in 0..p {
            let lambda = self.a_re[mu];
            for nu in 0..p {
                self.gin_re[mu * p + nu] = self.gj_re[mu * w + p + nu] * lambda;
                self.gin_im[mu * p + nu] = self.gj_im[mu * w + p + nu] * lambda;
            }
        }

        // --- A_iν = Σ_μ conj(ψ_i^μ) (G⁻¹)_μν  (N×P) --------------------------
        for i in 0..n {
            for nu in 0..p {
                let mut ar = 0.0f64;
                let mut ai = 0.0f64;
                for mu in 0..p {
                    let pr = self.pat_re[mu * n + i] as f64;
                    let pi = -(self.pat_im[mu * n + i] as f64);
                    let gr = self.gin_re[mu * p + nu];
                    let gi = self.gin_im[mu * p + nu];
                    ar += pr * gr - pi * gi;
                    ai += pr * gi + pi * gr;
                }
                self.a_re[i * p + nu] = ar;
                self.a_im[i * p + nu] = ai;
            }
        }

        // --- C_ij = Σ_ν A_iν ψ_j^ν -------------------------------------------
        for i in 0..n {
            for j in 0..n {
                let mut cr = 0.0f64;
                let mut ci = 0.0f64;
                for nu in 0..p {
                    let pr = self.pat_re[nu * n + j] as f64;
                    let pi = self.pat_im[nu * n + j] as f64;
                    cr += self.a_re[i * p + nu] * pr - self.a_im[i * p + nu] * pi;
                    ci += self.a_re[i * p + nu] * pi + self.a_im[i * p + nu] * pr;
                }
                self.c_re[i * n + j] = cr;
                self.c_im[i * n + j] = ci;
            }
        }

        // --- Gauge: acoplamento médio = 1 -------------------------------------
        let mut sum_mag = 0.0f64;
        let mut count = 0usize;
        for i in 0..n {
            for j in 0..n {
                let idx = i * n + j;
                if i == j || self.pruned[idx] {
                    continue;
                }
                let (re, im) = (self.c_re[idx], self.c_im[idx]);
                sum_mag += (re * re + im * im).sqrt();
                count += 1;
            }
        }
        let mean_mag = if count > 0 {
            (sum_mag / count as f64) as f32
        } else {
            0.0
        };
        self.mean_coupling = mean_mag;
        let gain = if mean_mag > 0.0 && mean_mag.is_finite() {
            1.0 / mean_mag
        } else {
            0.0
        };

        // --- Publica (W, δ) e roda a regra Δδ da especificação ----------------
        for i in 0..n {
            for j in 0..n {
                let idx = i * n + j;
                if i == j || self.pruned[idx] {
                    self.weights[idx] = 0.0;
                    self.phase_shifts[idx] = 0.0;
                    continue;
                }
                let re = self.c_re[idx];
                let im = self.c_im[idx];
                let mag = (re * re + im * im).sqrt() as f32 * gain;
                let w = if mag.is_finite() { mag } else { 0.0 };
                self.weights[idx] = w;
                if w > 0.0 && rate > 0.0 {
                    let target = (im.atan2(re) as f32).rem_euclid(TAU);
                    let mut d = self.phase_shifts[idx];
                    let step = rate * w;
                    for _ in 0..relax_steps {
                        let err = wrap_signed(target - d);
                        d = wrap(d + step * err.sin());
                    }
                    self.phase_shifts[idx] = d;
                } else if w == 0.0 {
                    self.phase_shifts[idx] = 0.0;
                }
            }
        }
        let _ = cap; // capacidade usada nos buffers; documentada por API
    }

    /// Recuperação associativa: reconstrói o padrão completo a partir de uma
    /// pista parcial.
    ///
    /// * `partial_cue`: fases da pista (os nós com `mask[i] == false` ignoram
    ///   o valor dado e partem do estado corrente).
    /// * `mask`: `true` = oscilador grampeado à pista, `false` = oculto.
    /// * retorna `(fases_reconstruídas, E_phase_final)`.
    ///
    /// Entradas inválidas (`NaN`, `dt ≤ 0`, `steps = 0`, tamanho errado,
    /// `n = 0`) são no-op: o estado não é alterado e a energia corrente é
    /// devolvida. Use [`Self::try_recall`] para o diagnóstico.
    pub fn recall(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> (Vec<f32>, f32) {
        let n = self.n;
        let mut out = vec![0.0f32; n];
        let energy = self.recall_into(partial_cue, mask, steps, dt, &mut out);
        (out, energy)
    }

    /// Versão de [`Self::recall`] que **não aloca** após o warmup: escreve em
    /// `out` (que deve ter `n` elementos) e devolve a energia final.
    ///
    /// Contrato: nenhuma alocação de heap, nenhum panic. Se `out.len() != n`,
    /// nada é escrito/integrado e a energia corrente é devolvida
    /// (`try_recall` reporta `InvalidSize`). As fases grampeadas seguem a pista
    /// no referencial girante (`cue_i + ω0·t`); a rotação acumulada fica em
    /// [`Self::rotation`].
    pub fn recall_into(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
        out: &mut [f32],
    ) -> f32 {
        // `out` incompatível: no-op total (não integra, não escreve, não panic).
        if out.len() != self.n {
            return self.phase_energy();
        }
        if self.integrate(partial_cue, mask, steps, dt).is_err() {
            out.copy_from_slice(&self.phases);
            return self.phase_energy();
        }
        out.copy_from_slice(&self.phases);
        self.phase_energy()
    }

    /// Igual a [`Self::recall`], mas com validação explícita.
    pub fn try_recall(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> Result<(Vec<f32>, f32), PhaseMemoryError> {
        self.integrate(partial_cue, mask, steps, dt)?;
        let mut out = vec![0.0f32; self.n];
        out.copy_from_slice(&self.phases);
        Ok((out, self.phase_energy()))
    }

    /// Núcleo da integração, com validação. Devolve `Err` sem tocar no estado.
    fn integrate(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> Result<(), PhaseMemoryError> {
        let n = self.n;
        if n == 0 {
            return Err(PhaseMemoryError::EmptyNetwork);
        }
        if partial_cue.len() != n || mask.len() != n {
            return Err(PhaseMemoryError::InvalidSize);
        }
        if steps == 0 || !dt.is_finite() || dt <= 0.0 {
            return Err(PhaseMemoryError::InvalidIntegration);
        }
        if !self.cfg.omega0.is_finite() || !self.cfg.clamp_strength.is_finite() {
            return Err(PhaseMemoryError::InvalidConfig);
        }
        if partial_cue.iter().any(|c| !c.is_finite()) {
            return Err(PhaseMemoryError::NonFiniteInput);
        }

        // Estado inicial: os nós grampeados assumem a pista; os ocultos partem
        // do estado corrente (continuidade da memória).
        for i in 0..n {
            if mask[i] {
                self.phases[i] = wrap(partial_cue[i]);
            }
        }

        let inv_n = 1.0 / n as f32;
        let omega0 = self.cfg.omega0;
        let mut rot = self.rotation;

        for _ in 0..steps {
            for i in 0..n {
                let mut drive = 0.0f32;
                let base = i * n;
                for j in 0..n {
                    if j == i {
                        continue;
                    }
                    let idx = base + j;
                    let dphi = self.phases[j] - self.phases[i] - self.phase_shifts[idx];
                    drive += self.weights[idx] * dphi.sin();
                }
                drive *= inv_n;

                if mask[i] {
                    // grampo sensorial suave; o alvo gira com o referencial
                    // (ω0·t), então ω0 ≠ 0 não quebra o atrator.
                    let target = wrap(partial_cue[i] + rot);
                    let err = wrap_signed(target - self.phases[i]);
                    drive += self.cfg.clamp_strength * err.sin();
                }

                let dphase = omega0 + drive;
                self.phases[i] = wrap(self.phases[i] + dphase * dt);
            }
            rot = wrap(rot + omega0 * dt);
        }

        self.rotation = rot;
        Ok(())
    }

    /// Evolução livre (sem grampo): útil para verificar que os atratores são
    /// estáveis e para o Módulo 8 (memória contínua).
    ///
    /// Entradas inválidas são no-op; devolve a energia corrente.
    pub fn free_run(&mut self, steps: usize, dt: f32) -> f32 {
        let n = self.n;
        if n == 0 {
            return 0.0;
        }
        let omega0 = self.cfg.omega0;
        if steps == 0 || !dt.is_finite() || dt <= 0.0 || !omega0.is_finite() {
            return self.phase_energy();
        }
        let inv_n = 1.0 / n as f32;
        let mut rot = self.rotation;
        for _ in 0..steps {
            for i in 0..n {
                let mut drive = 0.0f32;
                let base = i * n;
                for j in 0..n {
                    if j == i {
                        continue;
                    }
                    let idx = base + j;
                    let dphi = self.phases[j] - self.phases[i] - self.phase_shifts[idx];
                    drive += self.weights[idx] * dphi.sin();
                }
                self.phases[i] = wrap(self.phases[i] + (omega0 + drive * inv_n) * dt);
            }
            rot = wrap(rot + omega0 * dt);
        }
        self.rotation = rot;
        self.phase_energy()
    }

    /// Poda metabólica **permanente**: arestas abaixo do limiar são zeradas
    /// (peso e `δ`) e ficam marcadas, de modo que a projeção **não as
    /// ressuscita** nos imprints seguintes. Use [`Self::clear_pruning`] para
    /// reabrir a plasticidade.
    ///
    /// Devolve o número real de arestas apagadas (arestas que eram diferentes
    /// de zero — ou não finitas — e passaram a zero). `prune(∞)` apaga todas e
    /// devolve o número de arestas existentes. `threshold = NaN` é no-op.
    pub fn prune(&mut self, threshold: f32) -> usize {
        if threshold.is_nan() {
            return 0;
        }
        let mut cut = 0usize;
        for idx in 0..self.weights.len() {
            let w = self.weights[idx];
            // Podado se está abaixo do limiar ou se não é finito (NaN/∞).
            let remove = w < threshold || !w.is_finite();
            if !remove {
                continue;
            }
            if w != 0.0 || !w.is_finite() {
                cut += 1;
            }
            self.weights[idx] = 0.0;
            self.phase_shifts[idx] = 0.0;
            self.pruned[idx] = true;
        }
        cut
    }

    /// Reabre a plasticidade das arestas podadas (limpa a máscara de poda).
    ///
    /// O acoplamento só é recalculado no próximo `imprint`.
    pub fn clear_pruning(&mut self) {
        self.pruned.fill(false);
    }
}
