//! Módulo 7 — Plasticidade topológica por Energia Livre Variacional.
//!
//! A matriz de acoplamento `J_ij` **não é estática**: o circuito cultiva novas
//! conexões onde há novidade física (surpresa de reconstrução alta) e dissolve
//! sinapses ociosas por custo metabólico proporcional à densidade.
//!
//! # Equações efetivas (M7-HARDEN)
//!
//! ```text
//! S(t)   = max(prediction_error, 0)                 // surpresa de reconstrução (MSE)
//! D(t)   = #{ (i,j) : i≠j, |J_ij| > θ } / (n(n−1))  // densidade ativa
//! F(t)   = S(t) + λ_metab · Σ_{i≠j} |J_ij|          // energia livre REPORTADA
//! g(S)   = κ_grow · tanh(S)                         // ganho hebbiano (só surpresa)
//! γ(D)   = κ_decay · (1 + κ_density · D)            // custo metabólico ∝ densidade
//!
//! dJ_ij/dt = g(S) · (s_i s_j − J_ij) − γ(D) · J_ij
//! |J_ij| < θ_dissolve  ⇒  J_ij = 0                  // poda sináptica física
//! ```
//!
//! Desvio deliberado da especificação original (`tanh(F)`): a energia livre
//! contém o termo metabólico `λ·Σ|J|`, que é estritamente positivo mesmo com
//! `S = 0`. Usar `tanh(F)` como ganho fazia o crescimento hebbiano continuar no
//! silêncio (o "silêncio" alimentava a densidade). Aqui o ganho depende **apenas
//! da surpresa de reconstrução** `S`; o custo metabólico entra exclusivamente no
//! decaimento, e cresce com a densidade `D` — topologias densas decaem mais
//! rápido, topologias esparsas preservam orçamento metabólico.
//!
//! Consequências documentadas:
//! * `S = 0` ⇒ `g = 0` ⇒ `dJ/dt = −γ(D)·J`: toda sinapse com `|J| > θ` decai e
//!   é zerada (esparsidade sobe monotonicamente). O estado `J = 0` deixa de ser
//!   um poço absorvente para `S > 0`, pois `g(S)·s_i s_j` independe de `J`.
//! * Existe um limiar de nucleação: partindo de `J = 0`, uma sinapse só
//!   sobrevive ao primeiro passo se `g(S)·|s_i s_j|·dt > θ`. Para os padrões
//!   default (`κ_grow = 0.2`, `θ = 0.005`, `dt = 0.05`) isso ocorre em
//!   `S ≳ 0.55` com `|s_i s_j| = 1`. Abaixo do limiar não há como cristalizar
//!   conexão — é física, não bug (sem correlação suficiente, sem sinapse).
//!
//! # Homeostase espectral (ρ real)
//!
//! A cada `homeostasis_period` passos (default 2, limitado a `1..=5`) o raio
//! espectral **real** é estimado por iteração de potência determinística
//! (`O(N²)` por iteração, buffers pré-alocados — zero alocação no loop) e `J` é
//! escalado por `alvo/ρ` quando `ρ > alvo`. O limite de Gershgorin permanece
//! como teto de segurança (`gershgorin_cap · alvo`): se o escalonamento por ρ
//! ainda deixar o limite acima do teto, um escalonamento extra é aplicado.
//! Se `ρ` não for finito, nada é normalizado e `Err` é retornado.
//!
//! # Robustez
//!
//! [`MorphogeneticField::try_evolve_topology`] valida `dt > 0` finito, surpresa
//! finita, contagem/magnitude de spins e dimensão; em erro devolve
//! [`MorphogenesisError`] **sem panic**. [`MorphogeneticField::evolve_topology`]
//! é o wrapper legado: em erro é no-op e retorna `0.0`.
//!
//! **Métrica `sparsity_ratio`:** historicamente documentada como "fração de
//! conexões ativas", mas a especificação exige que ela **suba no silêncio**.
//! Aqui ela mede a fração de sinapses *dissolvidas* (`J_ij == 0`, fora da
//! diagonal) — isto é, esparsidade de verdade. Para densidade use
//! [`MorphogeneticField::density`]. O daemon exibe `sparsity_ratio` como
//! "topologia %"; o rótulo continua neutro, mas o valor agora é esparsidade.

/// Iterações de potência por normalização (convergência + teto determinístico).
const POWER_ITERATIONS: usize = 64;
/// Tolerância relativa de convergência da iteração de potência.
const POWER_TOL: f32 = 1e-5;
/// Magnitude máxima aceita para spins (garante `s_i·s_j` finito em f32).
const SPIN_MAGNITUDE_LIMIT: f32 = 1.0e6;
/// Período máximo de homeostase aceito (física > configurabilidade).
const MAX_HOMEOSTASIS_PERIOD: usize = 5;

/// Parâmetros morfogenéticos.
#[derive(Clone, Copy, Debug)]
pub struct MorphogenesisConfig {
    /// Ganho de crescimento hebbiano `κ_grow`.
    pub k_grow: f32,
    /// Decaimento metabólico basal `κ_decay`.
    pub k_decay: f32,
    /// Penalidade metabólica por densidade `κ_density` (γ = κ_decay·(1+κ_density·D)).
    pub k_density: f32,
    /// Limiar de dissolução `θ_dissolve`.
    pub dissolve: f32,
    /// Peso metabólico da energia livre reportada `λ_metabolic`.
    pub lambda_metabolic: f32,
    /// Raio espectral alvo (homeostase).
    pub target_spectral_radius: f32,
    /// Ciclos entre normalizações espectrais (saturado em `1..=5`).
    pub homeostasis_period: usize,
    /// Passo de integração máximo aceito (Euler explícito estável).
    pub max_dt: f32,
    /// Teto de Gershgorin como múltiplo do alvo espectral.
    pub gershgorin_cap: f32,
    /// Densidade alvo de sinapses ativas (plasticidade estrutural homeostática).
    ///
    /// `0.0` desliga (comportamento clássico: a topologia só decai). Com
    /// `0 < d < 1` a cada passo as sinapses mais fracas são podadas até a
    /// densidade `d`, preservando as mais fortes — poda por quota análoga à
    /// plasticidade estrutural homeostática de Gallinaro & Rotter
    /// (arXiv:1706.02912) e Millán et al. (arXiv:1705.02773). Impede o
    /// colapso ρ→0 sob silêncio prolongado.
    pub homeostatic_density: f32,
}

impl Default for MorphogenesisConfig {
    fn default() -> Self {
        Self {
            k_grow: 0.2,
            k_decay: 1.2,
            k_density: 1.0,
            dissolve: 0.005,
            lambda_metabolic: 0.002,
            target_spectral_radius: 1.0,
            homeostasis_period: 2,
            max_dt: 0.5,
            gershgorin_cap: 4.0,
            homeostatic_density: 0.0,
        }
    }
}

impl MorphogenesisConfig {
    /// Substitui valores não finitos/patológicos pelos defaults físicos.
    fn sanitized(mut self) -> Self {
        let d = Self::default();
        if !self.k_grow.is_finite() || self.k_grow < 0.0 {
            self.k_grow = d.k_grow;
        }
        if !self.k_decay.is_finite() || self.k_decay < 0.0 {
            self.k_decay = d.k_decay;
        }
        if !self.k_density.is_finite() || self.k_density < 0.0 {
            self.k_density = d.k_density;
        }
        if !self.dissolve.is_finite() || self.dissolve < 0.0 {
            self.dissolve = d.dissolve;
        }
        if !self.lambda_metabolic.is_finite() || self.lambda_metabolic < 0.0 {
            self.lambda_metabolic = d.lambda_metabolic;
        }
        if !self.target_spectral_radius.is_finite() || self.target_spectral_radius <= 0.0 {
            self.target_spectral_radius = d.target_spectral_radius;
        }
        self.homeostasis_period = self.homeostasis_period.clamp(1, MAX_HOMEOSTASIS_PERIOD);
        if !self.max_dt.is_finite() || self.max_dt <= 0.0 {
            self.max_dt = d.max_dt;
        }
        if !self.gershgorin_cap.is_finite() || self.gershgorin_cap < 1.0 {
            self.gershgorin_cap = d.gershgorin_cap;
        }
        if !self.homeostatic_density.is_finite() || self.homeostatic_density < 0.0 {
            self.homeostatic_density = 0.0;
        } else {
            self.homeostatic_density = self.homeostatic_density.min(1.0);
        }
        self
    }
}

/// Erros de validação da morfogênese.
///
/// Todos os caminhos de erro são pré-validados: `J` não é mutado parcialmente.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MorphogenesisError {
    /// Campo de dimensão zero não possui dinâmica.
    EmptyField,
    /// `dt` não finito ou `<= 0`.
    InvalidDt { dt: f32 },
    /// Surpresa de reconstrução não finita.
    InvalidPredictionError { value: f32 },
    /// Vetor de spins com tamanho diferente de `n`.
    SpinCountMismatch { expected: usize, got: usize },
    /// Spin não finito.
    InvalidSpin { index: usize, value: f32 },
    /// Spin com magnitude absurda (produto `s_i·s_j` transbordaria f32).
    SpinMagnitudeTooLarge { max_abs: f32 },
    /// Matriz inicial com tamanho diferente de `n·n`.
    InvalidInitialMatrix { expected: usize, got: usize },
    /// Matriz inicial com entrada não finita.
    NonFiniteInitialMatrix { index: usize },
    /// Raio espectral não finito: homeostase recusa normalizar.
    NonFiniteSpectralRadius,
    /// Atualização produziu valor não finito (não deveria ocorrer após validação).
    NonFiniteUpdate { index: usize },
}

impl std::fmt::Display for MorphogenesisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyField => write!(f, "campo morfogenético com dimensão zero"),
            Self::InvalidDt { dt } => write!(f, "dt inválido: {dt} (exige finito e > 0)"),
            Self::InvalidPredictionError { value } => {
                write!(f, "prediction_error inválido: {value} (exige finito)")
            }
            Self::SpinCountMismatch { expected, got } => {
                write!(f, "active_spins com {got} entradas; esperado {expected}")
            }
            Self::InvalidSpin { index, value } => {
                write!(f, "spin[{index}] inválido: {value}")
            }
            Self::SpinMagnitudeTooLarge { max_abs } => {
                write!(f, "spin com magnitude {max_abs} acima do limite")
            }
            Self::InvalidInitialMatrix { expected, got } => {
                write!(f, "J0 com {got} entradas; esperado {expected}")
            }
            Self::NonFiniteInitialMatrix { index } => {
                write!(f, "J0[{index}] não finito")
            }
            Self::NonFiniteSpectralRadius => {
                write!(f, "raio espectral não finito; normalização recusada")
            }
            Self::NonFiniteUpdate { index } => {
                write!(f, "atualização de J[{index}] não finita")
            }
        }
    }
}

impl std::error::Error for MorphogenesisError {}

/// Campo morfogenético: dono e evoluidor da topologia `J`.
///
/// `j` permanece público por compatibilidade com testes/código existentes; a
/// cada passo ele é re-simetrizado, tem a diagonal zerada, entradas não finitas
/// removidas e sinapses abaixo de `dissolve` podadas — mutações externas não
/// sobrevivem a um passo. Prefira [`Self::coupling`] para leitura.
pub struct MorphogeneticField {
    n: usize,
    cfg: MorphogenesisConfig,
    /// Matriz plana `N×N` (ordem C, simétrica, diagonal zero).
    pub j: Vec<f32>,
    cycles: usize,
    /// Última energia livre reportada (calculada uma vez por passo).
    free_energy: f32,
    /// Última surpresa de reconstrução (MSE, `>= 0`).
    surprise: f32,
    /// Buffer da iteração de potência (vetor).
    power_v: Vec<f32>,
    /// Buffer da iteração de potência (J·v).
    power_av: Vec<f32>,
    /// Buffer da poda homeostática por quota `(|w|, i, k)`.
    prune_buf: Vec<(f32, u32, u32)>,
}

impl std::fmt::Debug for MorphogeneticField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MorphogeneticField")
            .field("n", &self.n)
            .field("cfg", &self.cfg)
            .field("free_energy", &self.free_energy)
            .field("surprise", &self.surprise)
            .field("cycles", &self.cycles)
            .finish()
    }
}

impl MorphogeneticField {
    /// Cria o campo a partir de uma matriz plana (cópia, simetrizada).
    ///
    /// Tolerante por compatibilidade: se `j0.len() != n·n`, o campo nasce
    /// zerado; entradas não finitas viram zero e `|J_ij| < dissolve` é podado.
    /// Para validação estrita use [`Self::try_new`].
    pub fn new(n: usize, j0: &[f32], cfg: MorphogenesisConfig) -> Self {
        let cfg = cfg.sanitized();
        let len = n.saturating_mul(n);
        let mut j = vec![0.0f32; len];
        if j0.len() == len {
            j.copy_from_slice(j0);
        }
        let mut field = Self {
            n,
            cfg,
            j,
            cycles: 0,
            free_energy: 0.0,
            surprise: 0.0,
            power_v: vec![0.0f32; n],
            power_av: vec![0.0f32; n],
            prune_buf: Vec::new(),
        };
        field.sanitize();
        field
    }

    /// Cria o campo exigindo `j0` finito de tamanho exato `n·n`.
    pub fn try_new(
        n: usize,
        j0: &[f32],
        cfg: MorphogenesisConfig,
    ) -> Result<Self, MorphogenesisError> {
        let expected = n.saturating_mul(n);
        if j0.len() != expected {
            return Err(MorphogenesisError::InvalidInitialMatrix {
                expected,
                got: j0.len(),
            });
        }
        if let Some(index) = j0.iter().position(|v| !v.is_finite()) {
            return Err(MorphogenesisError::NonFiniteInitialMatrix { index });
        }
        Ok(Self::new(n, j0, cfg))
    }

    /// Dimensão `N`.
    pub fn dim(&self) -> usize {
        self.n
    }

    /// Ciclos completos já executados.
    pub fn cycles(&self) -> usize {
        self.cycles
    }

    /// Última energia livre reportada `F = S + λ·Σ|J|`.
    pub fn last_free_energy(&self) -> f32 {
        self.free_energy
    }

    /// Última surpresa de reconstrução `S` (MSE, `>= 0`).
    pub fn last_surprise(&self) -> f32 {
        self.surprise
    }

    /// Fração de sinapses **ativas** (`|J_ij| > θ_dissolve`, fora da diagonal).
    ///
    /// Este é o complemento de [`Self::sparsity_ratio`]; use este método quando
    /// o consumidor quer densidade (ex.: telemetria de ocupação).
    pub fn density(&self) -> f32 {
        1.0 - self.sparsity_ratio()
    }

    /// Fração de sinapses **dissolvidas** (`J_ij == 0`, fora da diagonal).
    ///
    /// Nota de compatibilidade: o nome histórico dizia "conexões ativas", mas a
    /// especificação exige que o valor **suba no silêncio** (poda). Portanto
    /// aqui `sparsity_ratio` é esparsidade real: `0` = grafo completo,
    /// `1` = grafo vazio. Para densidade, ver [`Self::density`].
    pub fn sparsity_ratio(&self) -> f32 {
        let n = self.n;
        let total = n.saturating_mul(n.saturating_sub(1));
        if total == 0 {
            return 0.0;
        }
        let mut dissolved = 0usize;
        for i in 0..n {
            for k in 0..n {
                if i != k && self.j[i * n + k] == 0.0 {
                    dissolved += 1;
                }
            }
        }
        dissolved as f32 / total as f32
    }

    /// Número de sinapses exatamente nulas fora da diagonal.
    pub fn zero_synapses(&self) -> usize {
        let n = self.n;
        let mut zeros = 0usize;
        for i in 0..n {
            for k in 0..n {
                if i != k && self.j[i * n + k] == 0.0 {
                    zeros += 1;
                }
            }
        }
        zeros
    }

    /// Número de sinapses ativas (`|J_ij| > θ_dissolve`, fora da diagonal).
    pub fn active_synapses(&self) -> usize {
        let n = self.n;
        let mut active = 0usize;
        for i in 0..n {
            for k in 0..n {
                if i != k && self.j[i * n + k].abs() > self.cfg.dissolve {
                    active += 1;
                }
            }
        }
        active
    }

    /// Energia livre variacional `F = S + λ·Σ_{i≠j}|J_ij|`.
    ///
    /// `prediction_error` é o MSE médio `(1/L) Σ (u − v̂)²`; o termo metabólico
    /// é `λ · Σ|J|`. Esta energia é **diagnóstico/telemetria** — ela não entra
    /// no ganho de plasticidade (ver documentação do módulo).
    pub fn free_energy(&self, prediction_error: f32) -> f32 {
        let mut metabolic = 0.0f32;
        let n = self.n;
        for i in 0..n {
            for k in 0..n {
                if i != k {
                    metabolic += self.j[i * n + k].abs();
                }
            }
        }
        prediction_error.max(0.0) + self.cfg.lambda_metabolic * metabolic
    }

    /// Um passo morfogenético, tolerante a entradas inválidas.
    ///
    /// Em erro é **no-op** e retorna `0.0` (indistinguível de uma energia livre
    /// legitimamente nula). Para distinguir erro, use
    /// [`Self::try_evolve_topology`].
    pub fn evolve_topology(&mut self, prediction_error: f32, active_spins: &[f32], dt: f32) -> f32 {
        self.try_evolve_topology(prediction_error, active_spins, dt)
            .unwrap_or(0.0)
    }

    /// Um passo morfogenético com validação estrita.
    ///
    /// * `prediction_error`: MSE da predição (surpresa de reconstrução); valores
    ///   negativos são saturados em `0` (não há surpresa negativa);
    /// * `active_spins`: estado motor/emergente atual (`len == n`); o produto
    ///   `s_i·s_j` modula onde a conexão cresce;
    /// * `dt`: passo de integração; saturado em `cfg.max_dt` (Euler explícito).
    ///
    /// Retorna a energia livre `F(t)` após o passo (e após a homeostase, se
    /// ocorrer neste ciclo).
    pub fn try_evolve_topology(
        &mut self,
        prediction_error: f32,
        active_spins: &[f32],
        dt: f32,
    ) -> Result<f32, MorphogenesisError> {
        let n = self.n;
        if n == 0 {
            return Err(MorphogenesisError::EmptyField);
        }
        if !dt.is_finite() || dt <= 0.0 {
            return Err(MorphogenesisError::InvalidDt { dt });
        }
        if !prediction_error.is_finite() {
            return Err(MorphogenesisError::InvalidPredictionError {
                value: prediction_error,
            });
        }
        if active_spins.len() != n {
            return Err(MorphogenesisError::SpinCountMismatch {
                expected: n,
                got: active_spins.len(),
            });
        }
        let mut max_abs = 0.0f32;
        for (index, spin) in active_spins.iter().enumerate() {
            if !spin.is_finite() {
                return Err(MorphogenesisError::InvalidSpin {
                    index,
                    value: *spin,
                });
            }
            let abs = spin.abs();
            if abs > max_abs {
                max_abs = abs;
            }
        }
        if max_abs > SPIN_MAGNITUDE_LIMIT {
            return Err(MorphogenesisError::SpinMagnitudeTooLarge { max_abs });
        }

        // Defesa contra mutação externa de `j` (campo público por compatibilidade).
        self.sanitize();

        let surprise = prediction_error.max(0.0);
        let step = dt.min(self.cfg.max_dt);
        let density = self.density();
        let gain = self.cfg.k_grow * surprise.tanh();
        let decay_rate = self.cfg.k_decay * (1.0 + self.cfg.k_density * density);
        let dissolve = self.cfg.dissolve;

        for i in 0..n {
            for k in (i + 1)..n {
                let idx_ik = i * n + k;
                let idx_ki = k * n + i;
                let jv = self.j[idx_ik];
                let corr = active_spins[i] * active_spins[k];
                let dj = gain * (corr - jv) - decay_rate * jv;
                let mut new_val = jv + dj * step;
                if !new_val.is_finite() {
                    return Err(MorphogenesisError::NonFiniteUpdate { index: idx_ik });
                }
                // Consolidação homeostática: uma sinapse estabelecida não
                // atravessa o limiar de dissolução por decaimento. A remoção
                // só acontece pela poda por quota (abaixo), que preserva as
                // sinapses mais fortes — plasticidade estrutural homeostática.
                if self.cfg.homeostatic_density > 0.0 && jv != 0.0 && new_val != 0.0 {
                    let floor = 2.0 * dissolve;
                    if new_val.abs() < floor {
                        new_val = floor.copysign(new_val);
                    }
                }
                if new_val.abs() < dissolve {
                    new_val = 0.0;
                }
                self.j[idx_ik] = new_val;
                self.j[idx_ki] = new_val;
            }
            self.j[i * n + i] = 0.0;
        }

        self.cycles += 1;
        self.surprise = surprise;
        if self.cycles % self.cfg.homeostasis_period == 0 {
            self.try_homeostasis()?;
        }
        if self.cfg.homeostatic_density > 0.0 {
            self.enforce_homeostatic_density();
        }

        // Energia livre calculada UMA vez por passo (após a normalização).
        self.free_energy = self.free_energy(surprise);
        Ok(self.free_energy)
    }

    /// Poda homeostática por quota.
    ///
    /// Mantém no máximo `homeostatic_density · N(N−1)/2` sinapses ativas,
    /// removendo as de menor `|J_ij|` (poda *weight-dependent*). Sem essa
    /// quota, um bloco de silêncio zera o ganho hebbiano e o decaimento
    /// metabólico dissolve a rede inteira (ρ→0): a plasticidade estrutural
    /// homeostática de Gallinaro & Rotter (arXiv:1706.02912) e o acoplamento
    /// poda↔memória de Millán et al. (arXiv:1705.02773) mostram que a
    /// conectividade deve estabilizar num ponto de operação, não em zero.
    fn enforce_homeostatic_density(&mut self) {
        let n = self.n;
        if n < 2 {
            return;
        }
        let pairs = n * (n - 1) / 2;
        let density = self.cfg.homeostatic_density;
        let target = ((density * pairs as f32).round() as usize).clamp(1, pairs);

        self.prune_buf.clear();
        for i in 0..n {
            for k in (i + 1)..n {
                let w = self.j[i * n + k];
                if w != 0.0 && w.is_finite() {
                    self.prune_buf.push((w.abs(), i as u32, k as u32));
                }
            }
        }
        let active = self.prune_buf.len();
        if active <= target {
            return;
        }

        // Ordem total determinística: |J| crescente, desempate por (i,k).
        self.prune_buf.sort_unstable_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        let remove = active - target;
        for idx in 0..remove {
            let (_, i, k) = self.prune_buf[idx];
            let (i, k) = (i as usize, k as usize);
            self.j[i * n + k] = 0.0;
            self.j[k * n + i] = 0.0;
        }
    }

    /// Homeostase espectral tolerante: ignora erros de ρ não finito.
    pub fn homeostasis(&mut self) {
        let _ = self.try_homeostasis();
    }

    /// Homeostase espectral por **raio espectral real** (iteração de potência).
    ///
    /// Escala `J` por `alvo/ρ` quando `ρ > alvo` (nunca amplifica — silêncio
    /// deve poder esfriar a rede), respeitando o teto de Gershgorin
    /// `gershgorin_cap · alvo`. Retorna o `ρ` medido **antes** da normalização.
    ///
    /// Se `J`/`ρ` não forem finitos, retorna
    /// [`MorphogenesisError::NonFiniteSpectralRadius`] **sem mutar** `J`.
    pub fn try_homeostasis(&mut self) -> Result<f32, MorphogenesisError> {
        let n = self.n;
        if n == 0 {
            return Ok(0.0);
        }
        if self.j.iter().any(|v| !v.is_finite()) {
            return Err(MorphogenesisError::NonFiniteSpectralRadius);
        }
        let rho = self.estimate_spectral_radius();
        if !rho.is_finite() {
            return Err(MorphogenesisError::NonFiniteSpectralRadius);
        }
        let target = self.cfg.target_spectral_radius;
        if rho <= target || rho <= f32::MIN_POSITIVE {
            return Ok(rho);
        }

        let mut scale = target / rho;
        let gershgorin = self.spectral_radius_bound();
        if !gershgorin.is_finite() {
            return Err(MorphogenesisError::NonFiniteSpectralRadius);
        }
        let cap = self.cfg.gershgorin_cap * target;
        if gershgorin > 0.0 && gershgorin * scale > cap {
            scale = scale.min(cap / gershgorin);
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(MorphogenesisError::NonFiniteSpectralRadius);
        }
        for v in self.j.iter_mut() {
            *v *= scale;
        }
        self.dissolve_pass();
        Ok(rho)
    }

    /// Raio espectral real estimado por iteração de potência (determinística).
    ///
    /// `O(N²)` por iteração com buffers pré-alocados; retorna `NaN` se `J`
    /// contiver valores não finitos e `0.0` para a matriz nula.
    pub fn spectral_radius(&mut self) -> f32 {
        self.estimate_spectral_radius()
    }

    /// Raio espectral aproximado (Gershgorin) — diagnóstico/teto de segurança.
    ///
    /// Retorna `f32::NAN` se houver qualquer entrada não finita (diagnóstico
    /// cego nunca deve mascarar NaN como `0.0`).
    pub fn spectral_radius_bound(&self) -> f32 {
        let n = self.n;
        if n == 0 {
            return 0.0;
        }
        let mut max = 0.0f32;
        for i in 0..n {
            let mut norm = 0.0f32;
            for k in 0..n {
                let v = self.j[i * n + k];
                if !v.is_finite() {
                    return f32::NAN;
                }
                norm += v.abs();
            }
            if !norm.is_finite() {
                return f32::NAN;
            }
            if norm > max {
                max = norm;
            }
        }
        max
    }

    /// Matriz de acoplamento como fatia (para alimentar o Módulo 1/6).
    pub fn coupling(&self) -> &[f32] {
        &self.j
    }

    /// Re-simetriza, zera diagonal, remove não finitos e poda `|J| < dissolve`.
    ///
    /// Idempotente; usado no começo de cada passo e na construção.
    fn sanitize(&mut self) {
        let n = self.n;
        let theta = self.cfg.dissolve;
        for i in 0..n {
            for k in (i + 1)..n {
                let idx_ik = i * n + k;
                let idx_ki = k * n + i;
                let a = self.j[idx_ik];
                let b = self.j[idx_ki];
                let mut v = if a.is_finite() && b.is_finite() {
                    0.5 * (a + b)
                } else if a.is_finite() {
                    a
                } else if b.is_finite() {
                    b
                } else {
                    0.0
                };
                if !v.is_finite() || v.abs() < theta {
                    v = 0.0;
                }
                self.j[idx_ik] = v;
                self.j[idx_ki] = v;
            }
            self.j[i * n + i] = 0.0;
        }
    }

    /// Poda sináptica após escalonamento uniforme (preserva a simetria).
    fn dissolve_pass(&mut self) {
        let n = self.n;
        let theta = self.cfg.dissolve;
        for i in 0..n {
            for k in (i + 1)..n {
                let idx_ik = i * n + k;
                let idx_ki = k * n + i;
                let mut v = 0.5 * (self.j[idx_ik] + self.j[idx_ki]);
                if !v.is_finite() || v.abs() < theta {
                    v = 0.0;
                }
                self.j[idx_ik] = v;
                self.j[idx_ki] = v;
            }
            self.j[i * n + i] = 0.0;
        }
    }

    /// `ρ(J)` via iteração de potência com inicialização determinística.
    fn estimate_spectral_radius(&mut self) -> f32 {
        let n = self.n;
        if n == 0 {
            return 0.0;
        }
        if self.j.iter().any(|v| !v.is_finite()) {
            return f32::NAN;
        }

        // Inicialização determinística e não degenerada (evita ortogonalidade
        // com o modo dominante, ex.: vetor de uns).
        let mut norm2 = 0.0f32;
        for (i, slot) in self.power_v.iter_mut().enumerate() {
            let v = ((i as f32) * 0.754_877_7 + 0.31).sin() + 0.025 * ((i % 5) as f32 - 2.0);
            *slot = v;
            norm2 += v * v;
        }
        let norm = norm2.sqrt();
        if !norm.is_finite() || norm <= f32::MIN_POSITIVE {
            return 0.0;
        }
        let inv = 1.0 / norm;
        for v in self.power_v.iter_mut() {
            *v *= inv;
        }

        let mut rho = 0.0f32;
        for _ in 0..POWER_ITERATIONS {
            Self::mat_vec(&self.j, &self.power_v, &mut self.power_av, n);
            let mut n2 = 0.0f32;
            for a in &self.power_av {
                n2 += a * a;
            }
            let nrm = n2.sqrt();
            if !nrm.is_finite() {
                return f32::NAN;
            }
            if nrm <= f32::MIN_POSITIVE {
                return 0.0;
            }
            let inv_n = 1.0 / nrm;
            for (v, a) in self.power_v.iter_mut().zip(self.power_av.iter()) {
                *v = a * inv_n;
            }
            let delta = (nrm - rho).abs();
            rho = nrm;
            if delta <= POWER_TOL * rho.max(1e-6) {
                break;
            }
        }

        // Quociente de Rayleigh final (O(N²)): preciso a O(θ²) no autovetor.
        // Em espectros quase degenerados (±λ) o quociente pode cancelar; a norma
        // `||Jv||/||v||` é exata nesse caso, então usamos o maior dos dois.
        let rho_norm = rho;
        Self::mat_vec(&self.j, &self.power_v, &mut self.power_av, n);
        let mut quotient = 0.0f32;
        for (v, a) in self.power_v.iter().zip(self.power_av.iter()) {
            quotient += v * a;
        }
        let estimate = quotient.abs().max(rho_norm);
        if estimate.is_finite() {
            estimate
        } else {
            f32::NAN
        }
    }

    /// `out = M·v` para matriz plana `n×n` (sem alocação).
    fn mat_vec(m: &[f32], v: &[f32], out: &mut [f32], n: usize) {
        for i in 0..n {
            let row = &m[i * n..(i + 1) * n];
            let mut acc = 0.0f32;
            for (k, w) in row.iter().enumerate() {
                acc += w * v[k];
            }
            out[i] = acc;
        }
    }
}
