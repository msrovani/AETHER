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
        if self.forgetting < 1.0 {
            let inv = 1.0 / self.forgetting as f64;
            for v in self.p.iter_mut() {
                *v *= inv;
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
        let mut buf = z.to_vec();
        for step in 0..h {
            let mut acc = 0.0f64;
            for i in 0..self.dim {
                acc += self.w[i] * buf[i] as f64;
            }
            let next = acc as f32;
            out[step] = next;
            buf.remove(0);
            buf.push(next);
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
}

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
        }
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
    }
}
