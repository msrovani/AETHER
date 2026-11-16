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
//!
//! # Contrato de alocação (Fase 2, §1)
//!
//! [`ThermodynamicDreamer::imagine_futures_into`] **não aloca** quando o
//! [`DreamEnsemble`] de saída já foi dimensionado (via
//! [`DreamEnsemble::with_capacity`] ou por uma chamada anterior) e o workspace
//! interno do sonhador já cobre `num_samples · 3 · N` f64. A primeira chamada
//! (ou uma chamada com `num_samples` maior) paga a alocação de *warmup*.
//!
//! # Determinismo
//!
//! Cada amostra `k` roda com `ChaCha8Rng` semeado por
//! `seed_do_ciclo ^ f(k)`; o resultado vai para o slot `k`, portanto é
//! bit-a-bit independente do número de threads do pool rayon.
//!
//! O sonhador mantém um contador `call_index`: a semente do ciclo `c` é
//! `seed ^ (c · 0x9E37_79B9_7F4A_7C15)`. Isso mantém o determinismo **por
//! sequência** (mesma configuração + mesma sequência de chamadas ⇒ mesmos
//! resultados) e evita que todo ciclo com `J` fixo produza exatamente o mesmo
//! sonho. O ciclo `0` reproduz a semente histórica `cfg.seed`.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use crate::error::{AetherError, Result};

/// Configuração do sonhador.
#[derive(Clone, Copy, Debug)]
pub struct DreamerConfig {
    /// `a0` da dinâmica (padrão 1.0).
    pub a0: f64,
    /// Amortecimento térmico `γ_damp`.
    pub gamma_damp: f64,
    /// Profundidade do controle na borda do caos `β_chaos` (0 = solver puro).
    pub beta_chaos: f64,
    /// Semente base (cada amostra usa `seed do ciclo + f(k)`).
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

/// Profundidade do histórico do currículo por progresso.
pub const PROGRESS_HISTORY: usize = 64;

/// Currículo por **progresso de aprendizado** (`d(erro)/dt`).
///
/// O E5 substitui a amostragem por surpresa bruta — que gera passeio caótico
/// atrás de outliers — por uma recompensa intrínseca baseada na **derivada** do
/// erro: só vale a pena reamostrar uma região enquanto o erro está caindo. A
/// intuição é a de Schmidhuber (1991) e Oudeyer & Kaplan (2007): o sinal útil
/// não é "quão surpreendente", mas "quão rápido estou aprendendo".
///
/// O tipo é POD (`Copy`), usa um anel fixo de [`PROGRESS_HISTORY`] erros e não
/// aloca no caminho quente.
#[derive(Clone, Copy, Debug)]
pub struct ProgressCurriculum {
    error_ema: f32,
    prev_ema: f32,
    progress_ema: f32,
    history: [f32; PROGRESS_HISTORY],
    head: usize,
    filled: usize,
    primed: bool,
    progress_gain: f32,
    surprise_penalty: f32,
}

impl Default for ProgressCurriculum {
    fn default() -> Self {
        Self {
            error_ema: 0.0,
            prev_ema: 0.0,
            progress_ema: 0.0,
            history: [0.0; PROGRESS_HISTORY],
            head: 0,
            filled: 0,
            primed: false,
            progress_gain: 2.0,
            surprise_penalty: 0.5,
        }
    }
}

impl ProgressCurriculum {
    /// Currículo com os ganhos do dossiê.
    pub fn new() -> Self {
        Self::default()
    }

    /// Currículo com ganho de progresso e penalidade de surpresa explícitos.
    pub fn with_gains(progress_gain: f32, surprise_penalty: f32) -> Self {
        let mut c = Self::default();
        if progress_gain.is_finite() && progress_gain >= 0.0 {
            c.progress_gain = progress_gain;
        }
        if surprise_penalty.is_finite() && surprise_penalty >= 0.0 {
            c.surprise_penalty = surprise_penalty;
        }
        c
    }

    /// Reinicia o histórico.
    pub fn reset(&mut self) {
        *self = Self {
            progress_gain: self.progress_gain,
            surprise_penalty: self.surprise_penalty,
            ..Self::default()
        };
    }

    /// Erro suavizado corrente.
    pub fn error(&self) -> f32 {
        self.error_ema
    }

    /// Progresso `d(erro)/dt` suavizado (positivo = aprendendo).
    pub fn progress(&self) -> f32 {
        self.progress_ema
    }

    /// Histórico circular de erros (mais recente em `head-1`).
    pub fn history(&self) -> &[f32; PROGRESS_HISTORY] {
        &self.history
    }

    /// Observa um erro e devolve o progresso instantâneo limitado.
    ///
    /// `progress = clamp((erro_anterior − erro_atual) / dt, −1, 1)`. A EMA usa
    /// um fator fixo de `0.2` para não oscilar com o ruído do mini-lote.
    pub fn observe(&mut self, error: f32, dt: f32) -> f32 {
        let e = if error.is_finite() {
            error.max(0.0)
        } else {
            self.error_ema
        };
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 1.0 };
        let instant = if self.primed {
            ((self.prev_ema - e) / dt).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.prev_ema = self.error_ema;
        self.error_ema = if self.primed {
            0.8 * self.error_ema + 0.2 * e
        } else {
            e
        };
        self.progress_ema = 0.8 * self.progress_ema + 0.2 * instant;
        self.history[self.head] = e;
        self.head = (self.head + 1) % PROGRESS_HISTORY;
        self.filled = (self.filled + 1).min(PROGRESS_HISTORY);
        self.primed = true;
        instant
    }

    /// Peso de reamostragem de uma trajetória com surpresa `s`.
    ///
    /// `w = exp(gain · progress) / (1 + penalty · s)`. Progresso negativo reduz
    /// o peso; surpresa alta sem progresso é descontada — o oposto do *surprise
    /// sampling* que o baseline `T03` usa.
    pub fn select_weight(&self, surprise: f32) -> f32 {
        let s = if surprise.is_finite() { surprise.max(0.0) } else { 0.0 };
        let g = (self.progress_gain * self.progress_ema).clamp(-4.0, 4.0);
        g.exp() / (1.0 + self.surprise_penalty * s)
    }

    /// `true` quando a região ainda está rendendo progresso (e vale replay).
    pub fn should_replay(&self, surprise: f32) -> bool {
        self.primed && self.progress_ema > 0.0 && self.select_weight(surprise) > 0.5
    }

    /// Temperatura de amostragem modulada pelo progresso.
    ///
    /// Progresso positivo abre a exploração (`T` sobe até 2×); progresso
    /// negativo a fecha (`T` cai até 0,5×). O limite mantém o sonhador dentro
    /// da borda do caos.
    pub fn temperature(&self, base: f64) -> f64 {
        let g = (self.progress_gain * self.progress_ema).clamp(-1.0, 1.0) as f64;
        (base * (1.0 + 0.5 * g)).clamp(0.0, base * 2.0)
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

impl DreamEnsemble {
    /// Cria um ensemble vazio com `num_samples` linhas de `n` spins já
    /// alocadas e capacidade para `num_samples` energias.
    ///
    /// É o buffer de *warmup* do contrato de zero alocação: depois de uma
    /// chamada de [`ThermodynamicDreamer::imagine_futures_into`] com este
    /// `out`, chamadas subsequentes de mesmo `(n, num_samples)` não alocam.
    pub fn with_capacity(n: usize, num_samples: usize) -> Self {
        let mut samples = Vec::with_capacity(num_samples);
        for _ in 0..num_samples {
            samples.push(vec![0i8; n]);
        }
        Self {
            samples,
            energies: Vec::with_capacity(num_samples),
            mean_energy: 0.0,
            epistemic_uncertainty: 0.0,
            temperature: 0.0,
        }
    }

    /// Ensemble degenerado determinístico (todos os spins em `-1`), usado
    /// como fallback documentado de [`ThermodynamicDreamer::imagine_futures`]
    /// quando os parâmetros são inválidos.
    ///
    /// Nunca contém `NaN`/`inf`; `temperature` é sanitizada (`0.0` se negativa
    /// ou não finita). Não é uma amostra de Boltzmann — é apenas um estado
    /// consistente para consumidores que não tratam erro.
    pub fn degenerate(n: usize, num_samples: usize, temperature: f64) -> Self {
        let t = if temperature.is_finite() && temperature >= 0.0 {
            temperature
        } else {
            0.0
        };
        let mut samples = Vec::with_capacity(num_samples);
        for _ in 0..num_samples {
            samples.push(vec![-1i8; n]);
        }
        Self {
            energies: vec![0.0; num_samples],
            samples,
            mean_energy: 0.0,
            epistemic_uncertainty: 0.0,
            temperature: t,
        }
    }
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

/// Escala de acoplamento `c0 = 0.5 / (sqrt(N) · std(J))` — mesma regra do
/// Módulo 1. `O(N²)`, sem alocação.
fn coupling_scale(j: &[f32], n: usize) -> f64 {
    let mut count = 0usize;
    let mut sum = 0.0f64;
    let mut sumsq = 0.0f64;
    for i in 0..n {
        let off = i * n;
        for k in 0..n {
            if i != k {
                let v = j[off + k] as f64;
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
    if std_j > 1e-12 {
        0.5 / ((n as f64).sqrt() * std_j)
    } else {
        0.5 / (n as f64).sqrt()
    }
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
    /// RNG stateful da referência aleatória (energia média de estados uniformes).
    rng: ChaCha8Rng,
    /// Workspace por amostra: `[x (N), y (N), hx (N)]` f64, fatiado por
    /// `par_chunks_mut(3·N)`. Pré-alocado no warmup, nunca realocado quando o
    /// número de amostras não cresce.
    scratch: Vec<f64>,
    /// Buffer reutilizado por [`Self::random_reference_energy`].
    ref_state: Vec<i8>,
    /// Número de chamadas de amostragem já feitas (varia a semente por ciclo).
    call_index: u64,
}

impl std::fmt::Debug for ThermodynamicDreamer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThermodynamicDreamer")
            .field("n", &self.n)
            .field("cfg", &self.cfg)
            .field("call_index", &self.call_index)
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
            scratch: Vec::new(),
            ref_state: vec![0i8; n],
            call_index: 0,
        }
    }

    /// Dimensão do sistema.
    pub fn dim(&self) -> usize {
        self.n
    }

    /// Número de chamadas de amostragem já realizadas (semente de ciclo).
    pub fn call_index(&self) -> u64 {
        self.call_index
    }

    /// Valida os parâmetros de amostragem sem `panic`.
    ///
    /// Rejeita: `n == 0`, `num_samples == 0`, `steps == 0`, `dt` não finito ou
    /// `<= 0`, `temperature` não finita ou `< 0`, `J.len() != N²`,
    /// `h.len() != N`.
    fn validate(
        &self,
        j: &[f32],
        h: &[f32],
        temperature: f64,
        num_samples: usize,
        steps: usize,
        dt: f64,
    ) -> Result<()> {
        let n = self.n;
        if n == 0 {
            return Err(AetherError::Config(
                "dimensao n deve ser > 0 (dim(0) nao tem dinamica)".into(),
            ));
        }
        if j.len() != n * n {
            return Err(AetherError::shape([n, n], [j.len()]));
        }
        if h.len() != n {
            return Err(AetherError::shape([n], [h.len()]));
        }
        if num_samples == 0 {
            return Err(AetherError::Config(
                "num_samples deve ser > 0 (ensemble vazio nao mede incerteza)".into(),
            ));
        }
        if steps == 0 {
            return Err(AetherError::Config(
                "steps deve ser > 0 (integracao vazia)".into(),
            ));
        }
        if !dt.is_finite() || dt <= 0.0 {
            return Err(AetherError::Config(format!(
                "dt deve ser finito e > 0 (obtido {dt})"
            )));
        }
        if !temperature.is_finite() || temperature < 0.0 {
            return Err(AetherError::Config(format!(
                "temperature deve ser finita e >= 0 (obtido {temperature})"
            )));
        }
        Ok(())
    }

    /// Semente base da chamada atual e avança o contador de ciclos.
    fn next_cycle_seed(&mut self) -> u64 {
        let cycle = self.call_index;
        self.call_index = self.call_index.wrapping_add(1);
        self.cfg.seed ^ cycle.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// Dimensionaliza `out` (amostras/energias) e o workspace interno.
    ///
    /// Só paga alocação quando o tamanho pedido é maior que o já reservado —
    /// esta é a única parte que pode alocar dentro do caminho de amostragem.
    fn prepare(&mut self, out: &mut DreamEnsemble, num_samples: usize) {
        let n = self.n;

        if out.samples.len() > num_samples {
            out.samples.truncate(num_samples);
        }
        out.samples.resize_with(num_samples, Vec::new);
        for s in out.samples.iter_mut() {
            if s.len() != n {
                s.clear();
                s.resize(n, 0);
            }
        }

        out.energies.resize(num_samples, 0.0);

        let needed = num_samples * 3 * n;
        if self.scratch.len() < needed {
            self.scratch.resize(needed, 0.0);
        }
    }

    /// Amostra `num_samples` futuros **em um buffer existente**.
    ///
    /// Sem alocação quando `out` já tem `num_samples` linhas de `N` spins,
    /// `num_samples` energias e o workspace já cobriu `num_samples · 3 · N`.
    /// Caso contrário, dimensiona uma única vez (warmup) e retorna `Ok(())`.
    ///
    /// Encolher `num_samples` descarta as linhas excedentes (sem alocar) e não
    /// preserva o workspace; voltar a crescer depois re-paga o warmup daquelas
    /// linhas. No caminho quente mantenha `num_samples` estável.
    ///
    /// Parâmetros inválidos retornam [`AetherError`] em vez de `panic`/`NaN`
    /// (ver [`Self::try_imagine_futures`]). Os resultados são independentes da
    /// quantidade de threads do pool rayon (cada amostra tem seu slot e sua
    /// semente).
    #[allow(clippy::too_many_arguments)] // espelha a assinatura pública + buffer
    pub fn imagine_futures_into(
        &mut self,
        j: &[f32],
        h: &[f32],
        temperature: f64,
        num_samples: usize,
        steps: usize,
        dt: f64,
        out: &mut DreamEnsemble,
    ) -> Result<()> {
        self.validate(j, h, temperature, num_samples, steps, dt)?;
        self.prepare(out, num_samples);

        let n = self.n;
        let a0 = self.cfg.a0;
        let gamma = self.cfg.gamma_damp;
        let beta = self.cfg.beta_chaos;
        let base_seed = self.next_cycle_seed();
        let noise_scale = (2.0 * temperature * dt).sqrt();
        let c0 = coupling_scale(j, n);

        out.temperature = temperature;

        let scratch = &mut self.scratch[..num_samples * 3 * n];
        let samples = &mut out.samples[..num_samples];
        let energies = &mut out.energies[..num_samples];

        // K trajetórias paralelas, cada uma uma realização independente.
        samples
            .par_iter_mut()
            .enumerate()
            .zip(energies.par_iter_mut())
            .zip(scratch.par_chunks_mut(3 * n))
            .for_each(|(((k, s), energy), w)| {
                let mut rng = ChaCha8Rng::seed_from_u64(
                    base_seed ^ 0x9E37_79B9u64.wrapping_add((k as u64).wrapping_mul(7919)),
                );
                let (x, rest) = w.split_at_mut(n);
                let (y, hx) = rest.split_at_mut(n);

                // estado inicial: sign de ruído fraco (mesma regra do Módulo 1)
                for v in x.iter_mut() {
                    *v = rng.gen::<f64>() * 0.2 - 0.1;
                }
                for v in y.iter_mut() {
                    *v = rng.gen::<f64>() * 0.2 - 0.1;
                }

                for step in 0..steps {
                    let frac = step as f64 / steps as f64;
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
                *energy = ising_energy(j, h, s, n);
            });

        let mut sum = 0.0f64;
        for e in energies.iter() {
            sum += *e;
        }
        out.mean_energy = sum / num_samples as f64;

        // Incerteza epistêmica: variância média dos spins entre os futuros.
        // s ∈ {-1,+1} ⇒ Var ∈ [0,1]; média sobre os N osciladores.
        out.epistemic_uncertainty = if num_samples < 2 || n == 0 {
            0.0
        } else {
            let k = num_samples as f64;
            let mut var_sum = 0.0f64;
            for i in 0..n {
                let mut m = 0.0f64;
                for s in samples.iter() {
                    m += s[i] as f64;
                }
                m /= k;
                let mut v = 0.0f64;
                for s in samples.iter() {
                    let d = s[i] as f64 - m;
                    v += d * d;
                }
                var_sum += v / k;
            }
            (var_sum / n as f64).clamp(0.0, 1.0)
        };

        Ok(())
    }

    /// Versão validada de [`Self::imagine_futures`]: devolve `Err` tipado em
    /// vez de propagar `NaN`/`panic`.
    pub fn try_imagine_futures(
        &mut self,
        j: &[f32],
        h: &[f32],
        temperature: f64,
        num_samples: usize,
        steps: usize,
        dt: f64,
    ) -> Result<DreamEnsemble> {
        self.validate(j, h, temperature, num_samples, steps, dt)?;
        let mut out = DreamEnsemble::with_capacity(self.n, num_samples);
        self.imagine_futures_into(j, h, temperature, num_samples, steps, dt, &mut out)?;
        Ok(out)
    }

    /// Amostra `num_samples` futuros da distribuição de Boltzmann
    /// `P(s) ∝ e^{−H(s)/T}` com `steps` passos de integração por trajetória.
    ///
    /// * `j`: `[N,N]` plano (f32), simétrica, diagonal zero;
    /// * `h`: `[N]` campo externo;
    /// * `temperature`: `T` (0.0 = regime de ação determinístico);
    /// * retorna o [`DreamEnsemble`] com amostras, energias, ⟨H⟩ e incerteza.
    ///
    /// # Parâmetros inválidos
    ///
    /// Esta variante **não faz `panic`**: se a validação falhar (por exemplo
    /// `dt <= 0`, `T` negativo/`NaN`, `steps == 0`, `J`/`h` com tamanho errado,
    /// `n == 0`, `num_samples == 0`), devolve
    /// [`DreamEnsemble::degenerate`] — estado determinístico com todos os spins
    /// em `-1`, energias `0.0` e incerteza `0.0`, sem `NaN`. Use
    /// [`Self::try_imagine_futures`] (ou `imagine_futures_into`) quando o erro
    /// precisa ser diagnosticado.
    pub fn imagine_futures(
        &mut self,
        j: &[f32],
        h: &[f32],
        temperature: f64,
        num_samples: usize,
        steps: usize,
        dt: f64,
    ) -> DreamEnsemble {
        match self.try_imagine_futures(j, h, temperature, num_samples, steps, dt) {
            Ok(ensemble) => ensemble,
            Err(_) => DreamEnsemble::degenerate(self.n, num_samples, temperature),
        }
    }

    /// Energia **média** de `trials` estados aleatórios uniformes
    /// `s_i ∈ {-1,+1}` (referência de comparação para os testes).
    ///
    /// * `trials == 0` ⇒ `0.0`;
    /// * `J`/`h` com shape inesperado ou `n == 0` ⇒ `0.0` (sem `panic`);
    /// * consome o RNG interno (sequência stateful) e reutiliza
    ///   `self.ref_state`, sem alocar por trial.
    pub fn random_reference_energy(&mut self, j: &[f32], h: &[f32], trials: usize) -> f64 {
        let n = self.n;
        if trials == 0 || n == 0 || j.len() != n * n || h.len() != n {
            return 0.0;
        }
        let rng = &mut self.rng;
        let state = &mut self.ref_state;
        let mut acc = 0.0f64;
        for _ in 0..trials {
            for s in state.iter_mut() {
                *s = if rng.gen::<f64>() < 0.5 { 1 } else { -1 };
            }
            acc += ising_energy(j, h, state, n);
        }
        acc / trials as f64
    }
}
