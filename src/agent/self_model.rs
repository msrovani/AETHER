//! Self-model por (tool, cluster de goal): Beta + UCB.
//!
//! Não é rede neural: é contagem bayesiana com exploração. O agente “sabe o
//! que sabe” por ferramenta/contexto — e sabe quando ainda não experimentou
//! o suficiente.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::json::Json;

use super::tool::Goal;

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
#[derive(Debug, Default)]
pub struct SelfModel {
    stats: HashMap<(String, u64), Beta>,
    episodes: u64,
    path: Option<PathBuf>,
}

impl SelfModel {
    /// Cria com política opcional.
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            stats: HashMap::new(),
            episodes: 0,
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
    pub fn update(&mut self, tool: &str, goal: &Goal, success: bool) {
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
        let doc = Json::obj([
            ("version", Json::num(1.0)),
            ("episodes", Json::num(self.episodes as f64)),
            ("stats", Json::Arr(entries)),
        ]);
        let _ = std::fs::write(path, doc.to_text());
    }

    /// Carrega a política, se existir.
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
        let Some(stats) = json.get("stats").and_then(|v| v.as_arr()) else {
            return;
        };
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
}
