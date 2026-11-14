//! Módulo 5 — Memória associativa oscilatória com acoplamento complexo (ONN).
//!
//! Rede de osciladores com pesos complexos `C_ij = W_ij·e^{i·δ_ij}`: a
//! magnitude `W_ij` guarda a **coerência** da associação entre `i` e `j` e o
//! deslocamento `δ_ij` guarda a relação de fase — é isso que permite gravar
//! padrões contínuos, e não apenas binários `0/π`.
//!
//! Dinâmica de recuperação (exatamente a da especificação):
//!
//! ```text
//! dφ_i/dt = ω0 + (1/N) Σ_j W_ij · sin(φ_j − φ_i − δ_ij)
//!                  + α_clamp · m_i · sin(φ_i^cue + ω0·t − φ_i)
//! ```
//!
//! `m_i = 1` nos nós conhecidos (grampo sensorial) e `m_i = 0` nos ocultos: os
//! osciladores mascarados caem no atrator gravado e são reconstruídos.
//! Energia de fase minimizada implicitamente:
//!
//! ```text
//! E_phase(φ) = −(1/2N) Σ_ij W_ij · cos(φ_j − φ_i − δ_ij)
//! ```
//!
//! # Regra de aprendizado (honesta)
//!
//! A regra hebbiana polar da especificação é
//!
//! ```text
//! ΔW_ij = η_W (cos(ψ_j − ψ_i − δ_ij) − γ_W W_ij)
//! Δδ_ij = η_δ · W_ij · sin(ψ_j − ψ_i − δ_ij)
//! ```
//!
//! Ela grava **um** atrator por matriz: com uma única variável `δ_ij` não há
//! como representar várias relações de fase simultâneas. A generalização
//! correta para memória associativa é acumular o **fasor** complexo
//! `C_ij = Σ_μ e^{i(ψ_j^μ − ψ_i^μ)}` e usar a representação polar
//! `(W_ij, δ_ij) = (|C_ij|/A, arg C_ij)`: a superposição é a média circular
//! das relações de fase e `arg C` é exatamente o alvo que a regra `Δδ`
//! persegue (o máximo de `Σ_μ cos(ψ_j^μ − ψ_i^μ − δ)`).
//!
//! O que este módulo faz de fato:
//!
//! * **`ΔW`**: o acumulador complexo `C += η_W·e^{iΔψ} − γ_W·C` é a forma
//!   complexa da regra acima; sua decomposição polar é
//!   `ΔW = η_W cos(Δψ−δ) − γ_W W`, idêntica à especificação.
//! * **`Δδ`**: implementado literalmente como relaxação da fase para o alvo
//!   `arg C_ij`, `δ ← δ + η_δ·W_ij·sin(arg C_ij − δ)`, executada a cada ciclo
//!   de imprint. `η_δ` é, portanto, a **taxa de aprendizado de fase** (com
//!   `η_δ = 0` a memória não aprende relações de fase — há teste para isso).
//! * **Sem `clamp` de magnitude.** A versão anterior truncava
//!   `W_ij = min(|C_ij|, 1.0)`. Como `|C|` satura em `η_W/γ_W ≫ 1`, *todos*
//!   os pesos viravam 1.0 e a informação de coerência (a magnitude relativa,
//!   que diz em quais pares os padrões concordam) era destruída — pares com
//!   `|C| ≈ 0` recebiam acoplamento unitário baseado só no ruído de `arg C`.
//!   Agora `W_ij = |C_ij| / <|C|>` (gauge: acoplamento médio = 1), uma
//!   **constante global multiplicativa, não truncamento**: as razões entre
//!   pesos são exatamente preservadas. O gauge também desacopla a escala do
//!   campo de `η_W`, `γ_W` e do número de imprints, o que mantém a dinâmica
//!   estável para qualquer histórico de gravação (incluindo o daemon, que
//!   reimprime a cada ciclo e nunca chama `recall`).
//!
//! # Limite físico
//!
//! A capacidade de uma memória de fase com uma única `δ_ij` por par é finita:
//! os padrões interferem (a soma de fasores não é linear em `(W, δ)`) e a
//! matriz de coerência resultante só mantém `P ≲ 0.1·N` atratores estáveis.
//! Para `N = 32` isso significa `P = 3` no limite — medido em
//! `tests/phase2_m5_hardening.rs` (ver a tabela de erros no relatório).
//! Acima disso o `recall` converge para um mínimo espúrio da mistura, não para
//! o padrão original: é um limite do modelo, não um bug de implementação.
//!
//! # Robustez
//!
//! `imprint`/`recall` nunca panicam (tamanho errado, `NaN`, `dt ≤ 0`,
//! `steps = 0` e `n = 0` são no-op documentados); `try_imprint`/`try_recall`
//! devolvem `Err` explícito para o chamador que quiser tratar o caso.

use std::f32::consts::{PI, TAU};

/// Erros de validação da memória de fase.
///
/// As versões infalíveis (`imprint`, `recall`, `recall_into`) transformam
/// qualquer um destes casos em *no-op* seguro; use `try_imprint`/`try_recall`
/// para receber o diagnóstico.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseMemoryError {
    /// A rede não tem osciladores (`n = 0`).
    EmptyNetwork,
    /// `pattern.len()`, `mask.len()` ou `out.len()` diferente de `n`.
    InvalidSize,
    /// Entrada com `NaN` ou infinito.
    NonFiniteInput,
    /// `dt` não positivo ou não finito, ou `steps = 0`.
    InvalidIntegration,
    /// Parâmetros de aprendizado inválidos (`η < 0`, `γ ∉ [0, 1)`, não finitos).
    InvalidConfig,
}

impl std::fmt::Display for PhaseMemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyNetwork => write!(f, "rede vazia (n = 0)"),
            Self::InvalidSize => write!(f, "tamanho de entrada incompatível com n"),
            Self::NonFiniteInput => write!(f, "entrada contém NaN ou infinito"),
            Self::InvalidIntegration => write!(f, "passo de integração inválido"),
            Self::InvalidConfig => write!(f, "parâmetros de aprendizado inválidos"),
        }
    }
}

impl std::error::Error for PhaseMemoryError {}

/// Parâmetros de aprendizado e dinâmica da memória de fase.
#[derive(Clone, Copy, Debug)]
pub struct PhaseMemoryConfig {
    /// Frequência natural dos osciladores (rad/s). `0.0` = osciladores estáticos.
    ///
    /// Com `ω0 ≠ 0` o atrator é um **ciclo limite** que gira nessa taxa; o
    /// grampo acompanha a rotação (`ψ_i^cue + ω0·t`) e a fase acumulada fica
    /// disponível em [`ComplexPhaseMemory::rotation`].
    pub omega0: f32,
    /// Força do grampo sensorial (`α_clamp`).
    pub clamp_strength: f32,
    /// Taxa de aprendizado da magnitude `W`: incremento do fasor por ciclo
    /// (`ΔC = η_W·e^{iΔψ} − γ_W·C`). Como o campo é normalizado por um gauge
    /// global, `η_W` controla a velocidade de consolidação, não a paisagem de
    /// atratores (que depende só de `γ_W` e das relações entre os padrões).
    pub eta_w: f32,
    /// Taxa de aprendizado da fase `δ` (regra `Δδ = η_δ·W·sin(arg C − δ)`).
    /// `0.0` desliga o aprendizado de fase.
    pub eta_delta: f32,
    /// Decaimento metabólico dos pesos (Wenger espectral). Deve estar em
    /// `[0, 1)`: é o fator de esquecimento por ciclo.
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
/// Toda a memória vive em buffers planos pré-alocados: `weights` (módulo de
/// `C_ij`), `phase_shifts` (argumento) e `phases` (estado corrente). Nada de
/// vetores estáticos, arquivos ou embeddings: a memória **é** o campo de
/// acoplamento.
///
/// # Gravação de múltiplos atratores
///
/// `imprint` acumula o fasor complexo `C_ij += η_W·e^{i(ψ_j−ψ_i)} − γ_W·C_ij`
/// e publica `(W_ij, δ_ij) = (|C_ij|/A, arg C_ij)`, com `δ` relaxado pela regra
/// polar `Δδ` da especificação. A dinâmica de recuperação e a energia de fase
/// são exatamente as da especificação; o acoplamento é a superposição dos
/// padrões gravados.
pub struct ComplexPhaseMemory {
    /// Número de osciladores.
    pub n: usize,
    /// Magnitudes `W_ij` (layout plano, ordem C: índice `i*n + j`).
    pub weights: Vec<f32>,
    /// Deslocamentos de fase `δ_ij ∈ [0, 2π)` (mesmo layout).
    pub phase_shifts: Vec<f32>,
    /// Fases correntes `φ_i ∈ [0, 2π)`.
    pub phases: Vec<f32>,
    /// Rotação acumulada do referencial (`∫ ω0 dt`) durante `recall`/`free_run`.
    pub rotation: f32,
    /// Acumulador complexo (parte real) — superposição dos imprints.
    c_re: Vec<f32>,
    /// Acumulador complexo (parte imaginária).
    c_im: Vec<f32>,
    /// Módulo médio do acoplamento complexo (diagnóstico do gauge).
    mean_coupling: f32,
    cfg: PhaseMemoryConfig,
}

impl std::fmt::Debug for ComplexPhaseMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComplexPhaseMemory")
            .field("n", &self.n)
            .field("cfg", &self.cfg)
            .field("mean_coupling", &self.mean_coupling)
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
    let mut v = x % TAU;
    if v > PI {
        v -= TAU;
    } else if v < -PI {
        v += TAU;
    }
    v
}

/// Relaxações de fase executadas por ciclo de `imprint` para a regra `Δδ`.
///
/// A regra `δ ← δ + η_δ·W·sin(arg C − δ)` tem taxa proporcional a `η_δ·W`;
/// algumas sub-relaxações por ciclo fazem `δ` acompanhar a média circular sem
/// remover o papel de `η_δ` (com `η_δ = 0` nada é aprendido).
const DELTA_SUBSTEPS: usize = 4;

impl ComplexPhaseMemory {
    /// Cria a memória com acoplamento nulo (superfície plana, pronta para o
    /// primeiro imprint). `n = 0` é aceito: a memória fica inerte e
    /// `phase_energy`/`density` devolvem `0.0`.
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
            rotation: 0.0,
            c_re: vec![0.0f32; n * n],
            c_im: vec![0.0f32; n * n],
            mean_coupling: 0.0,
            cfg,
        }
    }

    /// Configuração corrente.
    pub fn config(&self) -> &PhaseMemoryConfig {
        &self.cfg
    }

    /// Magnitude média `⟨|C_ij|⟩` do último imprint (referência do gauge:
    /// `weights = |C| / mean_coupling`). `0.0` antes do primeiro imprint.
    pub fn mean_coupling(&self) -> f32 {
        self.mean_coupling
    }

    /// Reinicia o estado dinâmico: todas as fases em `phase` e rotação em zero.
    ///
    /// `recall` é *stateful* (os nós ocultos partem do estado corrente, por
    /// continuidade com o Módulo 8); chame isto para garantir que uma
    /// recuperação não dependa da anterior.
    pub fn reset_phases(&mut self, phase: f32) {
        self.phases
            .fill(if phase.is_finite() { wrap(phase) } else { 0.0 });
        self.rotation = 0.0;
    }

    /// Energia de fase do estado atual (diagnóstico).
    ///
    /// `n = 0` devolve `0.0` (nunca `NaN`).
    pub fn phase_energy(&self) -> f32 {
        let n = self.n;
        if n == 0 {
            return 0.0;
        }
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
    ///
    /// `n = 0` devolve `0.0` (nunca `NaN`).
    pub fn density(&self, threshold: f32) -> f32 {
        if self.weights.is_empty() {
            return 0.0;
        }
        let active = self.weights.iter().filter(|w| **w > threshold).count();
        active as f32 / self.weights.len() as f32
    }

    /// Grava um atrator contínuo na memória (regra hebbiana local).
    ///
    /// Versão infalível: entradas inválidas (tamanho errado, `NaN`, `n = 0`,
    /// `cycles = 0`, configuração inválida) são **no-op** e não envenenam o
    /// estado. Use [`Self::try_imprint`] para receber o diagnóstico.
    pub fn imprint(&mut self, pattern: &[f32], cycles: usize) {
        let _ = self.try_imprint(pattern, cycles);
    }

    /// Grava um atrator contínuo, devolvendo erro de validação explícito.
    ///
    /// `cycles` é o número de passadas hebbianas; `cycles = 0` é no-op (`Ok`,
    /// nada é gravado — não é promovido silenciosamente a 1).
    pub fn try_imprint(&mut self, pattern: &[f32], cycles: usize) -> Result<(), PhaseMemoryError> {
        let n = self.n;
        if n == 0 {
            return Err(PhaseMemoryError::EmptyNetwork);
        }
        if pattern.len() != n {
            return Err(PhaseMemoryError::InvalidSize);
        }
        if pattern.iter().any(|p| !p.is_finite()) {
            return Err(PhaseMemoryError::NonFiniteInput);
        }
        let eta = self.cfg.eta_w;
        let gamma = self.cfg.gamma_w;
        if !eta.is_finite() || eta < 0.0 || !gamma.is_finite() || !(0.0..1.0).contains(&gamma) {
            return Err(PhaseMemoryError::InvalidConfig);
        }
        if cycles == 0 {
            return Ok(());
        }

        let rate = if self.cfg.eta_delta.is_finite() {
            self.cfg.eta_delta.clamp(0.0, 1.0)
        } else {
            0.0
        };

        for _ in 0..cycles {
            for i in 0..n {
                let base = i * n;
                let psi_i = wrap(pattern[i]);
                for (j, psi_j) in pattern.iter().enumerate().take(n) {
                    if i == j {
                        continue;
                    }
                    let idx = base + j;
                    let dpsi = wrap_signed(wrap(*psi_j) - psi_i);
                    // ΔC = η_W · e^{i(ψ_j−ψ_i)} − γ_W · C  (regra hebbiana local).
                    // A parte real é exatamente ΔW = η_W(cos(Δψ−δ) − γ_W W).
                    self.c_re[idx] = self.c_re[idx] + eta * dpsi.cos() - gamma * self.c_re[idx];
                    self.c_im[idx] = self.c_im[idx] + eta * dpsi.sin() - gamma * self.c_im[idx];
                }
            }
        }

        // Gauge do acoplamento: a matriz só é definida a menos de um ganho
        // global (reescalar W e dt não muda as trajetórias de fase). Fixamos a
        // unidade no acoplamento **médio** = 1, de modo que a escala do campo
        // não dependa de η_W, γ_W nem do número de imprints e `weights` fique
        // na unidade da coerência (1 = par médio). Não é um clamp: todos os
        // pesos recebem exatamente o mesmo fator.
        let mut sum_mag = 0.0f64;
        let mut count = 0usize;
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let idx = i * n + j;
                let re = self.c_re[idx] as f64;
                let im = self.c_im[idx] as f64;
                sum_mag += (re * re + im * im).sqrt();
                count += 1;
            }
        }
        let mean_mag = if count > 0 {
            (sum_mag / count as f64) as f32
        } else {
            0.0
        };
        self.mean_coupling = mean_mag;
        let gain = if mean_mag > 0.0 && mean_mag.is_finite() {
            1.0 / mean_mag
        } else {
            0.0
        };

        // Publica (W, δ) e executa a regra Δδ da especificação:
        //   δ ← δ + η_δ · W · sin(arg C − δ)
        // O número de relaxações é proporcional a `cycles`: η_δ é a taxa de
        // aprendizado de fase por ciclo (com η_δ = 0 nada é aprendido).
        let relax_steps = cycles.saturating_mul(DELTA_SUBSTEPS).min(4096);
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let idx = i * n + j;
                let re = self.c_re[idx];
                let im = self.c_im[idx];
                let mag = (re * re + im * im).sqrt();
                let w = if mag.is_finite() { mag * gain } else { 0.0 };
                self.weights[idx] = if w.is_finite() { w } else { 0.0 };
                // |C| = 0 ⇒ arg indefinido: mantém o δ corrente (peso nulo, o
                // par não participa da dinâmica) em vez de inventar um ângulo.
                if mag > 0.0 && w > 0.0 && rate > 0.0 {
                    let target = im.atan2(re);
                    let mut d = self.phase_shifts[idx];
                    let step = rate * w;
                    for _ in 0..relax_steps {
                        let err = wrap_signed(target - d);
                        d = wrap(d + step * err.sin());
                    }
                    self.phase_shifts[idx] = d;
                }
            }
        }
        Ok(())
    }

    /// Recuperação associativa: reconstrói o padrão completo a partir de uma
    /// pista parcial.
    ///
    /// * `partial_cue`: fases da pista (os nós com `mask[i] == false` ignoram
    ///   o valor dado e partem do estado corrente).
    /// * `mask`: `true` = oscilador grampeado à pista, `false` = oculto.
    /// * retorna `(fases_reconstruídas, E_phase_final)`.
    ///
    /// Entradas inválidas (`NaN`, `dt ≤ 0`, `steps = 0`, tamanho errado,
    /// `n = 0`) são no-op: o estado não é alterado e a energia corrente é
    /// devolvida. Use [`Self::try_recall`] para o diagnóstico.
    pub fn recall(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> (Vec<f32>, f32) {
        let n = self.n;
        let mut out = vec![0.0f32; n];
        let energy = self.recall_into(partial_cue, mask, steps, dt, &mut out);
        (out, energy)
    }

    /// Versão de [`Self::recall`] que **não aloca** após o warmup: escreve em
    /// `out` (que deve ter `n` elementos) e devolve a energia final.
    ///
    /// Contrato: nenhuma alocação de heap, nenhum panic. Se `out.len() != n`,
    /// nada é escrito e a energia corrente é devolvida (`try_recall` reporta
    /// `InvalidSize`). As fases grampeadas seguem a pista no referencial
    /// girante (`cue_i + ω0·t`); a rotação acumulada fica em
    /// [`Self::rotation`].
    pub fn recall_into(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
        out: &mut [f32],
    ) -> f32 {
        if self.integrate(partial_cue, mask, steps, dt).is_err() {
            if out.len() == self.n {
                out.copy_from_slice(&self.phases);
            }
            return self.phase_energy();
        }
        out.copy_from_slice(&self.phases);
        self.phase_energy()
    }

    /// Igual a [`Self::recall`], mas com validação explícita.
    pub fn try_recall(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> Result<(Vec<f32>, f32), PhaseMemoryError> {
        self.integrate(partial_cue, mask, steps, dt)?;
        let mut out = vec![0.0f32; self.n];
        out.copy_from_slice(&self.phases);
        Ok((out, self.phase_energy()))
    }

    /// Núcleo da integração, com validação. Devolve `Err` sem tocar no estado.
    fn integrate(
        &mut self,
        partial_cue: &[f32],
        mask: &[bool],
        steps: usize,
        dt: f32,
    ) -> Result<(), PhaseMemoryError> {
        let n = self.n;
        if n == 0 {
            return Err(PhaseMemoryError::EmptyNetwork);
        }
        if partial_cue.len() != n || mask.len() != n {
            return Err(PhaseMemoryError::InvalidSize);
        }
        if steps == 0 || !dt.is_finite() || dt <= 0.0 {
            return Err(PhaseMemoryError::InvalidIntegration);
        }
        if !self.cfg.omega0.is_finite() || !self.cfg.clamp_strength.is_finite() {
            return Err(PhaseMemoryError::InvalidConfig);
        }
        if partial_cue.iter().any(|c| !c.is_finite()) {
            return Err(PhaseMemoryError::NonFiniteInput);
        }

        // Estado inicial: os nós grampeados assumem a pista; os ocultos partem
        // do estado corrente (continuidade da memória).
        for i in 0..n {
            if mask[i] {
                self.phases[i] = wrap(partial_cue[i]);
            }
        }

        let inv_n = 1.0 / n as f32;
        let omega0 = self.cfg.omega0;
        let mut rot = self.rotation;

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
                    // grampo sensorial suave; o alvo gira com o referencial
                    // (ω0·t), então ω0 ≠ 0 não quebra o atrator.
                    let target = wrap(partial_cue[i] + rot);
                    let err = wrap_signed(target - self.phases[i]);
                    drive += self.cfg.clamp_strength * err.sin();
                }

                let dphase = omega0 + drive;
                self.phases[i] = wrap(self.phases[i] + dphase * dt);
            }
            rot = wrap(rot + omega0 * dt);
        }

        self.rotation = rot;
        Ok(())
    }

    /// Evolução livre (sem grampo): útil para verificar que os atratores são
    /// estáveis e para o Módulo 8 (memória contínua).
    ///
    /// Entradas inválidas são no-op; devolve a energia corrente.
    pub fn free_run(&mut self, steps: usize, dt: f32) -> f32 {
        let n = self.n;
        if n == 0 {
            return 0.0;
        }
        let omega0 = self.cfg.omega0;
        if steps == 0 || !dt.is_finite() || dt <= 0.0 || !omega0.is_finite() {
            return self.phase_energy();
        }
        let inv_n = 1.0 / n as f32;
        let mut rot = self.rotation;
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
                self.phases[i] = wrap(self.phases[i] + (omega0 + drive * inv_n) * dt);
            }
            rot = wrap(rot + omega0 * dt);
        }
        self.rotation = rot;
        self.phase_energy()
    }

    /// Poda metabólica: pesos abaixo do limiar são zerados **fisicamente**
    /// (peso, `δ` e acumulador, para não ressuscitarem no próximo imprint).
    ///
    /// Devolve o número real de arestas apagadas (arestas que eram diferentes
    /// de zero — ou não finitas — e passaram a zero). `prune(f32::INFINITY)`
    /// apaga todas as arestas e devolve o número de arestas existentes.
    /// `threshold = NaN` é no-op (devolve `0`).
    pub fn prune(&mut self, threshold: f32) -> usize {
        if threshold.is_nan() {
            return 0;
        }
        let mut cut = 0usize;
        for idx in 0..self.weights.len() {
            let w = self.weights[idx];
            // Podado se está abaixo do limiar ou se não é finito (NaN/∞).
            let remove = w < threshold || !w.is_finite();
            if !remove {
                continue;
            }
            if w != 0.0 || !w.is_finite() {
                cut += 1;
            }
            self.weights[idx] = 0.0;
            self.phase_shifts[idx] = 0.0;
            self.c_re[idx] = 0.0;
            self.c_im[idx] = 0.0;
        }
        cut
    }
}
