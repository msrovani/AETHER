//! E8 — reservatório **esparso** congelado × ajuste conjunto da dinâmica.
//!
//! O Vol. II §6/§10 pergunta se é preciso treinar a física do campo ou apenas o
//! *readout* linear. A literatura de reservoir computing responde com uma
//! assimetria forte:
//!
//! * o reservatório pode ser **aleatório e congelado**; só o readout linear é
//!   estimado, por ridge regression (Lukoševičius & Jaeger, *A tutorial on
//!   reservoir computing*, 2009; Lukoševičius, *A practical guide to applying
//!   echo state networks*, 2012);
//! * a física deve operar perto da borda do caos (`ρ(W) ≈ 1`), com
//!   conectividade **esparsa** (5–12 conexões por nó) — esparsidade reduz o
//!   custo de `O(N²)` para `O(N·k)` sem destruir a memória;
//! * o readout deve ser resolvido por **equações normais + Cholesky** e não por
//!   uma inversa ingênua; RLS é uma alternativa online, mas é sensível ao
//!   condicionamento (Haykin; SPIRES RLS; NGRC, *On the emergence of numerical
//!   instabilities*, 2025).
//!
//! # O que este módulo mede
//!
//! * [`Reservoir`] — matriz esparsa `W` (k conexões/nó) escalada ao raio
//!   espectral alvo, estado `x_{t+1} = (1−α)x_t + α·tanh(Wx_t + w_in u_t)`;
//! * [`RidgeReadout`] — acumula `XᵀX` e `Xᵀy` numa passada e resolve por
//!   Cholesky; o mesmo `XᵀX` serve para varrer `β` **sem reexecutar** o
//!   reservatório;
//! * [`RlsReadout`] — RLS/Sherman–Morrison online, mantido para o regime
//!   não-estacionário (T03) e para comparação;
//! * [`FrozenModel`] — física congelada, readout ridge/RLS;
//! * [`joint_search`] — baseline “conjunto”: busca **sem gradiente** sobre os
//!   parâmetros dinâmicos (`ρ`, `α`, escala de entrada), cada candidato com uma
//!   passada completa de treino. É o custo que o readout congelado evita.
//!
//! # Benchmarks
//!
//! [`mackey_glass`] (τ=17, caos com memória longa) e [`narma10`] (memória
//! não-linear de 10 passos) são os dois padrões da área. [`quasi_periodic`] fica
//! como controle determinístico.
//!
//! # Contrato
//!
//! `step`, `predict_into` e `observe` não alocam após a construção. A única
//! alocação de treino é a matriz `P`/`XᵀX`, dimensionada uma vez.

#![allow(clippy::needless_range_loop)]

use crate::error::{AetherError, Result};
use std::time::Instant;

/// Configuração do reservatório.
#[derive(Clone, Copy, Debug)]
pub struct ReservoirConfig {
    /// Número de neurônios.
    pub n: usize,
    /// Conexões recorrentes por neurônio (esparsidade `k/N`).
    pub degree: usize,
    /// Raio espectral alvo (típico `0.8..1.2`; `1.0` é a referência).
    pub spectral_radius: f32,
    /// Vazamento `α ∈ (0,1]`.
    pub leak: f32,
    /// Escala da entrada.
    pub input_scale: f32,
    /// Semente determinística.
    pub seed: u64,
}

impl Default for ReservoirConfig {
    fn default() -> Self {
        Self {
            n: 100,
            degree: 10,
            spectral_radius: 1.0,
            leak: 0.3,
            input_scale: 0.5,
            seed: 0xA37E_0E80,
        }
    }
}

impl ReservoirConfig {
    /// Sanitiza para valores fisicamente válidos.
    pub fn sanitized(self) -> Self {
        Self {
            n: self.n.max(1),
            degree: self.degree.clamp(1, self.n.saturating_sub(1).max(1)),
            spectral_radius: if self.spectral_radius.is_finite() {
                self.spectral_radius.clamp(0.05, 2.0)
            } else {
                1.0
            },
            leak: if self.leak.is_finite() {
                self.leak.clamp(0.001, 1.0)
            } else {
                0.3
            },
            input_scale: if self.input_scale.is_finite() {
                self.input_scale.clamp(0.0, 10.0)
            } else {
                0.5
            },
            seed: self.seed | 1,
        }
    }
}

/// Reservatório esparso de dinâmica fixa.
#[derive(Clone, Debug)]
pub struct Reservoir {
    cfg: ReservoirConfig,
    /// Pesos recorrentes compactos `n × degree`.
    w: Vec<f32>,
    /// Colunas de destino de cada conexão.
    cols: Vec<u32>,
    /// Pesos de entrada (`n`).
    win: Vec<f32>,
    /// Estado corrente.
    x: Vec<f32>,
    /// Buffer do passo (evita leitura suja).
    scratch: Vec<f32>,
}

impl Reservoir {
    /// Cria a matriz esparsa e a escala ao raio espectral alvo.
    pub fn new(cfg: ReservoirConfig) -> Result<Self> {
        let cfg = cfg.sanitized();
        let n = cfg.n;
        let k = cfg.degree;
        let mut rng = Lcg::new(cfg.seed);
        let mut w = vec![0.0f32; n * k];
        let mut cols = vec![0u32; n * k];
        for i in 0..n {
            for e in 0..k {
                // Sorteia uma coluna diferente de `i` (tenta algumas vezes;
                // duplicata só reduz a densidade efetiva, nunca quebra).
                let mut col = (rng.next_u64() % n as u64) as usize;
                for _ in 0..4 {
                    if col != i {
                        break;
                    }
                    col = (rng.next_u64() % n as u64) as usize;
                }
                cols[i * k + e] = col as u32;
                w[i * k + e] = rng.next_signed();
            }
        }
        // Raio espectral por iteração de potência na matriz esparsa.
        let mut x = vec![0.0f32; n];
        for v in x.iter_mut() {
            *v = rng.next_signed();
        }
        let mut y = vec![0.0f32; n];
        let mut rho = 1.0f32;
        for _ in 0..48 {
            let mut norm = 0.0f32;
            for v in x.iter() {
                norm += v * v;
            }
            norm = norm.sqrt().max(1e-12);
            for v in x.iter_mut() {
                *v /= norm;
            }
            for i in 0..n {
                let mut acc = 0.0f32;
                for e in 0..k {
                    acc += w[i * k + e] * x[cols[i * k + e] as usize];
                }
                y[i] = acc;
            }
            let mut norm_y = 0.0f32;
            for v in y.iter() {
                norm_y += v * v;
            }
            rho = norm_y.sqrt().max(1e-12);
            std::mem::swap(&mut x, &mut y);
        }
        let scale = cfg.spectral_radius / rho.max(1e-9);
        for v in w.iter_mut() {
            *v *= scale;
        }
        let win: Vec<f32> = (0..n)
            .map(|_| rng.next_signed() * cfg.input_scale)
            .collect();
        Ok(Self {
            cfg,
            w,
            cols,
            win,
            x: vec![0.0f32; n],
            scratch: vec![0.0f32; n],
        })
    }

    /// Dimensão do estado.
    pub fn n(&self) -> usize {
        self.cfg.n
    }

    /// Configuração efetiva.
    pub fn config(&self) -> ReservoirConfig {
        self.cfg
    }

    /// Reinicia o estado.
    pub fn reset(&mut self) {
        self.x.fill(0.0);
    }

    /// Avança um passo e devolve o novo estado (sem alocar).
    pub fn step(&mut self, u: f32) -> &[f32] {
        let n = self.cfg.n;
        let k = self.cfg.degree;
        let leak = self.cfg.leak;
        for i in 0..n {
            let mut acc = self.win[i] * u;
            let off = i * k;
            for e in 0..k {
                acc += self.w[off + e] * self.x[self.cols[off + e] as usize];
            }
            self.scratch[i] = acc;
        }
        for i in 0..n {
            self.x[i] = (1.0 - leak) * self.x[i] + leak * self.scratch[i].tanh();
        }
        &self.x
    }

    /// Estado corrente.
    pub fn state(&self) -> &[f32] {
        &self.x
    }
}

/// LCG mínimo (determinístico, sem dependência de `rand` no caminho quente).
#[derive(Clone, Copy, Debug)]
pub struct Lcg {
    state: u64,
}

impl Lcg {
    /// Semente.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    /// Próximo `u64`.
    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// `f32` em `[-1, 1]`.
    pub fn next_signed(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }

    /// `f32` em `[0, 1)`.
    pub fn next_unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Readout linear por **equações normais + Cholesky**.
///
/// Guarda `XᵀX` (dim×dim) e `Xᵀy` (dim), acumulados numa passada. A mesma
/// matriz permite varrer o `β` do ridge sem repetir a simulação do reservatório
/// — o truque padrão de Lukoševičius (2012, §3.2).
#[derive(Clone, Debug)]
pub struct RidgeReadout {
    dim: usize,
    xtx: Vec<f64>,
    xty: Vec<f64>,
    w: Vec<f64>,
    z: Vec<f64>,
    count: u64,
    lambda: f64,
}

impl RidgeReadout {
    /// Cria o acumulador para um estado de dimensão `dim` (bias incluído).
    pub fn new(dim: usize) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("RidgeReadout: dim deve ser > 0".into()));
        }
        let d = dim + 1;
        Ok(Self {
            dim,
            xtx: vec![0.0f64; d * d],
            xty: vec![0.0f64; d],
            w: vec![0.0f64; d],
            z: vec![0.0f64; d],
            count: 0,
            lambda: 1e-6,
        })
    }

    /// Dimensão do estado.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Amostras acumuladas.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Regularização corrente (`λ` por amostra).
    pub fn lambda(&self) -> f64 {
        self.lambda
    }

    /// Acumula um par `(estado, alvo)`; custo `O(dim²)`, sem alocar.
    pub fn observe(&mut self, state: &[f32], y: f32) -> Result<()> {
        if state.len() != self.dim || !y.is_finite() {
            return Err(AetherError::shape([self.dim], [state.len()]));
        }
        let d = self.dim + 1;
        for i in 0..self.dim {
            self.z[i] = f64::from(state[i]);
        }
        self.z[self.dim] = 1.0;
        for i in 0..d {
            let zi = self.z[i];
            let off = i * d;
            for j in i..d {
                self.xtx[off + j] += zi * self.z[j];
            }
            self.xty[i] += zi * f64::from(y);
        }
        self.count += 1;
        Ok(())
    }

    /// Resolve `(XᵀX + λ·T·I) w = Xᵀy` por Cholesky.
    ///
    /// `lambda` é a regularização **por amostra** (a matriz é multiplicada por
    /// `count` para ficar na mesma escala da soma). Valores típicos:
    /// `1e-8..1e-4`. Devolve o `w` em [`Self::weights`].
    pub fn solve(&mut self, lambda: f64) -> Result<()> {
        if self.count == 0 {
            return Err(AetherError::Config(
                "RidgeReadout: nenhuma amostra acumulada".into(),
            ));
        }
        let d = self.dim + 1;
        self.lambda = if lambda.is_finite() && lambda >= 0.0 {
            lambda
        } else {
            1e-6
        };
        let ridge = self.lambda * self.count as f64;
        let mut a = vec![0.0f64; d * d];
        for i in 0..d {
            for j in 0..d {
                a[i * d + j] = if j >= i {
                    self.xtx[i * d + j]
                } else {
                    self.xtx[j * d + i]
                };
            }
            a[i * d + i] += ridge;
        }
        // Jitter progressivo: Cholesky falha se a matriz não for SPD.
        let mut jitter = 0.0f64;
        for _ in 0..6 {
            if jitter > 0.0 {
                for i in 0..d {
                    a[i * d + i] += jitter;
                }
            }
            match cholesky_solve(&mut a, &self.xty, &mut self.w, d) {
                true => return Ok(()),
                false => {
                    jitter = if jitter == 0.0 {
                        ridge.max(1e-12)
                    } else {
                        jitter * 10.0
                    };
                }
            }
        }
        Err(AetherError::Degenerate(
            "RidgeReadout: XtX nao e positiva definida".into(),
        ))
    }

    /// Prediz a saída a partir do estado.
    pub fn predict_into(&self, state: &[f32], out: &mut [f32]) -> Result<()> {
        if state.len() != self.dim || out.is_empty() {
            return Err(AetherError::shape([self.dim], [state.len()]));
        }
        let mut acc = self.w[self.dim];
        for i in 0..self.dim {
            acc += self.w[i] * f64::from(state[i]);
        }
        out[0] = acc as f32;
        Ok(())
    }

    /// Predição alocando um `f32`.
    pub fn predict(&self, state: &[f32]) -> Result<f32> {
        let mut out = [0.0f32; 1];
        self.predict_into(state, &mut out)?;
        Ok(out[0])
    }

    /// Pesos do readout.
    pub fn weights(&self) -> &[f64] {
        &self.w
    }
}

/// Cholesky in-place + substituição; `false` se a matriz não for SPD.
fn cholesky_solve(a: &mut [f64], b: &[f64], w: &mut [f64], d: usize) -> bool {
    // L em `a` (triângulo inferior).
    for i in 0..d {
        for j in 0..=i {
            let mut sum = a[i * d + j];
            for k in 0..j {
                sum -= a[i * d + k] * a[j * d + k];
            }
            if i == j {
                if !sum.is_finite() || sum <= 0.0 {
                    return false;
                }
                a[i * d + j] = sum.sqrt();
            } else {
                let lj = a[j * d + j];
                if !lj.is_finite() || lj.abs() < 1e-15 {
                    return false;
                }
                a[i * d + j] = sum / lj;
            }
        }
    }
    // L y = b
    for i in 0..d {
        let mut sum = b[i];
        for k in 0..i {
            sum -= a[i * d + k] * w[k];
        }
        let lii = a[i * d + i];
        if !lii.is_finite() || lii.abs() < 1e-15 {
            return false;
        }
        w[i] = sum / lii;
    }
    // Lᵀ w = y
    for i in (0..d).rev() {
        let mut sum = w[i];
        for k in (i + 1)..d {
            sum -= a[k * d + i] * w[k];
        }
        let lii = a[i * d + i];
        if !lii.is_finite() || lii.abs() < 1e-15 {
            return false;
        }
        w[i] = sum / lii;
    }
    w.iter().all(|v| v.is_finite())
}

/// RLS/Sherman–Morrison online (opção não-estacionária, como no `T03`).
#[derive(Clone, Debug)]
pub struct RlsReadout {
    dim: usize,
    lambda: f32,
    forget: f32,
    p: Vec<f64>,
    w: Vec<f64>,
    z: Vec<f64>,
    k: Vec<f64>,
    zp: Vec<f64>,
}

impl RlsReadout {
    /// Cria o readout com regularização `lambda` e esquecimento `forget`.
    pub fn new(dim: usize, lambda: f32, forget: f32) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("RlsReadout: dim deve ser > 0".into()));
        }
        let d = dim + 1;
        let lambda = if lambda.is_finite() && lambda > 0.0 {
            lambda
        } else {
            0.05
        };
        let forget = if forget.is_finite() && forget > 0.0 && forget <= 1.0 {
            forget
        } else {
            1.0
        };
        let mut p = vec![0.0f64; d * d];
        for i in 0..d {
            p[i * d + i] = 1.0 / lambda as f64;
        }
        Ok(Self {
            dim,
            lambda,
            forget,
            p,
            w: vec![0.0f64; d],
            z: vec![0.0f64; d],
            k: vec![0.0f64; d],
            zp: vec![0.0f64; d],
        })
    }

    /// Prediz a partir do estado.
    pub fn predict_into(&self, state: &[f32], out: &mut [f32]) -> Result<()> {
        if state.len() != self.dim || out.is_empty() {
            return Err(AetherError::shape([self.dim], [state.len()]));
        }
        let mut acc = self.w[self.dim];
        for i in 0..self.dim {
            acc += self.w[i] * f64::from(state[i]);
        }
        out[0] = acc as f32;
        Ok(())
    }

    /// Atualiza com `(estado, alvo)`.
    pub fn observe(&mut self, state: &[f32], y: f32) -> Result<()> {
        if state.len() != self.dim || !y.is_finite() {
            return Err(AetherError::shape([self.dim], [state.len()]));
        }
        let d = self.dim + 1;
        for i in 0..self.dim {
            self.z[i] = f64::from(state[i]);
        }
        self.z[self.dim] = 1.0;
        for i in 0..d {
            let mut acc = 0.0f64;
            for j in 0..d {
                acc += self.p[i * d + j] * self.z[j];
            }
            self.k[i] = acc;
        }
        let mut den = f64::from(self.lambda);
        for i in 0..d {
            den += self.z[i] * self.k[i];
        }
        if !den.is_finite() || den <= 0.0 {
            return Ok(());
        }
        for i in 0..d {
            self.k[i] /= den;
        }
        let mut pred = self.w[self.dim];
        for i in 0..self.dim {
            pred += self.w[i] * self.z[i];
        }
        let err = f64::from(y) - pred;
        for i in 0..d {
            self.w[i] += self.k[i] * err;
        }
        for j in 0..d {
            let mut acc = 0.0f64;
            for l in 0..d {
                acc += self.z[l] * self.p[l * d + j];
            }
            self.zp[j] = acc;
        }
        for i in 0..d {
            let ki = self.k[i];
            for j in 0..d {
                self.p[i * d + j] -= ki * self.zp[j];
            }
        }
        Ok(())
    }
}

/// Modelo de física congelada: o readout é a única parte treinada.
#[derive(Clone, Debug)]
pub struct FrozenModel {
    reservoir: Reservoir,
    ridge: RidgeReadout,
}

impl FrozenModel {
    /// Cria o par reservatório + readout ridge.
    pub fn new(cfg: ReservoirConfig) -> Result<Self> {
        let reservoir = Reservoir::new(cfg)?;
        let ridge = RidgeReadout::new(cfg.n)?;
        Ok(Self { reservoir, ridge })
    }

    /// Treina o readout numa passada sobre `series[start..start+len]`.
    ///
    /// `washout` estados iniciais são descartados (transiente). O alvo em `t` é
    /// `series[t+1]` (predição um passo à frente).
    pub fn train(
        cfg: ReservoirConfig,
        series: &[f32],
        start: usize,
        len: usize,
        washout: usize,
        lambda: f64,
    ) -> Result<(Self, f64)> {
        if start + len + 1 > series.len() || len <= washout + 1 {
            return Err(AetherError::Config(
                "FrozenModel: janela de treino invalida".into(),
            ));
        }
        let t0 = Instant::now();
        let mut model = Self::new(cfg)?;
        for t in start..(start + len) {
            model.reservoir.step(series[t]);
            if t >= start + washout {
                let s = model.reservoir.state();
                model.ridge.observe(s, series[t + 1])?;
            }
        }
        model.ridge.solve(lambda)?;
        let secs = t0.elapsed().as_secs_f64();
        Ok((model, secs))
    }

    /// Treino supervisionado: o reservatório é dirigido por `inputs[t]` e o
    /// readout aprende `targets[t]` (alinhamento explícito, como no NARMA-10).
    pub fn train_supervised(
        cfg: ReservoirConfig,
        inputs: &[f32],
        targets: &[f32],
        start: usize,
        len: usize,
        washout: usize,
        lambda: f64,
    ) -> Result<(Self, f64)> {
        if start + len > inputs.len()
            || start + len > targets.len()
            || len <= washout + 1
        {
            return Err(AetherError::Config(
                "FrozenModel: janela supervisionada invalida".into(),
            ));
        }
        let t0 = Instant::now();
        let mut model = Self::new(cfg)?;
        for t in start..(start + len) {
            model.reservoir.step(inputs[t]);
            if t >= start + washout {
                let s = model.reservoir.state();
                model.ridge.observe(s, targets[t])?;
            }
        }
        model.ridge.solve(lambda)?;
        let secs = t0.elapsed().as_secs_f64();
        Ok((model, secs))
    }

    /// Predição um passo à frente a partir do estado corrente.
    pub fn predict_into(&self, out: &mut [f32]) -> Result<()> {
        self.ridge.predict_into(self.reservoir.state(), out)
    }

    /// Predição alocando um `f32`.
    pub fn predict(&self) -> Result<f32> {
        self.ridge.predict(self.reservoir.state())
    }

    /// Avança o reservatório (inferência pura, sem tocar no readout).
    pub fn step(&mut self, u: f32) -> &[f32] {
        self.reservoir.step(u)
    }

    /// Estado corrente.
    pub fn state(&self) -> &[f32] {
        self.reservoir.state()
    }

    /// Configuração.
    pub fn config(&self) -> ReservoirConfig {
        self.reservoir.config()
    }

    /// Readout.
    pub fn readout(&self) -> &RidgeReadout {
        &self.ridge
    }

    /// Reinicia o estado.
    pub fn reset(&mut self) {
        self.reservoir.reset();
    }
}

/// Resultado de um braço do benchmark.
#[derive(Clone, Copy, Debug)]
pub struct BenchResult {
    /// Tempo de treino (s).
    pub train_secs: f64,
    /// NRMSE no conjunto de teste.
    pub nrmse: f32,
    /// Predições por segundo (inferência pura, sem atualizar o readout).
    pub predict_per_sec: f64,
    /// Amostras de treino.
    pub train_samples: usize,
    /// Amostras de teste.
    pub test_samples: usize,
    /// Regularização escolhida.
    pub lambda: f64,
}

/// Resultado da busca conjunta.
#[derive(Clone, Copy, Debug)]
pub struct JointOutcome {
    /// Melhor configuração dinâmica.
    pub best: ReservoirConfig,
    /// NRMSE de validação do melhor candidato.
    pub val_nrmse: f32,
    /// Candidatos avaliados.
    pub candidates: usize,
    /// Passadas completas de treino (custo relativo).
    pub training_passes: usize,
    /// Tempo total da busca (s).
    pub search_secs: f64,
}

/// Avalia NRMSE e throughput de inferência **congelada** num trecho.
pub fn evaluate(
    model: &mut FrozenModel,
    series: &[f32],
    start: usize,
    len: usize,
) -> Result<(f32, f64)> {
    if start + len + 1 > series.len() {
        return Err(AetherError::Config(
            "evaluate: janela de teste invalida".into(),
        ));
    }
    model.reset();
    // Aquecimento: o estado precisa de história antes da primeira predição.
    let warm = model.config().n.min(len / 4).max(1);
    for t in start..(start + warm) {
        model.step(series[t]);
    }
    let mut pred = [0.0f32; 1];
    let mut se = 0.0f64;
    let mut count = 0usize;
    let begin = Instant::now();
    for t in (start + warm)..(start + len) {
        model.step(series[t]);
        model.predict_into(&mut pred)?;
        let d = f64::from(pred[0] - series[t + 1]);
        se += d * d;
        count += 1;
    }
    let secs = begin.elapsed().as_secs_f64().max(1e-9);
    let rmse = (se / count.max(1) as f64).sqrt();
    let (_, std) = mean_std(&series[(start + warm + 1)..(start + len + 1)]);
    Ok(((rmse / std) as f32, count as f64 / secs))
}

/// Treina o braço congelado e o avalia no teste.
pub fn bench_frozen(
    series: &[f32],
    cfg: ReservoirConfig,
    train_start: usize,
    train_len: usize,
    test_len: usize,
    washout: usize,
    lambda: f64,
) -> Result<BenchResult> {
    let (mut model, secs) =
        FrozenModel::train(cfg, series, train_start, train_len, washout, lambda)?;
    let test_start = train_start + train_len;
    let (nrmse, pps) = evaluate(&mut model, series, test_start, test_len)?;
    Ok(BenchResult {
        train_secs: secs,
        nrmse,
        predict_per_sec: pps,
        train_samples: train_len.saturating_sub(washout),
        test_samples: test_len,
        lambda,
    })
}

/// Avalia NRMSE com entradas e alvos separados (NARMA-10).
pub fn evaluate_supervised(
    model: &mut FrozenModel,
    inputs: &[f32],
    targets: &[f32],
    start: usize,
    len: usize,
) -> Result<(f32, f64)> {
    if start + len > inputs.len() || start + len > targets.len() {
        return Err(AetherError::Config(
            "evaluate_supervised: janela de teste invalida".into(),
        ));
    }
    model.reset();
    let warm = model.config().n.min(len / 4).max(1);
    for t in start..(start + warm) {
        model.step(inputs[t]);
    }
    let mut pred = [0.0f32; 1];
    let mut se = 0.0f64;
    let mut count = 0usize;
    let begin = Instant::now();
    for t in (start + warm)..(start + len) {
        model.step(inputs[t]);
        model.predict_into(&mut pred)?;
        let d = f64::from(pred[0] - targets[t]);
        se += d * d;
        count += 1;
    }
    let secs = begin.elapsed().as_secs_f64().max(1e-9);
    let rmse = (se / count.max(1) as f64).sqrt();
    let (_, std) = mean_std(&targets[(start + warm)..(start + len)]);
    Ok(((rmse / std) as f32, count as f64 / secs))
}

/// Treina e avalia o braço congelado com entradas/alvos separados.
#[allow(clippy::too_many_arguments)]
pub fn bench_frozen_supervised(
    inputs: &[f32],
    targets: &[f32],
    cfg: ReservoirConfig,
    train_start: usize,
    train_len: usize,
    test_len: usize,
    washout: usize,
    lambda: f64,
) -> Result<BenchResult> {
    let (mut model, secs) = FrozenModel::train_supervised(
        cfg,
        inputs,
        targets,
        train_start,
        train_len,
        washout,
        lambda,
    )?;
    let test_start = train_start + train_len;
    let (nrmse, pps) = evaluate_supervised(&mut model, inputs, targets, test_start, test_len)?;
    Ok(BenchResult {
        train_secs: secs,
        nrmse,
        predict_per_sec: pps,
        train_samples: train_len.saturating_sub(washout),
        test_samples: test_len,
        lambda,
    })
}

/// Baseline “conjunto”: busca **sem gradiente** sobre `(ρ, α, input_scale)`.
///
/// Cada candidato faz uma passada completa de treino (reservatório + ridge).
/// Não usa backprop: é evolução aleatória de parâmetros dinâmicos, o análogo
/// honrado do que o dossiê chama de “ajuste conjunto”. O custo é
/// `candidates × train_len` — a assimetria que o readout congelado evita.
#[allow(clippy::too_many_arguments)]
pub fn joint_search(
    series: &[f32],
    base: ReservoirConfig,
    train_start: usize,
    train_len: usize,
    val_len: usize,
    washout: usize,
    candidates: usize,
    lambda: f64,
) -> Result<JointOutcome> {
    if candidates == 0 {
        return Err(AetherError::Config(
            "joint_search: candidates deve ser > 0".into(),
        ));
    }
    let t0 = Instant::now();
    let mut rng = Lcg::new(base.seed ^ 0xC0FF_EE00);
    let mut best = base;
    let mut best_val = f32::INFINITY;
    for _ in 0..candidates {
        let cfg = ReservoirConfig {
            spectral_radius: 0.70 + 0.55 * rng.next_unit(),
            leak: 0.15 + 0.65 * rng.next_unit(),
            input_scale: 0.10 + 0.90 * rng.next_unit(),
            ..base
        };
        let (mut model, _) =
            FrozenModel::train(cfg, series, train_start, train_len, washout, lambda)?;
        let (val, _) = evaluate(&mut model, series, train_start + train_len, val_len)?;
        if val < best_val {
            best_val = val;
            best = cfg;
        }
    }
    Ok(JointOutcome {
        best,
        val_nrmse: best_val,
        candidates,
        training_passes: candidates,
        search_secs: t0.elapsed().as_secs_f64(),
    })
}

/// Versão supervisionada de [`joint_search`] (entradas e alvos separados).
#[allow(clippy::too_many_arguments)]
pub fn joint_search_supervised(
    inputs: &[f32],
    targets: &[f32],
    base: ReservoirConfig,
    train_start: usize,
    train_len: usize,
    val_len: usize,
    washout: usize,
    candidates: usize,
    lambda: f64,
) -> Result<JointOutcome> {
    if candidates == 0 {
        return Err(AetherError::Config(
            "joint_search_supervised: candidates deve ser > 0".into(),
        ));
    }
    let t0 = Instant::now();
    let mut rng = Lcg::new(base.seed ^ 0xC0FF_EE00);
    let mut best = base;
    let mut best_val = f32::INFINITY;
    for _ in 0..candidates {
        let cfg = ReservoirConfig {
            spectral_radius: 0.70 + 0.55 * rng.next_unit(),
            leak: 0.15 + 0.65 * rng.next_unit(),
            input_scale: 0.10 + 0.90 * rng.next_unit(),
            ..base
        };
        let (mut model, _) = FrozenModel::train_supervised(
            cfg,
            inputs,
            targets,
            train_start,
            train_len,
            washout,
            lambda,
        )?;
        let (val, _) =
            evaluate_supervised(&mut model, inputs, targets, train_start + train_len, val_len)?;
        if val < best_val {
            best_val = val;
            best = cfg;
        }
    }
    Ok(JointOutcome {
        best,
        val_nrmse: best_val,
        candidates,
        training_passes: candidates,
        search_secs: t0.elapsed().as_secs_f64(),
    })
}

/// Série de Mackey–Glass (τ=17, caos com memória longa), RK4 com atraso.
pub fn mackey_glass(n: usize, tau: usize, dt: f64) -> Vec<f32> {
    let beta = 0.2f64;
    let gamma = 0.1f64;
    let power = 10.0f64;
    let tau = tau.max(1);
    let delay = ((tau as f64) / dt).round().max(1.0) as usize;
    let burn = delay * 40;
    let total = n + delay + burn;
    let mut x = vec![1.2f64; total + 2];
    // História inicial **não constante**: com história uniforme o atraso
    // estabiliza o ponto fixo x*=1 e o atrator caótico nunca nasce.
    for (i, v) in x.iter_mut().take(delay).enumerate() {
        *v = 1.0 + 0.25 * (i as f64 * 0.7).sin() + 0.05 * (i as f64 * 0.13).cos();
    }
    // Euler com atraso — a formulação clássica de Mackey–Glass (1977) usada
    // nos benchmarks de ESN; RK4 com atraso congelado amortece o atrator.
    for t in delay..(total - 1) {
        let xd = x[t - delay];
        x[t + 1] = x[t] + dt * (beta * xd / (1.0 + xd.powf(power)) - gamma * x[t]);
    }
    x[burn..burn + n].iter().map(|v| *v as f32).collect()
}

/// NARMA-10: devolve `(entradas, alvos)` com `entradas[t] = u[t]` e
/// `alvos[t] = y[t+1]`. O reservatório deve ser dirigido por `u`, não por `y`.
pub fn narma10(n: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut rng = Lcg::new(seed);
    let total = n + 34;
    let mut u = vec![0.0f32; total];
    for v in u.iter_mut() {
        *v = rng.next_unit() * 0.5;
    }
    let mut y = vec![0.0f32; total];
    for t in 10..(total - 1) {
        let mut sum = 0.0f32;
        for i in 0..10 {
            sum += y[t - i];
        }
        y[t + 1] = 0.3 * y[t] + 0.05 * y[t] * sum + 1.5 * u[t - 9] * u[t] + 0.1;
    }
    let inputs = u[32..32 + n].to_vec();
    let targets = y[33..33 + n].to_vec();
    (inputs, targets)
}

/// Série quasi-periódica determinística (controle).
pub fn quasi_periodic(n: usize, seed: u64) -> Vec<f32> {
    let mut rng = Lcg::new(seed);
    let phase = rng.next_signed() * 0.1;
    (0..n)
        .map(|t| {
            let t = t as f32;
            0.50 * (0.050 * t + phase).sin()
                + 0.30 * (0.130 * t - 0.4 * phase).sin()
                + 0.20 * (0.290 * t + 0.7 * phase).sin()
                + 0.10 * (0.017 * t).sin()
        })
        .collect()
}

fn mean_std(values: &[f32]) -> (f64, f64) {
    let n = values.len().max(1) as f64;
    let mean = values.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let var = values
        .iter()
        .map(|v| {
            let d = f64::from(*v) - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    (mean, var.sqrt().max(1e-6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservatorio_esparso_e_estavel() {
        let cfg = ReservoirConfig {
            n: 32,
            degree: 5,
            ..ReservoirConfig::default()
        };
        let mut r = Reservoir::new(cfg).expect("reservoir");
        for t in 0..500 {
            let s = r.step((t as f32 * 0.05).sin());
            assert!(s.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }

    #[test]
    fn cholesky_resolve_identidade() {
        let d = 4;
        let mut a = vec![0.0f64; d * d];
        for i in 0..d {
            a[i * d + i] = 2.0;
        }
        let b = vec![2.0, 4.0, 6.0, 8.0];
        let mut w = vec![0.0f64; d];
        assert!(cholesky_solve(&mut a, &b, &mut w, d));
        for (i, v) in w.iter().enumerate() {
            assert!((*v - (i as f64 + 1.0)).abs() < 1e-12);
        }
    }

    #[test]
    fn ridge_aprende_uma_reta() {
        let mut r = RidgeReadout::new(2).expect("ridge");
        for k in 0..100 {
            let x = k as f32 / 100.0;
            r.observe(&[x, 1.0 - x], 2.0 * x + 1.0).expect("obs");
        }
        r.solve(1e-9).expect("solve");
        let p = r.predict(&[0.5, 0.5]).expect("pred");
        assert!((p - 2.0).abs() < 1e-3, "pred={p}");
    }
}
