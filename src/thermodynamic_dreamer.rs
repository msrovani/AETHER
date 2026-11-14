//! Módulo 6 — Amostrador generativo termodinâmico (GSB na borda do caos).
//!
//! Evolução do solver determinístico (Módulo 1) para um motor de **imaginação**:
//!
//! ```text
//! dx_i = a0 · y_i · dt
//! dy_i = ( −[a0 − a_i(t)] x_i + c0 [Σ_j J_ij x_j + h_i] − γ_damp y_i ) dt
//!        + sqrt(2 T dt) · η_i
//!
//! a_i(t) = a_base(t) · (1 − β_chaos · tanh|Σ_j J_ij x_j|)
//! ```
//!
//! O controle individual `a_i(t)` reduz o "afrouxamento" da bifurcação onde o
//! campo local é forte (borda do caos): o sistema não congela no primeiro
//! mínimo — ele **amostra** a distribuição de Boltzmann
//! `P(s) ∝ e^{−H(s)/T}`.
//!
//! Dois regimes cognitivos na mesma equação:
//! * `T → 0`  — ação: colapso rápido e determinístico;
//! * `T > 0`  — imaginação: `K` trajetórias paralelas (rayon) geram um
//!   conjunto de futuros plausíveis e medem a própria incerteza.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

/// Configuração do sonhador.
#[derive(Clone, Copy, Debug)]
pub struct DreamerConfig {
    /// `a0` da dinâmica (padrão 1.0).
    pub a0: f64,
    /// Amortecimento térmico `γ_damp`.
    pub gamma_damp: f64,
    /// Profundidade do controle na borda do caos `β_chaos` (0 = solver puro).
    pub beta_chaos: f64,
    /// Semente base (cada amostra usa `seed + k`).
    pub seed: u64,
}

impl Default for DreamerConfig {
    fn default() -> Self {
        Self {
            a0: 1.0,
            gamma_damp: 0.35,
            beta_chaos: 0.55,
            seed: 0xA37E_0002,
        }
    }
}

/// Conjunto de futuros amostrados.
#[derive(Clone, Debug)]
pub struct DreamEnsemble {
    /// Amostras `s^(k) ∈ {-1,+1}^N` (k linhas de N spins).
    pub samples: Vec<Vec<i8>>,
    /// Energia de cada amostra.
    pub energies: Vec<f64>,
    /// Energia média ⟨H⟩.
    pub mean_energy: f64,
    /// Incerteza epistêmica `U ∈ [0,1]` (variância média dos spins).
    pub epistemic_uncertainty: f64,
    /// Temperatura usada.
    pub temperature: f64,
}

/// Energia de Ising sobre `J` plana `[N,N]` (f64), campo `h` e spins `s`.
#[inline]
fn ising_energy(j: &[f32], h: &[f32], s: &[i8], n: usize) -> f64 {
    let mut e = 0.0f64;
    for i in 0..n {
        let off = i * n;
        let mut acc = 0.0f64;
        for k in 0..n {
            acc += j[off + k] as f64 * s[k] as f64;
        }
        e -= 0.5 * acc * s[i] as f64;
        e -= h[i] as f64 * s[i] as f64;
    }
    e
}

/// Gaussiana padrão via Box-Muller (determinística com o RNG do crate).
#[inline]
fn normal(rng: &mut ChaCha8Rng) -> f64 {
    let u1 = rng.gen::<f64>().max(f64::MIN_POSITIVE);
    let u2 = rng.gen::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Amostrador generativo termodinâmico.
///
/// Reutiliza o layout plano `N×N` de `J` do Módulo 1 (f32 na fronteira com o
/// Módulo 8; a dinâmica é em f64). Todas as trajetórias são independentes e
/// rodam em paralelo via `rayon`.
pub struct ThermodynamicDreamer {
    n: usize,
    cfg: DreamerConfig,
    /// Energia de referência de um estado aleatório (para comparação).
    rng: ChaCha8Rng,
}

impl std::fmt::Debug for ThermodynamicDreamer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThermodynamicDreamer")
            .field("n", &self.n)
            .field("cfg", &self.cfg)
            .finish()
    }
}

impl ThermodynamicDreamer {
    /// Cria o sonhador para sistemas de dimensão `n`.
    pub fn new(n: usize, cfg: DreamerConfig) -> Self {
        Self {
            n,
            cfg,
            rng: ChaCha8Rng::seed_from_u64(cfg.seed),
        }
    }

    /// Dimensão do sistema.
    pub fn dim(&self) -> usize {
        self.n
    }

    /// Amostra `num_samples` futuros da distribuição de Boltzmann
    /// `P(s) ∝ e^{−H(s)/T}` com `steps` passos de integração por trajetória.
    ///
    /// * `j`: `[N,N]` plano (f32), simétrica, diagonal zero;
    /// * `h`: `[N]` campo externo;
    /// * `temperature`: `T` (0.0 = regime de ação determinístico);
    /// * retorna o [`DreamEnsemble`] com amostras, energias, ⟨H⟩ e incerteza.
    pub fn imagine_futures(
        &mut self,
        j: &[f32],
        h: &[f32],
        temperature: f64,
        num_samples: usize,
        steps: usize,
        dt: f64,
    ) -> DreamEnsemble {
        let n = self.n;
        assert_eq!(j.len(), n * n, "J precisa ser [N,N] plana");
        assert_eq!(h.len(), n, "h precisa ter n entradas");
        assert!(temperature >= 0.0, "T >= 0");

        // normalização c0 = 0.5 / (sqrt(N) · std(J)) — mesma regra do Módulo 1.
        let mut count = 0usize;
        let mut sum = 0.0f64;
        let mut sumsq = 0.0f64;
        for i in 0..n {
            for k in 0..n {
                if i != k {
                    let v = j[i * n + k] as f64;
                    sum += v;
                    sumsq += v * v;
                    count += 1;
                }
            }
        }
        let std_j = if count > 0 {
            let mean = sum / count as f64;
            ((sumsq / count as f64 - mean * mean).max(0.0)).sqrt()
        } else {
            0.0
        };
        let c0 = if std_j > 1e-12 {
            0.5 / ((n as f64).sqrt() * std_j)
        } else {
            0.5 / (n as f64).sqrt()
        };

        let a0 = self.cfg.a0;
        let gamma = self.cfg.gamma_damp;
        let beta = self.cfg.beta_chaos;
        let base_seed = self.cfg.seed;
        let steps_max = steps.max(1);

        // K trajetórias paralelas, cada uma uma realização independente.
        let results: Vec<(Vec<i8>, f64)> = (0..num_samples)
            .into_par_iter()
            .map(|k| {
                let mut rng = ChaCha8Rng::seed_from_u64(base_seed ^ (0x9E37_79B9 + k as u64 * 7919));
                let mut x = vec![0.0f64; n];
                let mut y = vec![0.0f64; n];
                let mut hx = vec![0.0f64; n];
                let mut s = vec![0i8; n];

                // estado inicial: sign de ruído fraco (mesma regra do Módulo 1)
                for v in x.iter_mut() {
                    *v = rng.gen::<f64>() * 0.2 - 0.1;
                }
                for v in y.iter_mut() {
                    *v = rng.gen::<f64>() * 0.2 - 0.1;
                }

                let noise_scale = (2.0 * temperature * dt).sqrt();

                for step in 0..steps_max {
                    let frac = step as f64 / steps_max as f64;
                    // Rampa acelerada: a bifurcação "abre" em 40% do percurso,
                    // deixando 60% do tempo para o relaxamento termodinâmico
                    // (sem isso o momento não se acumula e nada colapsa).
                    let a_base = a0 * (2.5 * frac).min(1.0);

                    // gradiente local + controle na borda do caos
                    for i in 0..n {
                        let off = i * n;
                        let mut acc = h[i] as f64;
                        for kk in 0..n {
                            acc += j[off + kk] as f64 * x[kk];
                        }
                        let ai = a_base * (1.0 - beta * acc.abs().tanh());
                        let restoring = a0 - ai;
                        hx[i] = -restoring * x[i] + c0 * acc - gamma * y[i];
                    }

                    // integração semi-implícita com flutuações de Langevin
                    for i in 0..n {
                        y[i] += dt * hx[i] + noise_scale * normal(&mut rng);
                    }
                    for i in 0..n {
                        x[i] += dt * a0 * y[i];
                        // paredes perfeitamente inelásticas
                        if x[i] > 1.0 {
                            x[i] = 1.0;
                            y[i] = 0.0;
                        } else if x[i] < -1.0 {
                            x[i] = -1.0;
                            y[i] = 0.0;
                        }
                    }
                }

                for i in 0..n {
                    s[i] = if x[i] >= 0.0 { 1 } else { -1 };
                }
                let energy = ising_energy(j, h, &s, n);
                (s, energy)
            })
            .collect();

        let mut samples = Vec::with_capacity(num_samples);
        let mut energies = Vec::with_capacity(num_samples);
        for (s, e) in results {
            samples.push(s);
            energies.push(e);
        }
        let mean_energy = if energies.is_empty() {
            0.0
        } else {
            energies.iter().sum::<f64>() / energies.len() as f64
        };

        // Incerteza epistêmica: variância média dos spins entre os futuros.
        // s ∈ {-1,+1} ⇒ Var ∈ [0,1]; média sobre os N osciladores.
        let epistemic_uncertainty = if num_samples < 2 {
            0.0
        } else {
            let k = num_samples as f64;
            let mut var_sum = 0.0f64;
            for i in 0..n {
                let mut m = 0.0f64;
                for s in &samples {
                    m += s[i] as f64;
                }
                m /= k;
                let mut v = 0.0f64;
                for s in &samples {
                    let d = s[i] as f64 - m;
                    v += d * d;
                }
                v /= k;
                var_sum += v;
            }
            (var_sum / n as f64).clamp(0.0, 1.0)
        };

        DreamEnsemble {
            samples,
            energies,
            mean_energy,
            epistemic_uncertainty,
            temperature,
        }
    }

    /// Energia de um estado aleatório de referência (para os testes).
    pub fn random_reference_energy(&mut self, j: &[f32], h: &[f32], trials: usize) -> f64 {
        let n = self.n;
        let mut best = f64::INFINITY;
        for _ in 0..trials {
            let s: Vec<i8> = (0..n)
                .map(|_| {
                    let bit = self.rng.gen::<f64>() < 0.5;
                    if bit {
                        1
                    } else {
                        -1
                    }
                })
                .collect();
            let e = ising_energy(j, h, &s, n);
            if e < best {
                best = e;
            }
        }
        best
    }
}
