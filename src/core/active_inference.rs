//! Inferência ativa, barreira de segurança de Lyapunov e filtro de atuador.
//!
//! Substitui os *proxies* da auditoria por mecanismos do motor:
//!
//! * [`ActiveInferenceEngine`] — seleção de política por **Energia Livre
//!   Esperada** `G = custo − λ_epistêmico · incerteza` e atualização
//!   bayesiana das crenças (Friston 2010; Parr, Pezzulo & Friston 2022).
//! * [`LyapunovCbf`] — projeção ortogonal de uma ação desejada sobre a
//!   meia-espacial segura `V(z) ≤ Vmax`, com `V(z)=½ z'Pz` e fallback de
//!   contração exata (Ames et al., *Control Barrier Functions*, 2019).
//! * [`EmaActuator`] — filtro exponencial do vetor de atuação (suavidade
//!   física, reduz chattering).
//! * [`classify_swarm`] — papéis ecológicos `Sentinel`/`Architect`/`Regulator`
//!   por assinatura dinâmica (ρ, plasticidade), separação funcional do enxame.
//!
//! Zero alocação nos caminhos `observe`, `select`, `project_into` e
//! `filter_into` após a construção.

#![allow(clippy::needless_range_loop)]

use crate::error::{AetherError, Result};

/// Filtro exponencial de atuação: `u_t = α u + (1−α) u_{t−1}`.
#[derive(Clone, Debug)]
pub struct EmaActuator {
    alpha: f32,
    state: Vec<f32>,
    primed: bool,
}

impl EmaActuator {
    /// Cria o filtro para `dim` atuadores; `alpha ∈ (0,1]`.
    pub fn new(dim: usize, alpha: f32) -> Result<Self> {
        if dim == 0 {
            return Err(AetherError::Config("EmaActuator: dim deve ser > 0".into()));
        }
        let alpha = if alpha.is_finite() && alpha > 0.0 && alpha <= 1.0 {
            alpha
        } else {
            0.3
        };
        Ok(Self {
            alpha,
            state: vec![0.0f32; dim],
            primed: false,
        })
    }

    /// Reinicia o estado do filtro.
    pub fn reset(&mut self) {
        self.state.fill(0.0);
        self.primed = false;
    }

    /// Filtra `u` em `out` (zero alocação).
    pub fn filter_into(&mut self, u: &[f32], out: &mut [f32]) -> Result<()> {
        if u.len() != self.state.len() || out.len() != self.state.len() {
            return Err(AetherError::shape([self.state.len()], [u.len()]));
        }
        if !self.primed {
            out.copy_from_slice(u);
            self.state.copy_from_slice(u);
            self.primed = true;
            return Ok(());
        }
        for i in 0..u.len() {
            let v = self.alpha * u[i] + (1.0 - self.alpha) * self.state[i];
            self.state[i] = v;
            out[i] = v;
        }
        Ok(())
    }

    /// Versão alocando a saída.
    pub fn filter(&mut self, u: &[f32]) -> Result<Vec<f32>> {
        let mut out = vec![0.0f32; self.state.len()];
        self.filter_into(u, &mut out)?;
        Ok(out)
    }

    /// Estado filtrado atual.
    pub fn state(&self) -> &[f32] {
        &self.state
    }
}

/// Barreira de segurança de Lyapunov `V(z) = ½ Σ p_i z_i² ≤ Vmax`.
#[derive(Clone, Debug)]
pub struct LyapunovCbf {
    dim: usize,
    pub v_max: f32,
    p: Vec<f32>,
    grad: Vec<f32>,
    cand: Vec<f32>,
    safe: Vec<f32>,
}

impl LyapunovCbf {
    /// Cria a barreira com pesos `p_i = 1` e teto `v_max > 0`.
    pub fn new(dim: usize, v_max: f32) -> Result<Self> {
        if dim == 0 || !v_max.is_finite() || v_max <= 0.0 {
            return Err(AetherError::Config(
                "LyapunovCbf: dim > 0 e v_max > 0".into(),
            ));
        }
        Ok(Self {
            dim,
            v_max,
            p: vec![1.0f32; dim],
            grad: vec![0.0f32; dim],
            cand: vec![0.0f32; dim],
            safe: vec![0.0f32; dim],
        })
    }

    /// `V(z)`.
    pub fn value(&self, z: &[f32]) -> f32 {
        let mut v = 0.0f32;
        for i in 0..self.dim.min(z.len()) {
            v += 0.5 * self.p[i] * z[i] * z[i];
        }
        v
    }

    /// Projeta `u` para que `V(z + u_safe) ≤ Vmax`; escreve em `out` e
    /// devolve `V(z + u_safe)`.
    ///
    /// Primeiro tenta a projeção ortogonal sobre a meia-espacial linearizada
    /// `∇h·u ≥ −γ h` com `h = Vmax − V(z)`; se a não-linearidade ainda violar
    /// o teto, contrai o passo exatamente até a fronteira.
    pub fn project_into(
        &mut self,
        z: &[f32],
        u: &[f32],
        gamma: f32,
        out: &mut [f32],
    ) -> Result<f32> {
        if z.len() != self.dim || u.len() != self.dim || out.len() != self.dim {
            return Err(AetherError::shape([self.dim], [z.len()]));
        }
        let gamma = if gamma.is_finite() {
            gamma.clamp(0.0, 1.0)
        } else {
            0.1
        };

        let v0 = self.value(z);
        for i in 0..self.dim {
            self.cand[i] = z[i] + u[i];
        }
        let v1 = self.value(&self.cand);
        if v1 <= self.v_max {
            out.copy_from_slice(&self.cand);
            return Ok(v1);
        }

        // h = Vmax − V(z); grad_h = −P z.
        let h = (self.v_max - v0).max(0.0);
        let mut grad_sq = 0.0f32;
        for i in 0..self.dim {
            self.grad[i] = -self.p[i] * z[i];
            grad_sq += self.grad[i] * self.grad[i];
        }
        let mut gh_u = 0.0f32;
        for i in 0..self.dim {
            gh_u += self.grad[i] * u[i];
        }
        let rhs = -gamma * h;
        let mut lambda = 0.0f32;
        if gh_u < rhs && grad_sq > 1e-12 {
            lambda = (rhs - gh_u) / grad_sq;
        }
        for i in 0..self.dim {
            self.cand[i] = z[i] + u[i] + lambda * self.grad[i];
        }

        // Contração radial exata até a fronteira `V(z + α d) = Vmax`.
        // V(z + αd) = A α² + B α + C, com C = V(z) − Vmax ≤ 0.
        let mut aa = 0.0f32;
        let mut bb = 0.0f32;
        for i in 0..self.dim {
            let d = self.cand[i] - z[i];
            aa += 0.5 * self.p[i] * d * d;
            bb += self.p[i] * z[i] * d;
        }
        let cc = v0 - self.v_max;
        let mut alpha = 1.0f32;
        if aa > 1e-12 {
            let disc = bb * bb - 4.0 * aa * cc;
            if disc >= 0.0 {
                let root = (-bb + disc.sqrt()) / (2.0 * aa);
                if root.is_finite() {
                    alpha = root.clamp(0.0, 1.0);
                }
            } else {
                alpha = 0.0;
            }
        } else if bb.abs() > 1e-12 {
            alpha = (-cc / bb).clamp(0.0, 1.0);
        }
        // Margem de 0.1% para absorver arredondamento de f32.
        alpha *= 0.999;
        for i in 0..self.dim {
            self.safe[i] = z[i] + alpha * (self.cand[i] - z[i]);
        }
        out.copy_from_slice(&self.safe);
        Ok(self.value(out))
    }

    /// Versão alocando a saída.
    pub fn project(&mut self, z: &[f32], u: &[f32], gamma: f32) -> Result<Vec<f32>> {
        let mut out = vec![0.0f32; self.dim];
        self.project_into(z, u, gamma, &mut out)?;
        Ok(out)
    }
}

/// Crença sobre uma zona/hipótese: média e variância gaussianas + custo.
#[derive(Clone, Copy, Debug)]
pub struct Zone {
    /// Média da crença.
    pub mean: f32,
    /// Variância da crença (incerteza epistêmica).
    pub var: f32,
    /// Custo esperado de agir nesta zona.
    pub cost: f32,
}

/// Motor de inferência ativa por Energia Livre Esperada.
#[derive(Clone, Debug)]
pub struct ActiveInferenceEngine {
    zones: Vec<Zone>,
    lambda_epistemic: f32,
    obs_var: f32,
    visits: Vec<u32>,
}

impl ActiveInferenceEngine {
    /// Cria o motor com as zonas e o peso epistêmico `λ ≥ 0`.
    pub fn new(zones: Vec<Zone>, lambda_epistemic: f32, obs_var: f32) -> Result<Self> {
        if zones.is_empty() {
            return Err(AetherError::Config(
                "ActiveInference: pelo menos uma zona".into(),
            ));
        }
        if !lambda_epistemic.is_finite() || lambda_epistemic < 0.0 {
            return Err(AetherError::Config(
                "ActiveInference: lambda_epistemic >= 0".into(),
            ));
        }
        let obs_var = if obs_var.is_finite() && obs_var > 0.0 {
            obs_var
        } else {
            0.05
        };
        let n = zones.len();
        Ok(Self {
            zones,
            lambda_epistemic,
            obs_var,
            visits: vec![0u32; n],
        })
    }

    /// `G(i) = custo_i − λ·var_i` (menor é melhor).
    pub fn expected_free_energy(&self, i: usize) -> f32 {
        let z = self.zones[i];
        z.cost - self.lambda_epistemic * z.var
    }

    /// Escolhe a zona de menor `G` (maior valor epistêmico).
    pub fn select(&self) -> usize {
        let mut best = 0usize;
        let mut best_g = self.expected_free_energy(0);
        for i in 1..self.zones.len() {
            let g = self.expected_free_energy(i);
            if g < best_g {
                best_g = g;
                best = i;
            }
        }
        best
    }

    /// Atualiza a crença da zona `i` com uma observação (Normal-Normal).
    pub fn observe(&mut self, i: usize, sample: f32) -> f32 {
        let z = &mut self.zones[i];
        let prior_prec = 1.0 / z.var.max(1e-9);
        let obs_prec = 1.0 / self.obs_var;
        let post_prec = prior_prec + obs_prec;
        z.mean = (z.mean * prior_prec + sample * obs_prec) / post_prec;
        z.var = 1.0 / post_prec;
        self.visits[i] = self.visits[i].saturating_add(1);
        z.var
    }

    /// Incerteza epistêmica da zona `i`.
    pub fn uncertainty(&self, i: usize) -> f32 {
        self.zones[i].var
    }

    /// Número de visitas à zona `i`.
    pub fn visits(&self, i: usize) -> u32 {
        self.visits[i]
    }

    /// Número de zonas.
    pub fn zone_count(&self) -> usize {
        self.zones.len()
    }
}

/// Papéis ecológicos do enxame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcologicalRole {
    /// Alta vigilância: maior raio espectral / excitabilidade.
    Sentinel,
    /// Alta plasticidade estrutural: crescimento/exploração.
    Architect,
    /// Homeostase: mantém ρ perto do alvo e poda o excesso.
    Regulator,
}

/// Assinatura dinâmica de um nó usada para atribuir papel.
#[derive(Clone, Copy, Debug)]
pub struct RoleMetrics {
    /// Raio espectral estimado.
    pub spectral_radius: f32,
    /// Plasticidade recente (ganho hebbiano ou sinapses novas).
    pub plasticity: f32,
    /// Esparsidade atual.
    pub sparsity: f32,
}

/// Classifica um enxame em papéis distintos por ranking dinâmico.
///
/// Os `⌈n/3⌉` nós com maior ρ viram `Sentinel`; entre os restantes, os
/// `⌈n/3⌉` com maior plasticidade viram `Architect`; o resto é `Regulator`.
pub fn classify_swarm(metrics: &[RoleMetrics]) -> Vec<EcologicalRole> {
    let n = metrics.len();
    let mut roles = vec![EcologicalRole::Regulator; n];
    if n == 0 {
        return roles;
    }
    let quota = n.div_ceil(3);
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        metrics[b]
            .spectral_radius
            .total_cmp(&metrics[a].spectral_radius)
            .then_with(|| metrics[b].plasticity.total_cmp(&metrics[a].plasticity))
    });
    for &i in idx.iter().take(quota) {
        roles[i] = EcologicalRole::Sentinel;
    }
    let mut rest: Vec<usize> = idx.iter().copied().skip(quota).collect();
    rest.sort_by(|&a, &b| {
        metrics[b]
            .plasticity
            .total_cmp(&metrics[a].plasticity)
            .then_with(|| metrics[a].sparsity.total_cmp(&metrics[b].sparsity))
    });
    for &i in rest.iter().take(quota) {
        roles[i] = EcologicalRole::Architect;
    }
    // Garante ao menos dois papéis quando há nós suficientes.
    if n >= 2 && roles.iter().all(|r| *r == roles[0]) {
        roles[n - 1] = if roles[0] == EcologicalRole::Sentinel {
            EcologicalRole::Regulator
        } else {
            EcologicalRole::Sentinel
        };
    }
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cbf_mantem_v_abaixo_do_teto_e_ema_suaviza() {
        let mut cbf = LyapunovCbf::new(8, 1.0).expect("cbf");
        let mut z = vec![0.48f32; 8]; // V = 0.5*8*0.48² = 0.92
        let mut u = vec![0.0f32; 8];
        let mut safe = vec![0.0f32; 8];
        for step in 0..2000 {
            for i in 0..8 {
                u[i] = if (step + i) % 7 == 0 { 0.6 } else { -0.2 };
            }
            let v = cbf.project_into(&z, &u, 0.1, &mut safe).expect("cbf");
            assert!(v <= 1.0 + 1e-6, "V={v} no passo {step}");
            z.copy_from_slice(&safe);
        }
        let mut ema = EmaActuator::new(4, 0.2).expect("ema");
        let mut out = vec![0.0f32; 4];
        ema.filter_into(&[1.0, -1.0, 1.0, -1.0], &mut out)
            .expect("ema");
        ema.filter_into(&[-1.0, 1.0, -1.0, 1.0], &mut out)
            .expect("ema");
        assert!(out[0].abs() < 1.0, "EMA nao suavizou: {out:?}");
    }

    #[test]
    fn inferencia_ativa_explora_e_reduz_incerteza() {
        let zones = vec![
            Zone {
                mean: 0.0,
                var: 0.01,
                cost: 0.0,
            },
            Zone {
                mean: 1.0,
                var: 1.0,
                cost: 0.05,
            },
        ];
        let mut engine = ActiveInferenceEngine::new(zones, 1.0, 0.05).expect("ai");
        let mut rng = 0x1234_5678u32;
        let mut first_half_b = 0;
        let mut last_half_b = 0;
        for step in 0..400 {
            let i = engine.select();
            if i == 1 {
                if step < 200 {
                    first_half_b += 1;
                } else {
                    last_half_b += 1;
                }
            }
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = ((rng >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.4;
            let sample = if i == 1 { 1.0 + noise } else { noise };
            engine.observe(i, sample);
        }
        assert!(engine.uncertainty(1) < 0.20, "U={}", engine.uncertainty(1));
        assert!(
            last_half_b <= first_half_b,
            "exploracao nao reduziu: {first_half_b} -> {last_half_b}"
        );
    }

    #[test]
    fn papeis_emergem_distintos() {
        let metrics = vec![
            RoleMetrics {
                spectral_radius: 0.95,
                plasticity: 0.1,
                sparsity: 0.2,
            },
            RoleMetrics {
                spectral_radius: 0.80,
                plasticity: 0.9,
                sparsity: 0.1,
            },
            RoleMetrics {
                spectral_radius: 0.20,
                plasticity: 0.2,
                sparsity: 0.8,
            },
            RoleMetrics {
                spectral_radius: 0.15,
                plasticity: 0.8,
                sparsity: 0.7,
            },
        ];
        let roles = classify_swarm(&metrics);
        let mut distinct: Vec<EcologicalRole> = Vec::new();
        for r in roles {
            if !distinct.contains(&r) {
                distinct.push(r);
            }
        }
        assert!(distinct.len() >= 2, "papeis={distinct:?}");
    }
}
