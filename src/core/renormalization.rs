//! Hierarquia de renormalização (3 níveis) com modulação top-down.
//!
//! Bloqueio complexo Micro → Meso → Macro: cada nível agrega blocos de 4
//! fasores (média complexa normalizada) e o nível Macro impõe um objetivo de
//! fase aos níveis inferiores. É o análogo discreto do Grupo de
//! Renormalização para campos de fase: a intenção macroscópica (Camada 2)
//! ancora a dinâmica micro, e a coerência entre níveis é medida diretamente.
//!
//! Referências: K. G. Wilson, *The renormalization group*, 1971; aspas em
//! "block spin" de Kadanoff (1966); modulação top-down em hierarquias
//! preditivas (Friston 2010).
//!
//! Sem alocação em `forward` após a construção.

#![allow(clippy::needless_range_loop)]

use crate::error::{AetherError, Result};
use std::f32::consts::{PI, TAU};

#[inline]
fn wrap(x: f32) -> f32 {
    let mut v = x % TAU;
    if v < 0.0 {
        v += TAU;
    }
    v
}

#[inline]
fn wrap_signed(x: f32) -> f32 {
    let mut v = x % TAU;
    if v > PI {
        v -= TAU;
    } else if v < -PI {
        v += TAU;
    }
    v
}

/// Hierarquia Micro/Meso/Macro de fasores.
#[derive(Clone, Debug)]
pub struct RenormalizationHierarchy {
    micro: usize,
    meso: usize,
    macro_dim: usize,
    gain: f32,
    goal: Option<f32>,
    micro_phase: Vec<f32>,
    micro_re: Vec<f32>,
    micro_im: Vec<f32>,
    meso_re: Vec<f32>,
    meso_im: Vec<f32>,
    macro_re: Vec<f32>,
    macro_im: Vec<f32>,
}

impl RenormalizationHierarchy {
    /// Cria a hierarquia com `micro` canais (múltiplo de 16).
    ///
    /// `meso = micro/4`, `macro = meso/4` (ex.: 64 → 16 → 4).
    pub fn new(micro: usize) -> Result<Self> {
        if micro == 0 || micro % 16 != 0 {
            return Err(AetherError::Config(
                "Renormalization: micro deve ser multiplo de 16".into(),
            ));
        }
        let meso = micro / 4;
        let macro_dim = meso / 4;
        Ok(Self {
            micro,
            meso,
            macro_dim,
            gain: 0.15,
            goal: None,
            micro_phase: vec![0.0f32; micro],
            micro_re: vec![0.0f32; micro],
            micro_im: vec![0.0f32; micro],
            meso_re: vec![0.0f32; meso],
            meso_im: vec![0.0f32; meso],
            macro_re: vec![0.0f32; macro_dim],
            macro_im: vec![0.0f32; macro_dim],
        })
    }

    /// Tamanhos `(micro, meso, macro)`.
    pub fn level_sizes(&self) -> (usize, usize, usize) {
        (self.micro, self.meso, self.macro_dim)
    }

    /// Fases micro da última passagem de [`forward`](Self::forward).
    ///
    /// É a leitura usada pelos gates do E1 que medem o desvio angular
    /// micro antes/depois do torque top-down. Não aloca.
    pub fn micro_phases(&self) -> &[f32] {
        &self.micro_phase
    }

    /// Fixa o objetivo persistente da camada Macro (fase em radianos).
    pub fn set_goal(&mut self, phase: f32) {
        self.goal = Some(wrap(phase));
    }

    /// Objetivo atual, se houver.
    pub fn goal(&self) -> Option<f32> {
        self.goal
    }

    /// Ganho da modulação top-down.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Ajusta o ganho top-down (0 = sem modulação).
    pub fn set_gain(&mut self, gain: f32) {
        if gain.is_finite() && gain >= 0.0 {
            self.gain = gain.min(1.0);
        }
    }

    /// Processa um campo de fases micro (comprimento `micro`).
    ///
    /// 1. modulação top-down em direção ao objetivo macro (se definido);
    /// 2. bloqueio complexo Micro → Meso → Macro.
    pub fn forward(&mut self, phases: &[f32]) -> Result<()> {
        if phases.len() != self.micro {
            return Err(AetherError::shape([self.micro], [phases.len()]));
        }
        if let Some(i) = phases.iter().position(|p| !p.is_finite()) {
            return Err(AetherError::Degenerate(format!(
                "Renormalization: phase[{i}] nao finita"
            )));
        }
        // Top-down: puxa o micro na direção do objetivo macro.
        if let Some(goal) = self.goal {
            for i in 0..self.micro {
                self.micro_phase[i] = wrap(phases[i] + self.gain * wrap_signed(goal - phases[i]));
            }
        } else {
            self.micro_phase.copy_from_slice(phases);
        }

        for i in 0..self.micro {
            let (s, c) = self.micro_phase[i].sin_cos();
            self.micro_re[i] = c;
            self.micro_im[i] = s;
        }

        // Meso: média complexa de blocos de 4.
        for j in 0..self.meso {
            let base = j * 4;
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for l in 0..4 {
                re += self.micro_re[base + l];
                im += self.micro_im[base + l];
            }
            let mag = (re * re + im * im).sqrt().max(1e-12);
            self.meso_re[j] = re / mag;
            self.meso_im[j] = im / mag;
        }

        // Macro: média complexa de blocos de 4 meso.
        for j in 0..self.macro_dim {
            let base = j * 4;
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for l in 0..4 {
                re += self.meso_re[base + l];
                im += self.meso_im[base + l];
            }
            let mag = (re * re + im * im).sqrt().max(1e-12);
            self.macro_re[j] = re / mag;
            self.macro_im[j] = im / mag;
        }
        Ok(())
    }

    /// Fase média (circular) do nível Macro.
    pub fn macro_phase(&self) -> f32 {
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for j in 0..self.macro_dim {
            re += self.macro_re[j];
            im += self.macro_im[j];
        }
        im.atan2(re)
    }

    /// Parâmetro de ordem do nível Macro `|⟨e^{iψ}⟩| ∈ [0,1]`.
    pub fn macro_order(&self) -> f32 {
        let mut re = 0.0f32;
        let mut im = 0.0f32;
        for j in 0..self.macro_dim {
            re += self.macro_re[j];
            im += self.macro_im[j];
        }
        (re * re + im * im).sqrt() / self.macro_dim.max(1) as f32
    }

    /// Alinhamento fase-intenção micro↔macro: `(1/N) Σ cos(φ_i − ψ_macro)`.
    pub fn alignment(&self) -> f32 {
        let psi = self.macro_phase();
        let mut acc = 0.0f32;
        for i in 0..self.micro {
            acc += (self.micro_phase[i] - psi).cos();
        }
        acc / self.micro as f32
    }

    /// Desvio angular entre o macro e o objetivo persistente.
    pub fn goal_drift(&self) -> Option<f32> {
        self.goal.map(|g| wrap_signed(self.macro_phase() - g).abs())
    }

    /// Torque top-down na **Camada 0**: soma em `omega_eff[i]` (rad/s)
    /// `k · [ sin(ψ_macro(i) − φ_i) + ½ sin(ψ_meso(i) − φ_i) ]`.
    ///
    /// `ψ_macro(i)` é o objetivo persistente (se houver) ou a fase do bloco
    /// Macro pai (16 micro por macro); `ψ_meso(i)` é a fase do bloco Meso pai
    /// (4 micro). Usa a fase micro de [`forward`](Self::forward); zero
    /// alocação. É o análogo discreto de um campo de intenção acoplando a
    /// frequência efetiva dos osciladores.
    pub fn torque_into(&self, omega_eff: &mut [f32], k: f32) -> Result<()> {
        if omega_eff.len() != self.micro {
            return Err(AetherError::shape([self.micro], [omega_eff.len()]));
        }
        if !k.is_finite() {
            return Err(AetherError::Degenerate(
                "Renormalization: k nao finito".into(),
            ));
        }
        let per_macro = self.micro / self.macro_dim.max(1);
        for i in 0..self.micro {
            let meso = i / 4;
            let mac = (i / per_macro).min(self.macro_dim - 1);
            let psi_macro = match self.goal {
                Some(g) => g,
                None => self.macro_im[mac].atan2(self.macro_re[mac]),
            };
            let psi_meso = self.meso_im[meso].atan2(self.meso_re[meso]);
            let phi = self.micro_phase[i];
            omega_eff[i] += k * ((psi_macro - phi).sin() + 0.5 * (psi_meso - phi).sin());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloqueio_preserva_coerencia_e_ancora_o_objetivo() {
        let mut rg = RenormalizationHierarchy::new(64).expect("rg");
        assert_eq!(rg.level_sizes(), (64, 16, 4));
        let mut field = vec![0.0f32; 64];
        for (i, v) in field.iter_mut().enumerate() {
            *v = 0.7 + 0.02 * i as f32;
        }
        rg.forward(&field).expect("forward");
        assert!(rg.macro_order() > 0.9, "ordem={}", rg.macro_order());
        assert!(rg.alignment() > 0.85, "align={}", rg.alignment());

        let goal = rg.macro_phase();
        rg.set_goal(goal);
        let mut rng = 0x1234_5678u32;
        let mut worst = 0.0f32;
        for _ in 0..2000 {
            for (i, v) in field.iter_mut().enumerate() {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let noise = ((rng >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.2;
                *v = 0.7 + 0.02 * i as f32 + noise;
            }
            rg.forward(&field).expect("forward");
            if let Some(d) = rg.goal_drift() {
                worst = worst.max(d);
            }
        }
        assert!(worst < 0.20, "drift macro={worst}");
    }

    #[test]
    fn torque_top_down_aponta_para_a_fase_macro() {
        let mut rg = RenormalizationHierarchy::new(64).expect("rg");
        rg.set_gain(0.0);
        rg.set_goal(1.0);
        let field = vec![0.2f32; 64]; // atrás do objetivo (1.0)
        rg.forward(&field).expect("forward");
        let mut omega = vec![0.0f32; 64];
        rg.torque_into(&mut omega, 2.0).expect("torque");
        assert!(omega.iter().all(|w| *w > 0.0), "torque deve acelerar");
        assert!(rg.torque_into(&mut omega[..3], 2.0).is_err());
    }
}
