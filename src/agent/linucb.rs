//! LinUCB contextual — opt-in (`AETHER_LINUCB`); EXP3/Beta permanece default.
//!
//! Dimensão pequena e inversa explícita: sem dependência nova e sem gradiente.
//! Braços = tools do registry; contexto = [`goal_features`].

use super::tool::{Goal, Risk};

/// Dimensão do vetor de contexto do goal (A6).
pub const LINUCB_DIM: usize = 8;

/// Opt-in: `AETHER_LINUCB=1|true|on|hitl`.
pub fn linucb_enabled() -> bool {
    match std::env::var("AETHER_LINUCB") {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "hitl" | "linucb"
        ),
        Err(_) => false,
    }
}

/// Features estáveis do goal para o bandit contextual.
pub fn goal_features(goal: &Goal) -> [f64; LINUCB_DIM] {
    let risk = match goal.risk_ceiling {
        Risk::Low => 0.0,
        Risk::Medium => 0.5,
        Risk::High => 1.0,
    };
    [
        f64::from(goal.kind.code()) / 4.0,
        if goal.path.is_some() { 1.0 } else { 0.0 },
        if goal.query.is_some() { 1.0 } else { 0.0 },
        if goal.relation.is_some() { 1.0 } else { 0.0 },
        if goal.subject.is_some() { 1.0 } else { 0.0 },
        risk,
        (goal.budget.max_steps as f64 / 16.0).clamp(0.0, 1.0),
        if goal.pipeline.is_empty() { 0.0 } else { 1.0 },
    ]
}

/// Braço linear com intervalo de confiança.
#[derive(Clone, Debug)]
pub struct LinUcb {
    d: usize,
    arms: usize,
    alpha: f64,
    /// `A` por braço, linha-maior `d×d`. Começa na identidade.
    a: Vec<Vec<f64>>,
    /// `b` por braço.
    b: Vec<Vec<f64>>,
}

impl LinUcb {
    /// `arms` braços em dimensão `d`. `alpha` é a largura do intervalo.
    pub fn new(arms: usize, d: usize, alpha: f64) -> Self {
        let eye = identity(d);
        Self {
            d,
            arms,
            alpha,
            a: vec![eye; arms],
            b: vec![vec![0.0; d]; arms],
        }
    }

    /// Número de braços.
    pub fn arms(&self) -> usize {
        self.arms
    }

    /// Dimensão do contexto.
    pub fn dim(&self) -> usize {
        self.d
    }

    /// Escolhe o braço de maior UCB para o contexto `x`.
    pub fn choose(&self, x: &[f64]) -> usize {
        let mut best = 0usize;
        let mut best_score = f64::MIN;
        for arm in 0..self.arms {
            let score = self.predict(arm, x) + self.bonus(arm, x);
            if score > best_score {
                best_score = score;
                best = arm;
            }
        }
        best
    }

    /// Atualiza o braço com a recompensa observada.
    pub fn update(&mut self, arm: usize, x: &[f64], reward: f64) {
        if arm >= self.arms || x.len() != self.d {
            return;
        }
        let a = &mut self.a[arm];
        let b = &mut self.b[arm];
        for i in 0..self.d {
            b[i] += reward * x[i];
            for j in 0..self.d {
                a[i * self.d + j] += x[i] * x[j];
            }
        }
    }

    /// Média prevista do braço (para ECE / score).
    pub fn predict(&self, arm: usize, x: &[f64]) -> f64 {
        if arm >= self.arms || x.len() != self.d {
            return 0.0;
        }
        let theta = solve(&self.a[arm], &self.b[arm]);
        dot(&theta, x).clamp(0.0, 1.0)
    }

    /// Bônus UCB `α √(xᵀ A⁻¹ x)`.
    pub fn bonus(&self, arm: usize, x: &[f64]) -> f64 {
        if arm >= self.arms || x.len() != self.d {
            return 0.0;
        }
        self.alpha * x_a_inv_x(&self.a[arm], x).sqrt()
    }
}

/// Bandit de tools: braço ↔ nome estático do registry.
#[derive(Clone, Debug)]
pub struct ContextualTools {
    bandit: LinUcb,
    arms: Vec<&'static str>,
}

impl ContextualTools {
    /// Nova política contextual para a lista de tools.
    pub fn new(tool_names: Vec<&'static str>, alpha: f64) -> Self {
        let n = tool_names.len().max(1);
        Self {
            bandit: LinUcb::new(n, LINUCB_DIM, alpha),
            arms: tool_names,
        }
    }

    /// Nomes dos braços.
    pub fn arms(&self) -> &[&'static str] {
        &self.arms
    }

    /// Índice do braço ou `None`.
    pub fn arm_of(&self, name: &str) -> Option<usize> {
        self.arms.iter().position(|n| *n == name)
    }

    /// Predição + bônus para o planner.
    pub fn score_parts(&self, name: &str, goal: &Goal) -> Option<(f64, f64)> {
        let arm = self.arm_of(name)?;
        let x = goal_features(goal);
        Some((self.bandit.predict(arm, &x), self.bandit.bonus(arm, &x)))
    }

    /// Update com reward verificado ∈ [0,1].
    pub fn update_verified(&mut self, name: &str, goal: &Goal, reward: f64) {
        let Some(arm) = self.arm_of(name) else {
            return;
        };
        let x = goal_features(goal);
        self.bandit.update(arm, &x, reward.clamp(0.0, 1.0));
    }

    /// Acesso ao bandit (gates / ECE).
    pub fn bandit(&self) -> &LinUcb {
        &self.bandit
    }

    /// Bandit mutável (replay).
    pub fn bandit_mut(&mut self) -> &mut LinUcb {
        &mut self.bandit
    }
}

fn identity(d: usize) -> Vec<f64> {
    let mut m = vec![0.0; d * d];
    for i in 0..d {
        m[i * d + i] = 1.0;
    }
    m
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn x_a_inv_x(a: &[f64], x: &[f64]) -> f64 {
    let inv = invert(a);
    let mut ax = vec![0.0; x.len()];
    let d = x.len();
    for i in 0..d {
        for j in 0..d {
            ax[i] += inv[i * d + j] * x[j];
        }
    }
    dot(x, &ax).max(0.0)
}

fn solve(a: &[f64], b: &[f64]) -> Vec<f64> {
    let inv = invert(a);
    let d = b.len();
    let mut out = vec![0.0; d];
    for i in 0..d {
        for j in 0..d {
            out[i] += inv[i * d + j] * b[j];
        }
    }
    out
}

/// Inversa por Gauss–Jordan. `d` é pequeno (≤8).
fn invert(a: &[f64]) -> Vec<f64> {
    let d = (a.len() as f64).sqrt() as usize;
    let mut m = vec![0.0; d * d * 2];
    for i in 0..d {
        for j in 0..d {
            m[i * d * 2 + j] = a[i * d + j];
        }
        m[i * d * 2 + d + i] = 1.0;
    }
    for col in 0..d {
        let mut pivot = col;
        let mut best = m[col * d * 2 + col].abs();
        for r in (col + 1)..d {
            let v = m[r * d * 2 + col].abs();
            if v > best {
                best = v;
                pivot = r;
            }
        }
        if pivot != col {
            for c in 0..(d * 2) {
                m.swap(col * d * 2 + c, pivot * d * 2 + c);
            }
        }
        let div = m[col * d * 2 + col];
        if div.abs() < 1e-12 {
            continue;
        }
        for c in 0..(d * 2) {
            m[col * d * 2 + c] /= div;
        }
        for r in 0..d {
            if r == col {
                continue;
            }
            let f = m[r * d * 2 + col];
            for c in 0..(d * 2) {
                let v = m[col * d * 2 + c];
                m[r * d * 2 + c] -= f * v;
            }
        }
    }
    let mut inv = vec![0.0; d * d];
    for i in 0..d {
        for j in 0..d {
            inv[i * d + j] = m[i * d * 2 + d + j];
        }
    }
    inv
}
