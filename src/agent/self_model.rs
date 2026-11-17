//! Self-model por (tool, cluster de goal): Beta + UCB + calibração (P4).
//!
//! Não é rede neural: é contagem bayesiana com exploração. O agente “sabe o
//! que sabe” por ferramenta/contexto — e sabe quando ainda não experimentou
//! o suficiente.
//!
//! P4 acrescenta uma camada de **calibração** determinística e sem
//! dependências: um ring limitado de amostras `(p_previsto, desfecho)`,
//! métricas (Brier, ECE, tabela de confiabilidade) e uma recalibração
//! escalar de temperatura ajustada por grid search. `success()` continua
//! intocado para não regredir o planner; a versão calibrada vive em
//! [`SelfModel::calibrated_success`].

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use crate::json::Json;

use super::tool::Goal;

/// Capacidade do ring de amostras de calibração.
pub const CALIBRATION_CAPACITY: usize = 512;

/// Número default de bins do ECE/tabela de confiabilidade.
pub const DEFAULT_CALIBRATION_BINS: usize = 10;

/// Menor temperatura considerada no grid de ajuste.
pub const MIN_TEMPERATURE: f64 = 0.25;

/// Maior temperatura considerada no grid de ajuste.
pub const MAX_TEMPERATURE: f64 = 4.0;

/// Passos do grid de temperatura em [`SelfModel::fit_temperature`].
pub const TEMPERATURE_GRID_STEPS: usize = 64;

/// Piso de probabilidade usado em `logit`/log loss (evita `±infinito`).
const PROB_EPS: f64 = 1e-6;

/// `logit` estável de `p` (clampa em `[PROB_EPS, 1 - PROB_EPS]`).
fn logit(p: f64) -> f64 {
    let p = p.clamp(PROB_EPS, 1.0 - PROB_EPS);
    (p / (1.0 - p)).ln()
}

/// Sigmóide estável (sem overflow em `|x|` grande).
fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// Aplica a recalibração de temperatura em espaço logit: `sigmoid(logit(p)/t)`.
///
/// `t <= 0` ou não finito devolve `p` apenas clampado em `[0,1]` (identidade
/// defensiva); `t > 1` suaviza previsões extremas, `t < 1` as aguça.
pub fn recalibrate_probability(p: f64, temperature: f64) -> f64 {
    if !p.is_finite() {
        return 0.5;
    }
    if !temperature.is_finite() || temperature <= 0.0 {
        return p.clamp(0.0, 1.0);
    }
    sigmoid(logit(p) / temperature).clamp(0.0, 1.0)
}

/// Uma amostra de calibração: previsão e desfecho observado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationSample {
    /// Probabilidade prevista, em `(0,1)`.
    pub predicted: f64,
    /// Desfecho observado.
    pub success: bool,
}

impl CalibrationSample {
    /// Desfecho como `1.0` (sucesso) ou `0.0` (falha).
    pub fn observed(&self) -> f64 {
        if self.success {
            1.0
        } else {
            0.0
        }
    }

    /// Erro quadrático desta amostra: `(p - y)^2`.
    pub fn brier(&self) -> f64 {
        let diff = self.predicted - self.observed();
        diff * diff
    }

    /// Log loss desta amostra na temperatura dada.
    pub fn log_loss(&self, temperature: f64) -> f64 {
        let p =
            recalibrate_probability(self.predicted, temperature).clamp(PROB_EPS, 1.0 - PROB_EPS);
        let y = self.observed();
        -(y * p.ln() + (1.0 - y) * (1.0 - p).ln())
    }
}

/// Estatística Beta(α, β).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beta {
    /// Sucessos + prior.
    pub alpha: f64,
    /// Falhas + prior.
    pub beta: f64,
}

impl Default for Beta {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            beta: 1.0,
        }
    }
}

impl Beta {
    /// Média posterior.
    pub fn mean(&self) -> f64 {
        self.alpha / (self.alpha + self.beta)
    }

    /// Observações efetivas.
    pub fn observations(&self) -> f64 {
        (self.alpha + self.beta - 2.0).max(0.0)
    }

    /// Bônus UCB.
    pub fn ucb(&self, total: u64) -> f64 {
        let n = self.observations();
        if n <= 0.0 {
            return 1.0;
        }
        (2.0 * ((total as f64) + 1.0).ln() / n).sqrt().min(1.0)
    }

    /// Atualiza com um resultado verificado.
    pub fn update(&mut self, success: bool) {
        if success {
            self.alpha += 1.0;
        } else {
            self.beta += 1.0;
        }
        // Evita estagnação numérica.
        let scale = self.alpha + self.beta;
        if scale > 10_000.0 {
            self.alpha = self.alpha / scale * 100.0;
            self.beta = self.beta / scale * 100.0;
        }
    }
}

/// Modelo de competência.
#[derive(Debug)]
pub struct SelfModel {
    stats: HashMap<(String, u64), Beta>,
    episodes: u64,
    samples: VecDeque<CalibrationSample>,
    temperature: f64,
    path: Option<PathBuf>,
}

impl Default for SelfModel {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SelfModel {
    /// Cria com política opcional.
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            stats: HashMap::new(),
            episodes: 0,
            samples: VecDeque::new(),
            temperature: 1.0,
            path,
        }
    }

    /// Caminho da política.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Episódios.
    pub fn episodes(&self) -> u64 {
        self.episodes
    }

    /// Média posterior para (tool, goal).
    pub fn success(&self, tool: &str, goal: &Goal) -> f64 {
        self.stats
            .get(&(tool.to_string(), goal.cluster()))
            .copied()
            .unwrap_or_default()
            .mean()
    }

    /// Média posterior **recalibrada** pela temperatura corrente.
    ///
    /// É a leitura recomendada para decisão quando há amostras de calibração;
    /// [`SelfModel::success`] segue intocado (o planner depende dele).
    pub fn calibrated_success(&self, tool: &str, goal: &Goal) -> f64 {
        recalibrate_probability(self.success(tool, goal), self.temperature)
    }

    /// Temperatura corrente (`1.0` = identidade).
    pub fn temperature(&self) -> f64 {
        self.temperature
    }

    /// Define a temperatura. Valores `<= 0` ou não finitos são ignorados.
    pub fn set_temperature(&mut self, temperature: f64) {
        if temperature.is_finite() && temperature > 0.0 {
            self.temperature = temperature;
        }
    }

    /// Ring de amostras de calibração (mais antigas primeiro, tamanho
    /// limitado a [`CALIBRATION_CAPACITY`]).
    pub fn calibration(&self) -> &VecDeque<CalibrationSample> {
        &self.samples
    }

    /// Quantidade de amostras de calibração retidas.
    pub fn calibration_len(&self) -> usize {
        self.samples.len()
    }

    /// Registra uma previsão e seu desfecho observado.
    ///
    /// `predicted` é clampado em `(0,1)`; valores não finitos são descartados.
    /// Ao exceder [`CALIBRATION_CAPACITY`], a amostra mais antiga sai.
    pub fn record_prediction(&mut self, predicted: f64, success: bool) {
        if !predicted.is_finite() {
            return;
        }
        self.samples.push_back(CalibrationSample {
            predicted: predicted.clamp(PROB_EPS, 1.0 - PROB_EPS),
            success,
        });
        while self.samples.len() > CALIBRATION_CAPACITY {
            self.samples.pop_front();
        }
    }

    /// Brier score médio (`None` sem amostras).
    pub fn brier_score(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f64 = self.samples.iter().map(CalibrationSample::brier).sum();
        Some(sum / self.samples.len() as f64)
    }

    /// Log loss médio na temperatura corrente (`None` sem amostras).
    pub fn log_loss(&self) -> Option<f64> {
        self.log_loss_at(self.temperature)
    }

    /// Log loss médio numa temperatura arbitrária (`None` sem amostras ou
    /// temperatura inválida).
    pub fn log_loss_at(&self, temperature: f64) -> Option<f64> {
        if self.samples.is_empty() || !temperature.is_finite() || temperature <= 0.0 {
            return None;
        }
        let sum: f64 = self
            .samples
            .iter()
            .map(|sample| sample.log_loss(temperature))
            .sum();
        Some(sum / self.samples.len() as f64)
    }

    /// Ajusta a temperatura por grid search em `[MIN_TEMPERATURE,
    /// MAX_TEMPERATURE]` com [`TEMPERATURE_GRID_STEPS`] passos, minimizando o
    /// log loss das amostras correntes.
    ///
    /// Devolve a temperatura escolhida (`None` sem amostras). Empates ficam com
    /// a menor temperatura do grid.
    pub fn fit_temperature(&mut self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let steps = TEMPERATURE_GRID_STEPS.max(1);
        let span = MAX_TEMPERATURE - MIN_TEMPERATURE;
        let mut best_temperature = self.temperature;
        let mut best_loss = f64::INFINITY;
        for step in 0..steps {
            let frac = if steps > 1 {
                step as f64 / (steps - 1) as f64
            } else {
                0.0
            };
            let candidate = MIN_TEMPERATURE + span * frac;
            if let Some(loss) = self.log_loss_at(candidate) {
                if loss < best_loss {
                    best_loss = loss;
                    best_temperature = candidate;
                }
            }
        }
        if best_loss.is_finite() {
            self.temperature = best_temperature;
            Some(best_temperature)
        } else {
            None
        }
    }

    /// ECE (expected calibration error) com `bins` bins uniformes em `[0,1]`.
    ///
    /// `None` sem amostras ou com `bins == 0`.
    pub fn expected_calibration_error(&self, bins: usize) -> Option<f64> {
        if bins == 0 || self.samples.is_empty() {
            return None;
        }
        let total = self.samples.len() as f64;
        let ece = self
            .reliability_with_bins(bins)
            .into_iter()
            .map(|(_, mean_pred, empirical, count)| {
                (count as f64 / total) * (mean_pred - empirical).abs()
            })
            .sum();
        Some(ece)
    }

    /// Tabela de confiabilidade com [`DEFAULT_CALIBRATION_BINS`] bins.
    ///
    /// Cada linha é `(limite inferior do bin, média prevista, frequência
    /// empírica, contagem)`; bins vazios são omitidos.
    pub fn reliability(&self) -> Vec<(f64, f64, f64, usize)> {
        self.reliability_with_bins(DEFAULT_CALIBRATION_BINS)
    }

    /// Tabela de confiabilidade com número de bins explícito.
    ///
    /// Cada linha é `(limite inferior do bin, média prevista, frequência
    /// empírica, contagem)`; bins vazios são omitidos. `bins == 0` devolve
    /// vetor vazio.
    pub fn reliability_with_bins(&self, bins: usize) -> Vec<(f64, f64, f64, usize)> {
        if bins == 0 {
            return Vec::new();
        }
        let mut counts = vec![0usize; bins];
        let mut sum_pred = vec![0.0f64; bins];
        let mut sum_observed = vec![0.0f64; bins];
        for sample in &self.samples {
            let slot = (sample.predicted * bins as f64).floor().max(0.0) as usize;
            let slot = slot.min(bins - 1);
            counts[slot] += 1;
            sum_pred[slot] += sample.predicted;
            sum_observed[slot] += sample.observed();
        }
        let mut rows = Vec::new();
        for (index, &count) in counts.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let n = count as f64;
            rows.push((
                index as f64 / bins as f64,
                sum_pred[index] / n,
                sum_observed[index] / n,
                count,
            ));
        }
        rows
    }

    /// UCB para (tool, goal).
    pub fn ucb(&self, tool: &str, goal: &Goal) -> f64 {
        self.stats
            .get(&(tool.to_string(), goal.cluster()))
            .copied()
            .unwrap_or_default()
            .ucb(self.episodes)
    }

    /// Observações.
    pub fn observations(&self, tool: &str, goal: &Goal) -> f64 {
        self.stats
            .get(&(tool.to_string(), goal.cluster()))
            .map_or(0.0, |b| b.observations())
    }

    /// Atualiza com recompensa verificada.
    ///
    /// A probabilidade prevista **antes** da atualização é gravada como amostra
    /// de calibração junto do desfecho (ring limitado).
    pub fn update(&mut self, tool: &str, goal: &Goal, success: bool) {
        let predicted = self.success(tool, goal);
        self.record_prediction(predicted, success);
        let key = (tool.to_string(), goal.cluster());
        self.stats.entry(key).or_default().update(success);
        self.episodes = self.episodes.saturating_add(1);
    }

    /// Snapshot legível.
    pub fn snapshot(&self) -> Vec<(String, u64, f64, f64)> {
        self.stats
            .iter()
            .map(|((tool, cluster), beta)| {
                (tool.clone(), *cluster, beta.mean(), beta.observations())
            })
            .collect()
    }

    /// Persiste em JSON (se configurado).
    pub fn persist(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let entries: Vec<Json> = self
            .stats
            .iter()
            .map(|((tool, cluster), beta)| {
                Json::obj([
                    ("tool", Json::str(tool.clone())),
                    ("cluster", Json::str(format!("{cluster:016x}"))),
                    ("alpha", Json::num(beta.alpha)),
                    ("beta", Json::num(beta.beta)),
                ])
            })
            .collect();
        let samples: Vec<Json> = self
            .samples
            .iter()
            .map(|sample| {
                Json::Arr(vec![
                    Json::num(sample.predicted),
                    Json::num(sample.observed()),
                ])
            })
            .collect();
        let doc = Json::obj([
            ("version", Json::num(2.0)),
            ("episodes", Json::num(self.episodes as f64)),
            ("temperature", Json::num(self.temperature)),
            ("stats", Json::Arr(entries)),
            ("calibration", Json::Arr(samples)),
        ]);
        let _ = std::fs::write(path, doc.to_text());
    }

    /// Carrega a política, se existir.
    ///
    /// Arquivos antigos (sem `calibration`/`temperature`) continuam válidos:
    /// campos ausentes caem nos defaults (ring vazio, temperatura `1.0`).
    pub fn load(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(json) = Json::parse(&text) else {
            return;
        };
        if let Some(episodes) = json.get("episodes").and_then(|v| v.as_u64()) {
            self.episodes = episodes;
        }
        if let Some(temperature) = json.get("temperature").and_then(|v| v.as_f64()) {
            if temperature.is_finite() && temperature > 0.0 {
                self.temperature = temperature;
            }
        }
        if let Some(stats) = json.get("stats").and_then(|v| v.as_arr()) {
            for entry in stats {
                let Some(tool) = entry.get("tool").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(cluster) = entry
                    .get("cluster")
                    .and_then(|v| v.as_str())
                    .and_then(|v| u64::from_str_radix(v, 16).ok())
                else {
                    continue;
                };
                let alpha = entry.get("alpha").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let beta = entry.get("beta").and_then(|v| v.as_f64()).unwrap_or(1.0);
                if alpha.is_finite() && beta.is_finite() && alpha > 0.0 && beta > 0.0 {
                    self.stats
                        .insert((tool.to_string(), cluster), Beta { alpha, beta });
                }
            }
        }
        if let Some(calibration) = json.get("calibration").and_then(|v| v.as_arr()) {
            self.samples.clear();
            for entry in calibration {
                let Some(pair) = entry.as_arr() else {
                    continue;
                };
                let Some(predicted) = pair.first().and_then(|v| v.as_f64()) else {
                    continue;
                };
                let Some(observed) = pair.get(1).and_then(|v| v.as_f64()) else {
                    continue;
                };
                if !predicted.is_finite() || !observed.is_finite() {
                    continue;
                }
                self.samples.push_back(CalibrationSample {
                    predicted: predicted.clamp(PROB_EPS, 1.0 - PROB_EPS),
                    success: observed >= 0.5,
                });
                while self.samples.len() > CALIBRATION_CAPACITY {
                    self.samples.pop_front();
                }
            }
        }
    }
}
