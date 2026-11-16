//! E8 — reservatório congelado × ajuste conjunto das dinâmicas.
//!
//! O Vol. II §6/§10 pergunta se é preciso treinar a física do campo ou apenas o
//! *readout* linear. Este módulo implementa os dois lados do teste:
//!
//! * [`Reservoir`] — dinâmica fixa (matriz aleatória escalada ao raio espectral
//!   alvo) e estado `x_{t+1} = (1−leak)·x_t + leak·tanh(W x_t + w_in u_t)`;
//! * [`RlsReadout`] — readout linear atualizado por Sherman–Morrison (RLS com
//!   esquecimento), sem gradiente e sem alocar no passo;
//! * [`FrozenModel`] — física **congelada**, só o readout aprende;
//! * [`JointModel`] — física também ajustada por regra local de Oja (Hebbiana),
//!   o análogo *online* de “ajustar os parâmetros dinâmicos”.
//!
//! O gate do E8 não depende de opinião: mede regressão de acurácia, tempo de
//! treino e throughput de inferência dos dois modelos na mesma série.

#![allow(clippy::needless_range_loop)]

use std::time::Instant;

use crate::error::{AetherError, Result};

/// Configuração do reservatório.
#[derive(Clone, Copy, Debug)]
pub struct ReservoirConfig {
    /// Número de neurônios.
    pub n: usize,
    /// Raio espectral alvo (típico `0.8..1.0`).
    pub spectral_radius: f32,
    /// Vazamento (`0..1`).
    pub leak: f32,
    /// Escala da entrada.
    pub input_scale: f32,
    /// Semente determinística.
    pub seed: u64,
}

impl Default for ReservoirConfig {
    fn default() -> Self {
        Self {
            n: 64,
            spectral_radius: 0.9,
            leak: 0.3,
            input_scale: 0.5,
            seed: 0xA37E_0E80,
        }
    }
}

/// Reservatório de dinâmica fixa (ou plasticamente ajustável).
#[derive(Clone, Debug)]
pub struct Reservoir {
    cfg: ReservoirConfig,
    w: Vec<f32>,
    win: Vec<f32>,
    x: Vec<f32>,
    scratch: Vec<f32>,
}

impl Reservoir {
    /// Cria o reservatório com matriz aleatória escalada.
    pub fn new(cfg: ReservoirConfig) -> Result<Self> {
        if cfg.n == 0 {
            return Err(AetherError::Config("Reservoir: n deve ser > 0".into()));
        }
        let n = cfg.n;
        let mut rng = Lcg::new(cfg.seed);
        let mut w = vec![0.0f32; n * n];
        let mut max_row = 0.0f32;
        for i in 0..n {
            let mut row = 0.0f32;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let v = rng.next_signed();
                w[i * n + j] = v;
                row += v.abs();
            }
            max_row = max_row.max(row);
        }
        let scale = if max_row > 1e-12 {
            cfg.spectral_radius / max_row
        } else {
            0.0
        };
        for v in w.iter_mut() {
            *v *= scale;
        }
        let win = (0..n)
            .map(|_| rng.next_signed() * cfg.input_scale)
            .collect();
        Ok(Self {
            cfg,
            w,
            win,
            x: vec![0.0f32; n],
            scratch: vec![0.0f32; n],
        })
    }

    /// Dimensão do estado.
    pub fn n(&self) -> usize {
        self.cfg.n
    }

    /// Configuração.
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
        let leak = self.cfg.leak.clamp(0.0, 1.0);
        for i in 0..n {
            let off = i * n;
            let mut acc = self.win[i] * u;
            for j in 0..n {
                acc += self.w[off + j] * self.x[j];
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

    /// Ajuste local de Oja (`ΔW_ij = η·x_i·(x_j − x_i·W_ij)`), O(N²).
    ///
    /// É o análogo *online* de treinar a física: muda a matriz recorrente sem
    /// gradiente, usando apenas a atividade local.
    pub fn oja_update(&mut self, eta: f32) {
        let n = self.cfg.n;
        for i in 0..n {
            let xi = self.x[i];
            let off = i * n;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let v = self.w[off + j];
                let nv = v + eta * xi * (self.x[j] - xi * v);
                self.w[off + j] = nv.clamp(-1.5, 1.5);
            }
        }
        // Homeostase: preserva o raio espectral alvo (norma de linha), senão a
        // plasticidade de Oja deriva a dinâmica para fora da borda do caos.
        let mut max_row = 0.0f32;
        for i in 0..n {
            let off = i * n;
            let mut row = 0.0f32;
            for j in 0..n {
                if i != j {
                    row += self.w[off + j].abs();
                }
            }
            max_row = max_row.max(row);
        }
        if max_row > 1e-12 {
            let scale = self.cfg.spectral_radius / max_row;
            for v in self.w.iter_mut() {
                *v *= scale;
            }
        }
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
}

/// Readout linear com RLS (Sherman–Morrison) e esquecimento.
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
    /// Cria o readout para um estado de dimensão `dim` (bias incluído).
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

    /// Prediz a saída a partir do estado.
    pub fn predict_into(&self, state: &[f32], out: &mut [f32]) -> Result<()> {
        if state.len() != self.dim || out.is_empty() {
            return Err(AetherError::shape([self.dim], [state.len()]));
        }
        let d = self.dim;
        let mut acc = self.w[d];
        for i in 0..d {
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

    /// Atualiza o readout com `(estado, alvo)`; zero alocação.
    pub fn observe(&mut self, state: &[f32], y: f32) -> Result<()> {
        if state.len() != self.dim || !y.is_finite() {
            return Err(AetherError::shape([self.dim], [state.len()]));
        }
        let d = self.dim;
        self.z[d] = 1.0;
        for i in 0..d {
            self.z[i] = f64::from(state[i]);
        }
        // Esquecimento: P cresce 1/forget **depois** da atualização.
        // (a inflação antes de k quebrava a identidade de Sherman–Morrison)
        // k = P z / (λ + z'Pz)
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
        // erro a priori
        let mut pred = self.w[d];
        for i in 0..d {
            pred += self.w[i] * self.z[i];
        }
        let err = f64::from(y) - pred;
        for i in 0..d {
            self.w[i] += self.k[i] * err;
        }
        // P ← (P − k z'P) / forget
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
        if self.forget < 1.0 {
            let inv = 1.0 / f64::from(self.forget);
            for v in self.p.iter_mut() {
                *v *= inv;
            }
        }
        Ok(())
    }

    /// Pesos do readout.
    pub fn weights(&self) -> &[f64] {
        &self.w
    }
}

/// Modelo de física congelada: só o readout aprende.
#[derive(Clone, Debug)]
pub struct FrozenModel {
    reservoir: Reservoir,
    readout: RlsReadout,
}

impl FrozenModel {
    /// Cria o par reservatório + readout.
    pub fn new(cfg: ReservoirConfig, lambda: f32, forget: f32) -> Result<Self> {
        let reservoir = Reservoir::new(cfg)?;
        let readout = RlsReadout::new(cfg.n, lambda, forget)?;
        Ok(Self { reservoir, readout })
    }

    /// Passo de treino.
    pub fn observe(&mut self, u: f32, y: f32) -> Result<()> {
        let Self { reservoir, readout } = self;
        let state = reservoir.step(u);
        readout.observe(state, y)
    }

    /// Predição para a próxima entrada.
    pub fn predict_into(&self, state: &[f32], out: &mut [f32]) -> Result<()> {
        self.readout.predict_into(state, out)
    }

    /// Estado do reservatório.
    pub fn state(&self) -> &[f32] {
        self.reservoir.state()
    }

    /// Reservatório subjacente.
    pub fn reservoir(&self) -> &Reservoir {
        &self.reservoir
    }

    /// Readout subjacente.
    pub fn readout(&self) -> &RlsReadout {
        &self.readout
    }

    /// Reinicia o estado.
    pub fn reset(&mut self) {
        self.reservoir.reset();
    }
}

/// Modelo com física ajustada por Oja + readout RLS (baseline “conjunto”).
#[derive(Clone, Debug)]
pub struct JointModel {
    reservoir: Reservoir,
    readout: RlsReadout,
    eta: f32,
}

impl JointModel {
    /// Cria o baseline conjunto.
    pub fn new(cfg: ReservoirConfig, lambda: f32, forget: f32, eta: f32) -> Result<Self> {
        let reservoir = Reservoir::new(cfg)?;
        let readout = RlsReadout::new(cfg.n, lambda, forget)?;
        Ok(Self {
            reservoir,
            readout,
            eta: if eta.is_finite() && eta > 0.0 {
                eta
            } else {
                1e-3
            },
        })
    }

    /// Passo de treino: física (Oja) + readout (RLS).
    pub fn observe(&mut self, u: f32, y: f32) -> Result<()> {
        let Self {
            reservoir,
            readout,
            eta,
        } = self;
        let state = reservoir.step(u);
        readout.observe(state, y)?;
        reservoir.oja_update(*eta);
        Ok(())
    }

    /// Predição.
    pub fn predict_into(&self, state: &[f32], out: &mut [f32]) -> Result<()> {
        self.readout.predict_into(state, out)
    }

    /// Estado.
    pub fn state(&self) -> &[f32] {
        self.reservoir.state()
    }

    /// Reinicia.
    pub fn reset(&mut self) {
        self.reservoir.reset();
    }
}

/// Série não linear determinística e **limitada** para o benchmark.
///
/// `y_{t+1} = 0,55·y_t + 0,25·tanh(2,2·y_{t−2}) + 0,6·u_t·u_{t−3} + 0,1·u_{t−1}`,
/// com `u_t ∈ [−0,5, 0,5]`. Tem memória curta e não linearidade suficiente para
/// separar readout linear de física plástica sem explodir.
pub fn narma_like(n: usize, seed: u64) -> Vec<f32> {
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

/// `(média, desvio-padrão)` de uma fatia.
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

/// Resultado de um braço do benchmark.
#[derive(Clone, Copy, Debug)]
pub struct BenchResult {
    /// Tempo de treino (s).
    pub train_secs: f64,
    /// NRMSE no conjunto de teste.
    pub nrmse: f32,
    /// Predições por segundo (inferência).
    pub predict_per_sec: f64,
}

/// Roda o braço congelado e devolve `(resultado, modelo treinado)`.
pub fn bench_frozen(
    series: &[f32],
    cfg: ReservoirConfig,
    warmup: usize,
    predict_n: usize,
) -> Result<BenchResult> {
    let start = Instant::now();
    let mut model = FrozenModel::new(cfg, 0.03, 1.0)?;
    let train_end = series.len().saturating_sub(predict_n + 1);
    for t in warmup..train_end {
        model.observe(series[t], series[t + 1])?;
    }
    let train_secs = start.elapsed().as_secs_f64();
    let (nrmse, predict_per_sec) = evaluate_frozen(&mut model, series, train_end, predict_n)?;
    Ok(BenchResult {
        train_secs,
        nrmse,
        predict_per_sec,
    })
}

/// Roda o braço conjunto e devolve o resultado.
pub fn bench_joint(
    series: &[f32],
    cfg: ReservoirConfig,
    warmup: usize,
    predict_n: usize,
    eta: f32,
) -> Result<BenchResult> {
    let start = Instant::now();
    let mut model = JointModel::new(cfg, 0.03, 1.0, eta)?;
    let train_end = series.len().saturating_sub(predict_n + 1);
    for t in warmup..train_end {
        model.observe(series[t], series[t + 1])?;
    }
    let train_secs = start.elapsed().as_secs_f64();
    let mut pred = [0.0f32; 1];
    let mut se = 0.0f64;
    let mut count = 0usize;
    let infer_start = Instant::now();
    for t in train_end..(train_end + predict_n) {
        model.observe(series[t], series[t + 1])?;
        let state = model.state();
        model.predict_into(state, &mut pred)?;
        let d = f64::from(pred[0] - series[t + 1]);
        se += d * d;
        count += 1;
    }
    let infer_secs = infer_start.elapsed().as_secs_f64().max(1e-9);
    let rmse = (se / count.max(1) as f64).sqrt();
    let (_, std) = mean_std(&series[train_end..(train_end + predict_n)]);
    let nrmse = (rmse / std) as f32;
    Ok(BenchResult {
        train_secs,
        nrmse,
        predict_per_sec: count as f64 / infer_secs,
    })
}

fn evaluate_frozen(
    model: &mut FrozenModel,
    series: &[f32],
    start: usize,
    predict_n: usize,
) -> Result<(f32, f64)> {
    let mut pred = [0.0f32; 1];
    let mut se = 0.0f64;
    let mut count = 0usize;
    let infer_start = Instant::now();
    for t in start..(start + predict_n) {
        model.observe(series[t], series[t + 1])?;
        let state = model.state();
        model.predict_into(state, &mut pred)?;
        let d = f64::from(pred[0] - series[t + 1]);
        se += d * d;
        count += 1;
    }
    let infer_secs = infer_start.elapsed().as_secs_f64().max(1e-9);
    let rmse = (se / count.max(1) as f64).sqrt();
    let (_, std) = mean_std(&series[start..(start + predict_n)]);
    Ok(((rmse / std) as f32, count as f64 / infer_secs))
}
