//! Koopman online (EDMD/RLS) e descoberta de equações (SINDy/STLSQ).
//!
//! Substitui os instrumentos do harness por mecanismos do motor:
//!
//! * [`OnlineKoopmanOperator`] — operador linear sobre um *embedding* de
//!   atraso, atualizado por **Sherman-Morrison** (RLS) sem gradiente nem
//!   backpropagation. É o análogo discreto de um Koopman/EDMD online:
//!   `z_{t+1} = A z_t` com atualização recursiva da pseudo-inversa.
//! * [`DelayEmbedding`] — janela de atraso de um sinal contínuo (ring buffer).
//! * [`SindyEquationDiscoverer`] — SINDy com **STLSQ** (sequentially
//!   thresholded least squares) sobre uma biblioteca polinomial, com buffers
//!   reutilizados.
//!
//! Referências: B. O. Koopman (1931); Schmid (2010) DMD; Brunton, Proctor &
//! Kutz, *Discovering governing equations from data by sparse identification
//! of nonlinear dynamical systems*, PNAS 2016 (SINDy); Ljung, *System
//! Identification* (RLS/Sherman-Morrison).

#![allow(clippy::needless_range_loop)]

use crate::error::{AetherError, Result};

/// Dimensão do estado do operador *lifted*.
pub const KOOPMAN_D: usize = 6;
/// Dimensão do dicionário `ψ(x)` do operador *lifted*.
pub const KOOPMAN_P: usize = 20;
/// Fator de esquecimento (`λ`) do operador *lifted*.
pub const KOOPMAN_LAMBDA: f32 = 0.99;

/// Dicionário `ψ: R^6 → R^20`: `[1, x_i (6), x_i² (6), x_i x_{i+1} (6), x_0 x_2 x_4]`.
///
/// Os observáveis lineares ficam em `ψ[1..7]` (decodificação direta de `x`).
pub fn lift_state(x: &[f32], psi: &mut [f32; KOOPMAN_P]) {
    psi[0] = 1.0;
    for i in 0..KOOPMAN_D {
        psi[1 + i] = x[i];
        psi[7 + i] = x[i] * x[i];
        psi[13 + i] = x[i] * x[(i + 1) % KOOPMAN_D];
    }
    psi[19] = x[0] * x[2] * x[4];
}

/// Operador linear online `y = W z` com covariância inversa `P` (RLS).
#[derive(Debug)]
pub struct OnlineKoopmanOperator {
    dim: usize,
    out_dim: usize,
    lambda: f32,
    /// Fator de esquecimento (1.0 = RLS clássico; <1 = adaptação rápida).
    forgetting: f32,
    /// `dim × dim` (ordem C).
    p: Vec<f64>,
    /// `out_dim × dim`.
    w: Vec<f64>,
    pz: Vec<f64>,
    k: Vec<f64>,
    pred: Vec<f64>,
    updates: u64,
}

impl OnlineKoopmanOperator {
    /// Cria o operador com fator de esquecimento `lambda > 0` (RLS).
    pub fn new(dim: usize, out_dim: usize, lambda: f32) -> Result<Self> {
        Self::with_forgetting(dim, out_dim, lambda, 1.0)
    }

    /// Cria o operador com **fator de esquecimento** `forgetting ∈ (0,1]`.
    ///
    /// `forgetting < 1` multiplica `P` por `1/forgetting` a cada passo,
    /// permitindo que o operador troque de regime (adaptação não-estacionária;
    /// Ljung, *System Identification*, cap. 11).
    pub fn with_forgetting(
        dim: usize,
        out_dim: usize,
        lambda: f32,
        forgetting: f32,
    ) -> Result<Self> {
        if dim == 0 || out_dim == 0 {
            return Err(AetherError::Config(
                "Koopman: dim e out_dim devem ser > 0".into(),
            ));
        }
        let lambda = if lambda.is_finite() && lambda > 0.0 {
            lambda
        } else {
            1e-3
        };
        let forgetting = if forgetting.is_finite() && forgetting > 0.0 && forgetting <= 1.0 {
            forgetting
        } else {
            1.0
        };
        let mut p = vec![0.0f64; dim * dim];
        for i in 0..dim {
            p[i * dim + i] = 1.0 / lambda as f64;
        }
        Ok(Self {
            dim,
            out_dim,
            lambda,
            forgetting,
            p,
            w: vec![0.0f64; out_dim * dim],
            pz: vec![0.0f64; dim],
            k: vec![0.0f64; dim],
            pred: vec![0.0f64; out_dim],
            updates: 0,
        })
    }

    /// Fator de esquecimento atual.
    pub fn forgetting(&self) -> f32 {
        self.forgetting
    }

    /// Dimensão do embedding.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Número de atualizações online.
    pub fn updates(&self) -> u64 {
        self.updates
    }

    /// Reinicia os pesos e a covariância (mantém as dimensões).
    pub fn reset(&mut self) {
        for v in self.p.iter_mut() {
            *v = 0.0;
        }
        for i in 0..self.dim {
            self.p[i * self.dim + i] = 1.0 / self.lambda as f64;
        }
        self.w.fill(0.0);
        self.updates = 0;
    }

    fn check(&self, name: &str, v: &[f32]) -> Result<()> {
        if v.len() != self.dim {
            return Err(AetherError::shape([self.dim], [v.len()]));
        }
        if let Some(i) = v.iter().position(|x| !x.is_finite()) {
            return Err(AetherError::Degenerate(format!(
                "Koopman: {name}[{i}] nao finito"
            )));
        }
        Ok(())
    }

    /// Atualiza `W` e `P` com o par `(z, y)` via Sherman-Morrison.
    ///
    /// `k = P z / (λ + z'Pz)`, `P ← P − k (Pz)'`, `W ← W + k (y − Wz)'`.
    /// Zero alocação após a construção.
    pub fn observe(&mut self, z: &[f32], y: &[f32]) -> Result<()> {
        self.check("z", z)?;
        if y.len() != self.out_dim {
            return Err(AetherError::shape([self.out_dim], [y.len()]));
        }
        if let Some(i) = y.iter().position(|v| !v.is_finite()) {
            return Err(AetherError::Degenerate(format!(
                "Koopman: y[{i}] nao finito"
            )));
        }
        let dim = self.dim;
        // Esquecimento exponencial: P cresce 1/f por passo.
        // Anti-windup: só infla `P` enquanto `tr(P)` estiver abaixo do valor
        // inicial (`dim/λ`); direções sem excitação não divergem.
        if self.forgetting < 1.0 {
            let mut tr = 0.0f64;
            for i in 0..dim {
                tr += self.p[i * dim + i];
            }
            if tr < dim as f64 / self.lambda as f64 {
                let inv = 1.0 / self.forgetting as f64;
                for v in self.p.iter_mut() {
                    *v *= inv;
                }
            }
        }
        for i in 0..dim {
            let mut s = 0.0f64;
            for j in 0..dim {
                s += self.p[i * dim + j] * z[j] as f64;
            }
            self.pz[i] = s;
        }
        let mut den = self.lambda as f64;
        for i in 0..dim {
            den += z[i] as f64 * self.pz[i];
        }
        if !den.is_finite() || den <= 0.0 {
            return Ok(());
        }
        for i in 0..dim {
            self.k[i] = self.pz[i] / den;
        }
        for i in 0..dim {
            let ki = self.k[i];
            if ki == 0.0 {
                continue;
            }
            for j in 0..dim {
                self.p[i * dim + j] -= ki * self.pz[j];
            }
        }
        for o in 0..self.out_dim {
            let mut acc = 0.0f64;
            for i in 0..dim {
                acc += self.w[o * dim + i] * z[i] as f64;
            }
            self.pred[o] = acc;
        }
        for o in 0..self.out_dim {
            let err = y[o] as f64 - self.pred[o];
            for i in 0..dim {
                self.w[o * dim + i] += self.k[i] * err;
            }
        }
        self.updates = self.updates.wrapping_add(1);
        Ok(())
    }

    /// Predição `W z` (uma etapa).
    pub fn predict_into(&self, z: &[f32], out: &mut [f32]) -> Result<()> {
        self.check("z", z)?;
        if out.len() != self.out_dim {
            return Err(AetherError::shape([self.out_dim], [out.len()]));
        }
        for o in 0..self.out_dim {
            let mut acc = 0.0f64;
            for i in 0..self.dim {
                acc += self.w[o * self.dim + i] * z[i] as f64;
            }
            out[o] = acc as f32;
        }
        Ok(())
    }

    /// Predição alocando a saída.
    pub fn predict(&self, z: &[f32]) -> Result<Vec<f32>> {
        let mut out = vec![0.0f32; self.out_dim];
        self.predict_into(z, &mut out)?;
        Ok(out)
    }

    /// Rollout de `h` passos sobre um embedding de atraso escalar.
    ///
    /// `z` contém `dim` amostras (mais antiga → mais nova) e `W` prevê a
    /// próxima amostra na primeira saída. Zero alocação em `rollout_into`.
    pub fn rollout_into(&self, z: &[f32], h: usize, out: &mut [f32]) -> Result<()> {
        self.check("z", z)?;
        if out.len() < h {
            return Err(AetherError::shape([h], [out.len()]));
        }
        // Janela deslizante virtual: `buf[i] = z[step+i]` enquanto houver
        // amostras originais, depois as previsões já escritas em `out`.
        for step in 0..h {
            let mut acc = 0.0f64;
            for i in 0..self.dim {
                let k = step + i;
                let v = if k < self.dim {
                    z[k]
                } else {
                    out[k - self.dim]
                };
                acc += self.w[i] * v as f64;
            }
            out[step] = acc as f32;
        }
        Ok(())
    }

    /// Operador **lifted** do sprint: `x ∈ R^6 → ψ(x) ∈ R^20`, `λ = 0.99`.
    ///
    /// `K` (20×20) e `P` (20×20) evoluem por Sherman-Morrison com fator de
    /// esquecimento `0.99`. Use [`observe_state`](Self::observe_state) e
    /// [`predict_steps`](Self::predict_steps) (zero alocação).
    pub fn lifted() -> Result<Self> {
        Self::with_forgetting(KOOPMAN_P, KOOPMAN_P, 1e-2, KOOPMAN_LAMBDA)
    }

    fn require_lifted(&self) -> Result<()> {
        if self.dim != KOOPMAN_P || self.out_dim != KOOPMAN_P {
            return Err(AetherError::Config(
                "Koopman: operador nao e `lifted()` (dim=out_dim=20)".into(),
            ));
        }
        Ok(())
    }

    /// Aprende `K` com o par de estados `(x_t, x_{t+1})`, `x ∈ R^6`.
    pub fn observe_state(&mut self, x: &[f32], x_next: &[f32]) -> Result<()> {
        self.require_lifted()?;
        if x.len() != KOOPMAN_D || x_next.len() != KOOPMAN_D {
            return Err(AetherError::shape([KOOPMAN_D], [x.len().min(x_next.len())]));
        }
        let mut z = [0.0f32; KOOPMAN_P];
        let mut zn = [0.0f32; KOOPMAN_P];
        lift_state(x, &mut z);
        lift_state(x_next, &mut zn);
        self.observe(&z, &zn)
    }

    /// Prevê `horizon` passos: `x_{k+1} = [K ψ(x_k)]_{1..6}` (os observáveis
    /// lineares de `ψ` decodificam `x`). Escreve `horizon × 6` valores em
    /// `out` (linha `k` = `x̂_{t+k+1}`), sem alocar.
    pub fn predict_steps(&self, x0: &[f32], horizon: usize, out: &mut [f32]) -> Result<()> {
        self.require_lifted()?;
        if x0.len() != KOOPMAN_D {
            return Err(AetherError::shape([KOOPMAN_D], [x0.len()]));
        }
        if out.len() < horizon * KOOPMAN_D {
            return Err(AetherError::shape([horizon * KOOPMAN_D], [out.len()]));
        }
        // Re-eleva `x` a cada passo (só as linhas lineares 1..=6 de `K` são
        // necessárias): evita compor o erro do subespaço de observáveis
        // não lineares.
        let mut x = [0.0f32; KOOPMAN_D];
        x.copy_from_slice(x0);
        let mut z = [0.0f32; KOOPMAN_P];
        for k in 0..horizon {
            lift_state(&x, &mut z);
            for d in 0..KOOPMAN_D {
                let row = (1 + d) * KOOPMAN_P;
                let mut acc = 0.0f64;
                for i in 0..KOOPMAN_P {
                    acc += self.w[row + i] * z[i] as f64;
                }
                x[d] = acc as f32;
                out[k * KOOPMAN_D + d] = x[d];
            }
        }
        Ok(())
    }

    /// Rollout alocando a saída.
    pub fn rollout(&self, z: &[f32], h: usize) -> Result<Vec<f32>> {
        let mut out = vec![0.0f32; h];
        self.rollout_into(z, h, &mut out)?;
        Ok(out)
    }
}

/// Embedding de atraso de um sinal escalar (janela deslizante).
#[derive(Clone, Debug)]
pub struct DelayEmbedding {
    dim: usize,
    buf: Vec<f32>,
    pos: usize,
    filled: usize,
}

impl DelayEmbedding {
    /// Cria uma janela de `dim` amostras.
    pub fn new(dim: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config(
                "DelayEmbedding: dim deve ser > 0".into(),
            ));
        }
        Ok(Self {
            dim,
            buf: vec![0.0f32; dim],
            pos: 0,
            filled: 0,
        })
    }

    /// Insere uma amostra.
    pub fn push(&mut self, x: f32) {
        if x.is_finite() {
            self.buf[self.pos] = x;
        }
        self.pos = (self.pos + 1) % self.dim;
        self.filled = (self.filled + 1).min(self.dim);
    }

    /// `true` quando a janela tem `dim` amostras.
    pub fn is_full(&self) -> bool {
        self.filled == self.dim
    }

    /// Copia o vetor na ordem temporal (mais antiga → mais nova).
    pub fn vector_into(&self, out: &mut [f32]) -> Result<()> {
        if out.len() != self.dim {
            return Err(AetherError::shape([self.dim], [out.len()]));
        }
        for k in 0..self.dim {
            out[k] = self.buf[(self.pos + k) % self.dim];
        }
        Ok(())
    }

    /// Vetor alocando a saída.
    pub fn vector(&self) -> Vec<f32> {
        let mut out = vec![0.0f32; self.dim];
        let _ = self.vector_into(&mut out);
        out
    }
}

/// SINDy com STLSQ: biblioteca polinomial + limiarização + mínimos quadrados.
#[derive(Debug)]
pub struct SindyEquationDiscoverer {
    terms: usize,
    theta: Vec<f64>,
    a: Vec<f64>,
    b: Vec<f64>,
    active: Vec<usize>,
    coef: Vec<f64>,
    /// Buffer circular `SINDY_WINDOW × 2` do modo streaming.
    ring: Vec<f64>,
    head: usize,
    filled: usize,
    dt: f64,
    tau_sparse: f64,
    /// `Θ` da janela (`(W−4) × 10`) e alvo `ẋ` (`W−4`), pré-alocados.
    ws_theta: Vec<f64>,
    ws_dx: Vec<f64>,
    xi: [[f64; SINDY_TERMS]; 2],
    /// Termos ativos por equação (para o avaliador especializado).
    active_terms: [[u8; SINDY_TERMS]; 2],
    active_len: [usize; 2],
}

/// Expoentes `(x1^a · x2^b)` de cada termo da biblioteca.
const TERM_EXP: [(u8, u8); SINDY_TERMS] = [
    (0, 0),
    (1, 0),
    (0, 1),
    (2, 0),
    (1, 1),
    (0, 2),
    (3, 0),
    (2, 1),
    (1, 2),
    (0, 3),
];

/// Janela circular do SINDy streaming.
pub const SINDY_WINDOW: usize = 128;
/// Termos da biblioteca polinomial (grau ≤ 3 em 2 variáveis).
pub const SINDY_TERMS: usize = 10;

impl Default for SindyEquationDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

impl SindyEquationDiscoverer {
    /// Biblioteca de 10 termos em `(x1, x2)`.
    pub const TERM_NAMES: [&'static str; 10] = [
        "1", "x1", "x2", "x1^2", "x1*x2", "x2^2", "x1^3", "x1^2*x2", "x1*x2^2", "x2^3",
    ];

    /// Cria o descobridor com buffers para 10 termos.
    pub fn new() -> Self {
        let terms = 10;
        Self {
            terms,
            theta: Vec::new(),
            a: vec![0.0; terms * terms],
            b: vec![0.0; terms],
            active: Vec::with_capacity(terms),
            coef: vec![0.0; terms],
            ring: vec![0.0; SINDY_WINDOW * 2],
            head: 0,
            filled: 0,
            dt: 1e-3,
            tau_sparse: 0.02,
            ws_theta: vec![0.0; (SINDY_WINDOW - 4) * SINDY_TERMS],
            ws_dx: vec![0.0; SINDY_WINDOW - 4],
            xi: [[0.0; SINDY_TERMS]; 2],
            active_terms: [[0u8; SINDY_TERMS]; 2],
            active_len: [0usize; 2],
        }
    }

    /// Descobridor *streaming*: janela circular `W=128`, passo `dt` e limiar
    /// de esparsidade `tau_sparse` (STLSQ).
    pub fn streaming(dt: f32, tau_sparse: f64) -> Result<Self> {
        if !dt.is_finite() || dt <= 0.0 {
            return Err(AetherError::Config("SINDy: dt invalido".into()));
        }
        if !tau_sparse.is_finite() || tau_sparse < 0.0 {
            return Err(AetherError::Config("SINDy: tau_sparse >= 0".into()));
        }
        let mut s = Self::new();
        s.dt = dt as f64;
        s.tau_sparse = tau_sparse;
        Ok(s)
    }

    /// Insere uma amostra `(x1, x2)` no buffer circular (zero alocação).
    pub fn push(&mut self, x: [f32; 2]) {
        if !(x[0].is_finite() && x[1].is_finite()) {
            return;
        }
        self.ring[self.head * 2] = x[0] as f64;
        self.ring[self.head * 2 + 1] = x[1] as f64;
        self.head = (self.head + 1) % SINDY_WINDOW;
        self.filled = (self.filled + 1).min(SINDY_WINDOW);
    }

    /// Amostras na janela.
    pub fn window_len(&self) -> usize {
        self.filled
    }

    /// Coeficientes `Ξ` atuais (`[ẋ1; ẋ2] × 10 termos`).
    pub fn coefficients(&self) -> &[[f64; SINDY_TERMS]; 2] {
        &self.xi
    }

    /// Avalia `ẋ = Θ(x) Ξ` em `x`.
    pub fn rhs(&self, x: [f64; 2]) -> [f64; 2] {
        let th = Self::library(x[0], x[1]);
        let mut d = [0.0f64; 2];
        for c in 0..2 {
            for t in 0..SINDY_TERMS {
                d[c] += th[t] * self.xi[c][t];
            }
        }
        d
    }

    /// Recalcula a lista de termos ativos por equação.
    fn rebuild_active(&mut self) {
        for c in 0..2 {
            let mut n = 0usize;
            for t in 0..SINDY_TERMS {
                if self.xi[c][t] != 0.0 {
                    self.active_terms[c][n] = t as u8;
                    n += 1;
                }
            }
            self.active_len[c] = n;
        }
    }

    /// Avaliador **especializado**: percorre apenas os termos ativos de cada
    /// equação, com potências pré-computadas (sem a biblioteca completa).
    ///
    /// É o caminho recomendado antes de qualquer JIT: para 3–4 termos ativos
    /// ele elimina 6–7 produtos e a soma dos zeros por avaliação.
    pub fn rhs_active(&self, x: [f64; 2]) -> [f64; 2] {
        let (x1, x2) = (x[0], x[1]);
        let p1 = [1.0, x1, x1 * x1, x1 * x1 * x1];
        let p2 = [1.0, x2, x2 * x2, x2 * x2 * x2];
        let mut d = [0.0f64; 2];
        for c in 0..2 {
            let n = self.active_len[c];
            if n == 0 {
                d[c] = self.rhs(x)[c];
                continue;
            }
            for k in 0..n {
                let t = self.active_terms[c][k] as usize;
                let (a, b) = TERM_EXP[t];
                d[c] += self.xi[c][t] * p1[a as usize] * p2[b as usize];
            }
        }
        d
    }

    #[inline]
    fn sample(&self, t: usize, c: usize) -> f64 {
        let start = (self.head + SINDY_WINDOW - self.filled) % SINDY_WINDOW;
        self.ring[((start + t) % SINDY_WINDOW) * 2 + c]
    }

    /// Ajusta `Ξ` na janela atual: derivadas centradas de **4ª ordem**
    /// `ẋ_t = (−x_{t+2} + 8x_{t+1} − 8x_{t−1} + x_{t−2}) / 12dt` e STLSQ
    /// (workspace pré-alocado). Retorna o erro relativo
    /// `‖ẋ − Θ Ξ‖ / ‖ẋ‖` sobre a janela.
    pub fn fit_window(&mut self) -> Result<f64> {
        if self.filled < 4 + 2 * SINDY_TERMS {
            return Err(AetherError::Config("SINDy: janela curta".into()));
        }
        let rows = self.filled - 4;
        let inv = 1.0 / (12.0 * self.dt);
        for r in 0..rows {
            let t = r + 2;
            let x1 = self.sample(t, 0);
            let x2 = self.sample(t, 1);
            let th = Self::library(x1, x2);
            self.ws_theta[r * SINDY_TERMS..(r + 1) * SINDY_TERMS].copy_from_slice(&th);
        }
        let mut num = 0.0f64;
        let mut den = 0.0f64;
        for c in 0..2 {
            for r in 0..rows {
                let t = r + 2;
                self.ws_dx[r] = (-self.sample(t + 2, c) + 8.0 * self.sample(t + 1, c)
                    - 8.0 * self.sample(t - 1, c)
                    + self.sample(t - 2, c))
                    * inv;
            }
            let mut xi = [0.0f64; SINDY_TERMS];
            stlsq_ws(&self.ws_theta, &self.ws_dx, rows, self.tau_sparse, &mut xi);
            for r in 0..rows {
                let mut p = 0.0f64;
                for t in 0..SINDY_TERMS {
                    p += self.ws_theta[r * SINDY_TERMS + t] * xi[t];
                }
                let e = self.ws_dx[r] - p;
                num += e * e;
                den += self.ws_dx[r] * self.ws_dx[r];
            }
            self.xi[c] = xi;
        }
        self.rebuild_active();
        Ok((num / den.max(1e-30)).sqrt())
    }

    /// Avalia a biblioteca em `(x1, x2)`.
    pub fn library(x1: f64, x2: f64) -> [f64; 10] {
        [
            1.0,
            x1,
            x2,
            x1 * x1,
            x1 * x2,
            x2 * x2,
            x1 * x1 * x1,
            x1 * x1 * x2,
            x1 * x2 * x2,
            x2 * x2 * x2,
        ]
    }

    /// Descobre as equações `ẋ1 = Θ c1`, `ẋ2 = Θ c2` a partir de uma série.
    ///
    /// Derivadas por diferença central; STLSQ com até 3 passes. Retorna
    /// `[coef_x1; coef_x2]` (2 × 10).
    pub fn discover(&mut self, x: &[[f32; 2]], dt: f32, threshold: f64) -> Result<Vec<Vec<f64>>> {
        let n = x.len();
        if n < self.terms + 2 {
            return Err(AetherError::Config(
                "SINDy: serie curta para a biblioteca".into(),
            ));
        }
        if !dt.is_finite() || dt <= 0.0 {
            return Err(AetherError::Config("SINDy: dt invalido".into()));
        }
        let rows = n - 2;
        self.theta.clear();
        self.theta.reserve(rows * self.terms);
        let mut y0 = Vec::with_capacity(rows);
        let mut y1 = Vec::with_capacity(rows);
        for i in 1..(n - 1) {
            let x1 = x[i][0] as f64;
            let x2 = x[i][1] as f64;
            self.theta.extend_from_slice(&Self::library(x1, x2));
            y0.push((x[i + 1][0] - x[i - 1][0]) as f64 / (2.0 * dt as f64));
            y1.push((x[i + 1][1] - x[i - 1][1]) as f64 / (2.0 * dt as f64));
        }

        let c0 = self.stlsq(rows, &y0, threshold);
        let c1 = self.stlsq(rows, &y1, threshold);
        self.xi[0][..SINDY_TERMS].copy_from_slice(&c0[..SINDY_TERMS]);
        self.xi[1][..SINDY_TERMS].copy_from_slice(&c1[..SINDY_TERMS]);
        self.rebuild_active();
        Ok(vec![c0, c1])
    }

    fn stlsq(&mut self, rows: usize, y: &[f64], threshold: f64) -> Vec<f64> {
        let terms = self.terms;
        self.coef.fill(0.0);
        self.active.clear();
        for t in 0..terms {
            self.active.push(t);
        }
        for _ in 0..3 {
            let m = self.active.len();
            if m == 0 {
                break;
            }
            // Normal equations restritas aos termos ativos.
            for i in 0..m {
                let ti = self.active[i];
                for j in 0..m {
                    let tj = self.active[j];
                    let mut acc = 0.0f64;
                    for r in 0..rows {
                        acc += self.theta[r * terms + ti] * self.theta[r * terms + tj];
                    }
                    self.a[i * m + j] = acc;
                }
                let mut acc = 0.0f64;
                for r in 0..rows {
                    acc += self.theta[r * terms + ti] * y[r];
                }
                self.b[i] = acc;
            }
            let ridge = 1e-8;
            for i in 0..m {
                self.a[i * m + i] += ridge;
            }
            let Some(sol) = solve_dense(&self.a, &self.b, m) else {
                break;
            };
            self.coef.fill(0.0);
            for i in 0..m {
                self.coef[self.active[i]] = sol[i];
            }
            let mut next = Vec::with_capacity(m);
            for &t in &self.active {
                if self.coef[t].abs() >= threshold {
                    next.push(t);
                }
            }
            if next.len() == self.active.len() {
                break;
            }
            self.active = next;
        }
        self.coef.clone()
    }
}

/// STLSQ em workspace de pilha (zero alocação): `min ‖y − Θ ξ‖` com
/// limiarização sequencial `|ξ_t| < tau` e re-ajuste nos termos ativos.
fn stlsq_ws(theta: &[f64], y: &[f64], rows: usize, tau: f64, xi: &mut [f64; SINDY_TERMS]) {
    const T: usize = SINDY_TERMS;
    let mut active = [true; T];
    for _ in 0..T {
        let mut idx = [0usize; T];
        let mut m = 0usize;
        for t in 0..T {
            if active[t] {
                idx[m] = t;
                m += 1;
            }
        }
        if m == 0 {
            xi.fill(0.0);
            return;
        }
        // Sistema aumentado [A | b] (m × (m+1)), A = ΘᵀΘ + ridge, b = Θᵀy.
        let w = m + 1;
        let mut aug = [0.0f64; T * (T + 1)];
        for i in 0..m {
            for j in i..m {
                let mut acc = 0.0f64;
                for r in 0..rows {
                    acc += theta[r * T + idx[i]] * theta[r * T + idx[j]];
                }
                aug[i * w + j] = acc;
                aug[j * w + i] = acc;
            }
            let mut acc = 0.0f64;
            for r in 0..rows {
                acc += theta[r * T + idx[i]] * y[r];
            }
            aug[i * w + m] = acc;
            aug[i * w + i] += 1e-10;
        }
        if !gauss_jordan_inplace(&mut aug, m) {
            return;
        }
        xi.fill(0.0);
        let mut changed = false;
        for i in 0..m {
            let c = aug[i * w + m];
            if c.abs() < tau {
                active[idx[i]] = false;
                changed = true;
            } else {
                xi[idx[i]] = c;
            }
        }
        if !changed {
            return;
        }
    }
}

/// Gauss-Jordan com pivoteamento parcial sobre `[A | b]` (`m × (m+1)`).
fn gauss_jordan_inplace(aug: &mut [f64], m: usize) -> bool {
    let w = m + 1;
    for col in 0..m {
        let mut piv = col;
        for r in (col + 1)..m {
            if aug[r * w + col].abs() > aug[piv * w + col].abs() {
                piv = r;
            }
        }
        if aug[piv * w + col].abs() < 1e-14 {
            return false;
        }
        if piv != col {
            for j in 0..w {
                aug.swap(col * w + j, piv * w + j);
            }
        }
        let p = aug[col * w + col];
        for j in col..w {
            aug[col * w + j] /= p;
        }
        for r in 0..m {
            if r == col {
                continue;
            }
            let f = aug[r * w + col];
            if f == 0.0 {
                continue;
            }
            for j in col..w {
                aug[r * w + j] -= f * aug[col * w + j];
            }
        }
    }
    true
}

/// Resolve `A x = b` (n ≤ ~16) por eliminação gaussiana com pivoteamento.
fn solve_dense(a: &[f64], b: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut m = vec![0.0f64; n * (n + 1)];
    for i in 0..n {
        for j in 0..n {
            m[i * (n + 1) + j] = a[i * n + j];
        }
        m[i * (n + 1) + n] = b[i];
    }
    for col in 0..n {
        let mut piv = col;
        for r in (col + 1)..n {
            if m[r * (n + 1) + col].abs() > m[piv * (n + 1) + col].abs() {
                piv = r;
            }
        }
        if m[piv * (n + 1) + col].abs() < 1e-14 {
            return None;
        }
        if piv != col {
            for j in 0..(n + 1) {
                m.swap(col * (n + 1) + j, piv * (n + 1) + j);
            }
        }
        let p = m[col * (n + 1) + col];
        for j in col..(n + 1) {
            m[col * (n + 1) + j] /= p;
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = m[r * (n + 1) + col];
            if f == 0.0 {
                continue;
            }
            for j in col..(n + 1) {
                m[r * (n + 1) + j] -= f * m[col * (n + 1) + j];
            }
        }
    }
    let mut x = vec![0.0f64; n];
    for i in 0..n {
        x[i] = m[i * (n + 1) + n];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(s: &mut u64) -> f32 {
        *s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*s >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }

    #[test]
    fn koopman_lifted_preve_sistema_linear_6d() {
        // 3 rotações 2D amortecidas com frequências distintas.
        let th = [0.20f32, 0.35, 0.50];
        let step = |x: &[f32; 6]| -> [f32; 6] {
            let mut y = [0.0f32; 6];
            for b in 0..3 {
                let (s, c) = th[b].sin_cos();
                y[2 * b] = 0.99 * (c * x[2 * b] - s * x[2 * b + 1]);
                y[2 * b + 1] = 0.99 * (s * x[2 * b] + c * x[2 * b + 1]);
            }
            y
        };
        let mut op = OnlineKoopmanOperator::lifted().expect("lifted");
        let mut seed = 7u64;
        for _ in 0..200 {
            let mut x = [0.0f32; 6];
            for v in x.iter_mut() {
                *v = lcg(&mut seed);
            }
            for _ in 0..30 {
                let n = step(&x);
                op.observe_state(&x, &n).expect("observe");
                x = n;
            }
        }
        let x0 = [0.6f32, -0.4, 0.3, 0.5, -0.7, 0.2];
        let mut out = [0.0f32; 6 * 10];
        op.predict_steps(&x0, 10, &mut out).expect("predict");
        let mut x = x0;
        let mut err = 0.0f32;
        for k in 0..10 {
            x = step(&x);
            for d in 0..6 {
                err = err.max((out[k * 6 + d] - x[d]).abs());
            }
        }
        assert!(err < 0.05, "erro max de predicao {err}");
        assert!(op.predict_steps(&x0[..3], 2, &mut out).is_err());
    }

    #[test]
    fn sindy_streaming_recupera_oscilador_duffing() {
        // ẋ1 = x2 ; ẋ2 = −4 x1 − 0.2 x2 − 1.0 x1³ (RK4, dt = 0.01).
        let f = |x: [f64; 2]| [x[1], -4.0 * x[0] - 0.2 * x[1] - 1.0 * x[0].powi(3)];
        let dt = 0.01f64;
        let mut x = [1.0f64, 0.0];
        let mut s = SindyEquationDiscoverer::streaming(dt as f32, 0.05).expect("sindy");
        for _ in 0..(SINDY_WINDOW + 40) {
            s.push([x[0] as f32, x[1] as f32]);
            let k1 = f(x);
            let k2 = f([x[0] + 0.5 * dt * k1[0], x[1] + 0.5 * dt * k1[1]]);
            let k3 = f([x[0] + 0.5 * dt * k2[0], x[1] + 0.5 * dt * k2[1]]);
            let k4 = f([x[0] + dt * k3[0], x[1] + dt * k3[1]]);
            for i in 0..2 {
                x[i] += dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
            }
        }
        assert_eq!(s.window_len(), SINDY_WINDOW);
        let err = s.fit_window().expect("fit");
        assert!(err < 0.04, "erro relativo de dx/dt = {err}");
        let xi = s.coefficients();
        assert!((xi[0][2] - 1.0).abs() < 0.05, "x2 em dx1 = {}", xi[0][2]);
        assert!((xi[1][1] + 4.0).abs() < 0.3, "x1 em dx2 = {}", xi[1][1]);
        assert!((xi[1][6] + 1.0).abs() < 0.3, "x1^3 em dx2 = {}", xi[1][6]);
    }

    #[test]
    fn rls_aprende_ar_e_sindy_recupera_equacoes() {
        // AR(8) estável: x_{t+1} = 0.6 x_t - 0.2 x_{t-1}.
        let mut op = OnlineKoopmanOperator::new(8, 1, 1e-4).expect("op");
        let mut emb = DelayEmbedding::new(8).expect("emb");
        let mut x = vec![0.0f32; 2000];
        x[0] = 1.0;
        x[1] = 0.5;
        for t in 2..x.len() {
            x[t] = 0.6 * x[t - 1] - 0.2 * x[t - 2];
        }
        for t in 0..8 {
            emb.push(x[t]);
        }
        for t in 8..1500 {
            let z = emb.vector();
            op.observe(&z, &[x[t + 1]]).expect("observe");
            emb.push(x[t + 1]);
        }
        let z = emb.vector();
        let p = op.rollout(&z, 1).expect("rollout");
        let err = (p[0] - x[1501]).abs();
        assert!(err < 0.05, "AR erro {err}");

        // SINDy: ẋ1 = -0.5x1 + 1.2x2², ẋ2 = 0.8x1 - 0.3x2³.
        let mut series = Vec::new();
        let (mut x1, mut x2) = (0.8f64, -0.4f64);
        let dt = 0.001;
        for _ in 0..3000 {
            series.push([x1 as f32, x2 as f32]);
            let d1 = -0.5 * x1 + 1.2 * x2 * x2;
            let d2 = 0.8 * x1 - 0.3 * x2 * x2 * x2;
            x1 += d1 * dt;
            x2 += d2 * dt;
        }
        let mut sindy = SindyEquationDiscoverer::new();
        let coef = sindy.discover(&series, dt as f32, 0.02).expect("sindy");
        // Termos ativos esperados: x1 em 1, x2^2 em 5; x1 em 9? (x2^3 = 9).
        assert!((coef[0][1] + 0.5).abs() < 0.05, "c1 x1={}", coef[0][1]);
        assert!((coef[0][5] - 1.2).abs() < 0.05, "c1 x2^2={}", coef[0][5]);
        assert!((coef[1][1] - 0.8).abs() < 0.05, "c2 x1={}", coef[1][1]);
        assert!((coef[1][9] + 0.3).abs() < 0.05, "c2 x2^3={}", coef[1][9]);
        assert_eq!(coef[0][3], 0.0);
        assert_eq!(coef[1][0], 0.0);

        // Avaliador especializado ≡ avaliador completo.
        for x in [[0.37f64, -0.21], [-0.9, 0.4], [0.0, 0.0]] {
            let full = sindy.rhs(x);
            let act = sindy.rhs_active(x);
            assert!((full[0] - act[0]).abs() < 1e-12, "rhs x1 {x:?}");
            assert!((full[1] - act[1]).abs() < 1e-12, "rhs x2 {x:?}");
        }
    }
}
