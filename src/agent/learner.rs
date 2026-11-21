//! A6 — learner de episódios verificados (replay JSONL).
//!
//! Só grava/aplica reward quando `verified=true` (`Feedback.correct.is_some()`).
//! Não treina em `outcome.success` sozinho (reward fake).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::json::Json;

use super::linucb::{goal_features, ContextualTools, LINUCB_DIM};
use super::tool::Goal;

/// Episódio de aprendizado (uma tool × um goal).
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeRecord {
    /// Nome da tool.
    pub tool: String,
    /// Features do goal (`LINUCB_DIM`).
    pub features: Vec<f64>,
    /// Reward ∈ [0,1].
    pub reward: f64,
    /// Só `true` se veio de `Feedback.correct`.
    pub verified: bool,
    /// Cluster do goal.
    pub cluster: u64,
}

impl EpisodeRecord {
    /// Novo episódio verificado a partir do goal.
    pub fn verified(tool: impl Into<String>, goal: &Goal, correct: bool) -> Self {
        Self {
            tool: tool.into(),
            features: goal_features(goal).to_vec(),
            reward: if correct { 1.0 } else { 0.0 },
            verified: true,
            cluster: goal.cluster(),
        }
    }

    fn to_json(&self) -> Json {
        Json::obj([
            ("tool", Json::str(&self.tool)),
            (
                "features",
                Json::Arr(self.features.iter().copied().map(Json::num).collect()),
            ),
            ("reward", Json::num(self.reward)),
            ("verified", Json::Bool(self.verified)),
            ("cluster", Json::num(self.cluster as f64)),
        ])
    }

    fn from_json(v: &Json) -> Option<Self> {
        let tool = v.get("tool")?.as_str()?.to_string();
        let features = match v.get("features")? {
            Json::Arr(items) => items
                .iter()
                .filter_map(|j| j.as_f64())
                .collect::<Vec<_>>(),
            _ => return None,
        };
        if features.len() != LINUCB_DIM {
            return None;
        }
        let reward = v.get("reward")?.as_f64()?;
        let verified = v.get("verified")?.as_bool()?;
        let cluster = v.get("cluster")?.as_f64()? as u64;
        Some(Self {
            tool,
            features,
            reward,
            verified,
            cluster,
        })
    }
}

/// Buffer + journal de episódios.
#[derive(Clone, Debug, Default)]
pub struct Learner {
    path: Option<PathBuf>,
    buffer: Vec<EpisodeRecord>,
}

impl Learner {
    /// Learner em memória (sem journal).
    pub fn memory() -> Self {
        Self::default()
    }

    /// Journal JSONL em `path` (cria pai se preciso).
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut learner = Self {
            path: Some(path.clone()),
            buffer: Vec::new(),
        };
        let _ = learner.load();
        learner
    }

    /// Path do env `AETHER_LEARNER_PATH`, ou memória.
    pub fn from_env() -> Self {
        match std::env::var("AETHER_LEARNER_PATH") {
            Ok(p) if !p.trim().is_empty() => Self::with_path(p.trim()),
            _ => Self::memory(),
        }
    }

    /// Episódios em buffer (inclui só os já carregados/gravados nesta sessão).
    pub fn episodes(&self) -> &[EpisodeRecord] {
        &self.buffer
    }

    /// Contagem de verificados.
    pub fn verified_count(&self) -> usize {
        self.buffer.iter().filter(|e| e.verified).count()
    }

    /// Grava só se `verified`. Retorna `false` se ignorado.
    pub fn record(&mut self, episode: EpisodeRecord) -> bool {
        if !episode.verified {
            return false;
        }
        if let Some(path) = &self.path {
            if append_jsonl(path, &episode.to_json()).is_err() {
                return false;
            }
        }
        self.buffer.push(episode);
        true
    }

    /// Recarrega do journal (substitui buffer).
    pub fn load(&mut self) -> usize {
        let Some(path) = &self.path else {
            return 0;
        };
        let Ok(file) = File::open(path) else {
            return 0;
        };
        let mut out = Vec::new();
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = Json::parse(line) {
                if let Some(ep) = EpisodeRecord::from_json(&v) {
                    if ep.verified {
                        out.push(ep);
                    }
                }
            }
        }
        let n = out.len();
        self.buffer = out;
        n
    }

    /// Replay no bandit contextual (só verificados).
    pub fn replay(&self, tools: &mut ContextualTools) -> usize {
        let mut n = 0usize;
        for ep in &self.buffer {
            if !ep.verified {
                continue;
            }
            let Some(arm) = tools.arm_of(&ep.tool) else {
                continue;
            };
            if ep.features.len() != LINUCB_DIM {
                continue;
            }
            tools
                .bandit_mut()
                .update(arm, &ep.features, ep.reward.clamp(0.0, 1.0));
            n += 1;
        }
        n
    }
}

fn append_jsonl(path: &Path, value: &Json) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", value.to_text())?;
    Ok(())
}
