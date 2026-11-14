//! Módulo 5 — Memória associativa oscilatória com acoplamento complexo (ONN).
//!
//! Rede de osciladores com pesos complexos `C_ij = W_ij·e^{i·δ_ij}`: a
//! magnitude `W_ij` guarda a força da associação e o deslocamento `δ_ij`
//! guarda a **relação de fase arbitrária** entre osciladores — é isso que
//! permite gravar padrões contínuos, e não apenas binários `0/π`.
//!
//! Dinâmica de recuperação:
//!
//! ```text
//! dφ_i/dt = ω0 + (1/N) Σ_j W_ij · sin(φ_j − φ_i − δ_ij)
//!                  + α_clamp · m_i · sin(φ_i^cue − φ_i)
//! ```
//!
//! `m_i = 1` nos nós conhecidos (força de grampo) e `m_i = 0` nos ocultos:
//! os osciladores mascarados caem no atrator gravado e são reconstruídos.
//! Energia de fase minimizada implicitamente:
//!
//! ```text
//! E_phase(φ) = −(1/2N) Σ_ij W_ij · cos(φ_j − φ_i − δ_ij)
//! ```
//!
//! Aprendizado é hebbiano local (zero backprop):
//!
//! ```text
//! ΔW_ij     = η_W (cos(ψ_j − ψ_i − δ_ij) − γ_W W_ij)
//! Δδ_ij     = η_δ · W_ij · sin(ψ_j − ψ_i − δ_ij)
//! ```

use std::f32::consts::TAU;

/// Parâmetros de aprendizado e dinâmica da memória de fase.
#[derive(Clone, Copy, Debug)]
pub struct PhaseMemoryConfig {
    /// Frequência natural dos osciladores (rad/s). `0.0` = osciladores estáticos.
    pub omega0: f32,
    /// Força do grampo sensorial (`α_clamp`).
    pub clamp_strength: f32,
    /// Taxa de aprendizado da magnitude `W`.
    pub eta_w: f32,
    /// Taxa de aprendizado do deslocamento `δ`.
    pub eta_delta: f32,
    /// Decaimento metabólico dos pesos (Wenger espectral).
    pub gamma_w: f32,
}

impl Default for PhaseMemoryConfig {
    fn default() -> Self {
        Self {
            omega0: 0.0,
            clamp_strength: 6.0,
            eta_w: 0.08,
            eta_delta: 0.05,
            gamma_w: 0.004,
        }
    }
}

/// Memória associativa oscilatória de acoplamento complexo.
///
/// Toda a memória vive em três buffers planos pré-alocados: `weights` (módulo
/// de `C_ij`), `phase_shifts` (argumento) e `phases` (estado corrente). Nada
/// de vetores estáticos, arquivos ou embeddings: a memória **é** o campo de
/// acoplamento.
///
/// # Gravação de múltiplos atratores
///
/// A regra polar literal (`ΔW`, `Δδ` sobre um único ângulo) armazena **um**
/// atrator por matriz — o último imprint sobrescreve os anteriores. Para
/// cumprir o invariante de memória associativa (3 padrões coexistentes), o
/// `imprint` acumula o **fasor complexo** `C_ij += η_W·e^{i(ψ_j−ψ_i)}` com
/// decaimento metabólico `γ_W` e publica a representação polar
/// `(W_ij, δ_ij) = (|C_ij|, arg C_ij)`. A dinâmica de recuperação e a energia
/// de fase são exatamente as da especificação — o acoplamento passa a ser a
/// superposição dos padrões imprinted, e cada um deles é um mínimo de
/// `E_phase`.
pub struct ComplexPhaseMemory {
    /// Número de osciladores.
    pub n: usize,
    /// Magnitudes `W_ij` (layout plano, ordem C: índice `i*n + j`).
    pub weights: Vec<f32>,
    /// Deslocamentos de fase `δ_ij ∈ [0, 2π)` (mesmo layout).
    pub phase_shifts: Vec<f32>,
    /// Fases correntes `φ_i ∈ [0, 2π)`.
    pub phases: Vec<f32>,
    /// Acumulador complexo (parte real) — superposição dos imprints.
    c_re: Vec<f32>,
    /// Acumulador complexo (parte imaginária).
    c_im: Vec<f32>,
    cfg: PhaseMemoryConfig,
}

impl std::fmt::Debug for ComplexPhaseMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComplexPhaseMemory")
            .field("n", &self.n)
            .field("cfg", &self.cfg)
            .finish()
    }
}

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
    let mut v = x;
    while v > std::f32::consts::PI {
        v -= TAU;
    }
    while v < -std::f32::consts::PI {
        v += TAU;
    }
    v
}

impl ComplexPhaseMemory {
    /// Cria a memória com acoplamento nulo (superfície plana, pronta para o
    /// primeiro imprint).
    pub fn new(n: usize) -> Self {
        Self::with_config(n, PhaseMemoryConfig::default())
    }

    /// Cria a memória com configuração explícita.
    pub fn with_config(n: usize, cfg: PhaseMemoryConfig) -> Self {
        Self {
            n,
            weights: vec![0.0f32; n * n],
            phase_shifts: vec![0.0f32; n * n],
            phases: vec![0.0f32; n],
            c_re: vec![0.0f32; n * n],
            c_im: vec![0.0f32; n * n],
            cfg,
        }
    }

    /// Configuração corrente.
    pub fn config(&self) -> &PhaseMemoryConfig {
        &self.cfg
    }

    /// Energia de fase do estado atual (diagnóstico).
    pub fn phase_energy(&self) -> f32 {
        let n = self.n;
        let mut e = 0.0f32;
        for i in 0..n {
            for j in 0..n {
                let idx = i * n + j;
                let dphi = self.phases[j] - self.phases[i] - self.phase_shifts[idx];
                e -= self.weights[idx] * dphi.cos();
            }
        }
        e / (2.0 * n as f32)
    }

    /// Densidade de acoplamento ativo (pesos com módulo acima do limiar).
    pub fn density(&self, threshold: f32) -> f32 {
        let active = self.weights.iter().filter(|w| **w > threshold).count();
        active as f32 / self.weights.len() as f32
    }

    /// Grava um atrator contínuo na memória (regra hebbiana local).
    ///
    /// `pattern` tem `n` fases em `[0, 2π)`. Cada ciclo acumula o fasor
    /// associativo `C_ij += η_W·e^{i(ψ_j−ψ_i)}` com decaimento `γ_W` e
    /// republica a representação polar `(W_ij, δ_ij)`. Vários imprints
    /// coexistem como superposição de fase (ver o cabeçalho do módulo).
    pub fn imprint(&mut self, pattern: &[f32], cycles: usize) {
        let n = self.n;
        assert_eq!(pattern.len(), n, "padrão precisa ter n fases");
        let cycles = cycles.max(1);

        for _ in 0..cycles {
            for i in 0..n {
                for j in 0..n {
                    if i == j {
                        continue;
                    }
                    let idx = i * n + j;
                    let dpsi = wrap_signed(wrap(pattern[j]) - wrap(pattern[i]));
                    // ΔC = η_W · e^{i(ψ_j−ψ_i)} − γ_W · C  (regra hebbiana local)
                    self.c_re[idx] +=
                        self.cfg.eta_w * dpsi.cos() - self.cfg.gamma_w * self.c_re[idx];
                    self.c_im[idx] +=
                        self.cfg.eta_w * dpsi.sin() - self.cfg.gamma_w * self.c_im[idx];
                }
            }
        }

        // Publica a representação polar usada pela dinâmica e pela energia.
        for idx in 0..(n * n) {
            let re = self.c_re[idx];
            let im = self.c_im[idx];
            self.weights[idx] = (re * re + im * im).sqrt().min(1.0);
            self.phase_shifts[idx] = wrap(im.atan2(re));
        }
    }

    /// Recuperação associativa: reconstrói o padrão completo a partir de uma
    /// pista parcial.
    ///
    /// * `partial_cue`: fases da pista (os nós com `mask[i] == false` ignoram
    ///   o valor dado e partem do estado corrente).
    /// * `mask`: `true` = oscilador grampeado à pista, `false` = oculto.
    /// * retorna `(fases_reconstruídas, E_phase_final)`.
    pub fn recall(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> (Vec<f32>, f32) {
        let n = self.n;
        assert_eq!(partial_cue.len(), n);
        assert_eq!(mask.len(), n);

        // Estado inicial: os nós grampeados assumem a pista; os ocultos partem
        // do estado corrente (continuidade da memória).
        for i in 0..n {
            if mask[i] {
                self.phases[i] = wrap(partial_cue[i]);
            }
        }

        // Integração com passo semi-implícito (estável, sem backprop).
        let inv_n = 1.0 / n as f32;
        for _ in 0..steps {
            for i in 0..n {
                let mut drive = 0.0f32;
                let base = i * n;
                for j in 0..n {
                    if j == i {
                        continue;
                    }
                    let idx = base + j;
                    let dphi = self.phases[j] - self.phases[i] - self.phase_shifts[idx];
                    drive += self.weights[idx] * dphi.sin();
                }
                drive *= inv_n;

                if mask[i] {
                    // grampo sensorial suave (não duro: preserva a dinâmica)
                    let err = wrap_signed(wrap(partial_cue[i]) - self.phases[i]);
                    drive += self.cfg.clamp_strength * err.sin();
                }

                let dphase = self.cfg.omega0 + drive;
                self.phases[i] = wrap(self.phases[i] + dphase * dt);
            }
        }

        let energy = self.phase_energy();
        (self.phases.clone(), energy)
    }

    /// Evolução livre (sem grampo): útil para verificar que os atrators são
    /// estáveis e para o Módulo 8 (memória contínua).
    pub fn free_run(&mut self, steps: usize, dt: f32) -> f32 {
        let n = self.n;
        let inv_n = 1.0 / n as f32;
        for _ in 0..steps {
            for i in 0..n {
                let mut drive = 0.0f32;
                let base = i * n;
                for j in 0..n {
                    if j == i {
                        continue;
                    }
                    let idx = base + j;
                    let dphi = self.phases[j] - self.phases[i] - self.phase_shifts[idx];
                    drive += self.weights[idx] * dphi.sin();
                }
                self.phases[i] = wrap(self.phases[i] + (self.cfg.omega0 + drive * inv_n) * dt);
            }
        }
        self.phase_energy()
    }

    /// Poda metabólica: pesos abaixo do limiar são zerados fisicamente.
    pub fn prune(&mut self, threshold: f32) -> usize {
        let before = self.density(threshold);
        for w in self.weights.iter_mut() {
            if *w < threshold {
                *w = 0.0;
            }
        }
        let after = self.density(threshold);
        ((before - after) * self.weights.len() as f32).round() as usize
    }
}
