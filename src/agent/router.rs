//! Gate MoE: mistura de especialistas com confiabilidade aprendida (EXP3).
//!
//! Não há camada densa nem backpropagation. O gate combina três sinais:
//!
//! * **competência declarada** — `probe` do especialista para a tarefa;
//! * **confiabilidade aprendida** — peso EXP3 atualizado por feedback;
//! * **custo** — implícito no probe (especialistas caros declaram menos).
//!
//! A seleção é amostrada sem reposição (exploração explícita) e o update é o
//! EXP3 clássico: `w_i ← w_i · exp(γ · r / (K · p_i))`. O resultado é um
//! **mixture-of-experts de inferência** — os especialistas são determinísticos,
//! o que aprende é o roteamento.

use std::path::PathBuf;

use crate::json::Json;

use super::ExpertId;

/// Número de especialistas conhecidos.
const SLOTS: usize = 5;

/// Roteador MoE.
#[derive(Debug)]
pub struct MoERouter {
    weights: [f64; SLOTS],
    gamma: f64,
    episodes: u64,
    rng: u64,
    path: Option<PathBuf>,
    selected: Vec<(ExpertId, f64)>,
}

impl MoERouter {
    /// Cria o roteador (pesos uniformes).
    pub fn new(seed: u64, path: Option<PathBuf>) -> Self {
        let mut router = Self {
            weights: [1.0; SLOTS],
            gamma: 0.15,
            episodes: 0,
            rng: seed | 1,
            path,
            selected: Vec::new(),
        };
        router.normalize();
        router
    }

    /// Ajusta o fator de exploração.
    pub fn with_gamma(mut self, gamma: f64) -> Self {
        if gamma.is_finite() && gamma > 0.0 && gamma < 1.0 {
            self.gamma = gamma;
        }
        self
    }

    /// Pesos na ordem canônica de [`ExpertId`].
    pub fn weights(&self) -> Vec<f32> {
        self.weights.iter().map(|w| *w as f32).collect()
    }

    /// Peso de um especialista.
    pub fn weight_of(&self, id: ExpertId) -> f32 {
        self.weights[id.code() as usize] as f32
    }

    /// Episódios de aprendizado.
    pub fn episodes(&self) -> u64 {
        self.episodes
    }

    /// Caminho da política.
    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Define/limpa o caminho da política.
    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Seleciona até `k` especialistas (sem repetir `used`).
    ///
    /// Registra a probabilidade de cada escolha para o update EXP3.
    pub fn select(&mut self, probes: &[f32], k: usize, used: &[ExpertId]) -> Vec<usize> {
        let mut pool: Vec<(usize, f64)> = probes
            .iter()
            .enumerate()
            .filter(|(i, p)| **p > 0.01 && !used.iter().any(|id| id.code() as usize == *i))
            .map(|(i, probe)| {
                let w = self.weights[i];
                (i, w * (f64::from(*probe) + 0.05))
            })
            .collect();
        if pool.is_empty() {
            return Vec::new();
        }
        let total: f64 = pool.iter().map(|(_, score)| score).sum();
        let mut picks = Vec::new();
        let k = k.max(1).min(pool.len());
        for _ in 0..k {
            let mut draw = self.uniform() * total;
            let mut chosen = pool.len() - 1;
            for (idx, (_, score)) in pool.iter().enumerate() {
                if draw < *score {
                    chosen = idx;
                    break;
                }
                draw -= *score;
            }
            let (slot, score) = pool.remove(chosen);
            let prob = (score / total).max(1e-9);
            self.selected.push((expert_of(slot), prob));
            picks.push(slot);
        }
        picks
    }

    /// Atualiza os pesos com o reward (`0..=1`) das escolhas do último passo.
    pub fn update(&mut self, selected: &[ExpertId], reward: f32) {
        let reward = f64::from(reward.clamp(0.0, 1.0));
        // Prefere as probabilidades registradas na seleção; se o chamador
        // passou apenas as IDs, usa uma estimativa uniforme.
        let recorded: Vec<(ExpertId, f64)> = if self.selected.is_empty() {
            selected
                .iter()
                .map(|id| {
                    let n = selected.len().max(1) as f64;
                    (*id, 1.0 / n)
                })
                .collect()
        } else {
            std::mem::take(&mut self.selected)
        };
        let k = recorded.len().max(1) as f64;
        for (id, p) in recorded {
            let idx = id.code() as usize;
            let factor = (self.gamma * reward / (k * p.max(1e-9))).exp();
            self.weights[idx] = (self.weights[idx] * factor).clamp(1e-4, 1e4);
        }
        self.normalize();
        self.episodes += 1;
    }

    fn normalize(&mut self) {
        let sum: f64 = self.weights.iter().sum();
        if sum.is_finite() && sum > 0.0 {
            for w in self.weights.iter_mut() {
                *w /= sum;
            }
        }
    }

    /// Persiste a política em JSON (se configurada).
    pub fn persist(&self) {
        let Some(path) = &self.path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let weights: Vec<(String, Json)> = ExpertId::ALL
            .iter()
            .map(|id| {
                (
                    id.label().to_string(),
                    Json::num(self.weights[id.code() as usize]),
                )
            })
            .collect();
        let doc = Json::obj([
            ("version", Json::num(1.0)),
            ("episodes", Json::num(self.episodes as f64)),
            ("gamma", Json::num(self.gamma)),
            ("weights", Json::Obj(weights)),
        ]);
        let _ = std::fs::write(path, doc.to_text());
    }

    /// Carrega a política, se o arquivo existir.
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
        if let Some(weights) = json.get("weights") {
            for id in ExpertId::ALL {
                if let Some(value) = weights.get(id.label()).and_then(|v| v.as_f64()) {
                    if value.is_finite() && value > 0.0 {
                        self.weights[id.code() as usize] = value;
                    }
                }
            }
        }
        self.normalize();
    }

    /// Reseta para pesos uniformes.
    pub fn reset(&mut self) {
        self.weights = [1.0; SLOTS];
        self.normalize();
        self.episodes = 0;
    }
}

fn expert_of(slot: usize) -> ExpertId {
    match slot {
        0 => ExpertId::Transplant,
        1 => ExpertId::Phase,
        2 => ExpertId::Logic,
        3 => ExpertId::Solver,
        _ => ExpertId::Memory,
    }
}

impl ExpertId {
    /// Todos os especialistas (ordem canônica).
    pub const ALL: [ExpertId; SLOTS] = [
        ExpertId::Transplant,
        ExpertId::Phase,
        ExpertId::Logic,
        ExpertId::Solver,
        ExpertId::Memory,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pesos_comecam_uniformes_e_normalizados() {
        let router = MoERouter::new(7, None);
        let weights = router.weights();
        assert_eq!(weights.len(), SLOTS);
        let sum: f32 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn selecao_respeita_probes_e_used() {
        let mut router = MoERouter::new(7, None);
        let probes = [1.0, 0.0, 0.5, 0.0, 0.0];
        let picks = router.select(&probes, 2, &[]);
        assert_eq!(picks.len(), 2);
        assert!(picks.contains(&0) && picks.contains(&2));
        assert!(router
            .select(&probes, 2, &[ExpertId::Transplant, ExpertId::Logic])
            .is_empty());
    }

    #[test]
    fn reward_favorece_o_escolhido() {
        let mut router = MoERouter::new(11, None);
        let probes = [1.0, 1.0, 0.0, 0.0, 0.0];
        for _ in 0..40 {
            let picks = router.select(&probes, 1, &[]);
            let selected = picks.iter().map(|i| expert_of(*i)).collect::<Vec<_>>();
            let reward = if selected[0] == ExpertId::Transplant {
                1.0
            } else {
                0.0
            };
            router.update(&selected, reward);
        }
        assert!(router.weight_of(ExpertId::Transplant) > router.weight_of(ExpertId::Phase));
    }
}
