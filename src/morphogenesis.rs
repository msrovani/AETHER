//! Módulo 7 — Plasticidade topológica por Energia Livre Variacional.
//!
//! A matriz de acoplamento `J_ij` **não é estática**: o circuito cultiva novas
//! conexões onde há novidade física (surpresa alta) e dissolve sinapses
//! ociosas por decaimento metabólico.
//!
//! ```text
//! F(t) = (1/L) Σ_x (u(x,t) − v̂(x,t))² + λ_metabolic · Σ_ij |J_ij|
//!
//! dJ_ij/dt = κ_grow · tanh(F) · (s_i s_j − J_ij) − κ_decay · J_ij
//! |J_ij| < θ_dissolve  ⇒  J_ij = 0   (poda sináptica física)
//! ```
//!
//! Garantias estruturais a cada passo: `J_ij = J_ji` (simetria estrita) e
//! `J_ii = 0`. Homeostase espectral: normalização periódica para manter
//! ρ(J) ≈ 1.0 (regime crítico entre ordem rígida e caos).

/// Parâmetros morfogenéticos.
#[derive(Clone, Copy, Debug)]
pub struct MorphogenesisConfig {
    /// Ganho de crescimento hebbiano `κ_grow`.
    pub k_grow: f32,
    /// Decaimento metabólico `κ_decay`.
    pub k_decay: f32,
    /// Limiar de dissolução `θ_dissolve`.
    pub dissolve: f32,
    /// Peso metabólico da energia livre `λ_metabolic`.
    pub lambda_metabolic: f32,
    /// Raio espectral alvo (homeostase).
    pub target_spectral_radius: f32,
    /// Ciclos entre normalizações espectrais.
    pub homeostasis_period: usize,
}

impl Default for MorphogenesisConfig {
    fn default() -> Self {
        Self {
            k_grow: 0.35,
            k_decay: 0.012,
            dissolve: 0.005,
            lambda_metabolic: 0.002,
            target_spectral_radius: 1.0,
            homeostasis_period: 50,
        }
    }
}

/// Campo morfogenético: dono e evoluidor da topologia `J`.
pub struct MorphogeneticField {
    n: usize,
    cfg: MorphogenesisConfig,
    /// Matriz plana `N×N` (ordem C, simétrica, diagonal zero).
    pub j: Vec<f32>,
    cycles: usize,
    /// Última energia livre calculada.
    pub free_energy: f32,
    /// Última energia de surpresa (erro de predição) calculada.
    pub surprise: f32,
}

impl std::fmt::Debug for MorphogeneticField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MorphogeneticField")
        .field("n", &self.n)
        .field("cfg", &self.cfg)
        .field("free_energy", &self.free_energy)
        .field("surprise", &self.surprise)
        .finish()
    }
}

impl MorphogeneticField {
    /// Cria o campo a partir de uma matriz plana (cópia, simetrizada).
    pub fn new(n: usize, j0: &[f32], cfg: MorphogenesisConfig) -> Self {
        assert_eq!(j0.len(), n * n, "J0 precisa ser [N,N] plana");
        let mut j = j0.to_vec();
        for i in 0..n {
            j[i * n + i] = 0.0;
            for k in (i + 1)..n {
                let avg = 0.5 * (j[i * n + k] + j[k * n + i]);
                j[i * n + k] = avg;
                j[k * n + i] = avg;
            }
        }
        Self {
            n,
            cfg,
            j,
            cycles: 0,
            free_energy: 0.0,
            surprise: 0.0,
        }
    }

    /// Dimensão `N`.
    pub fn dim(&self) -> usize {
        self.n
    }

    /// Fração de conexões ativas (`|J_ij| > θ_dissolve`, fora da diagonal).
    pub fn sparsity_ratio(&self) -> f32 {
        let n = self.n;
        let mut active = 0usize;
        let mut total = 0usize;
        for i in 0..n {
            for k in 0..n {
                if i != k {
                    total += 1;
                    if self.j[i * n + k].abs() > self.cfg.dissolve {
                        active += 1;
                    }
                }
            }
        }
        if total == 0 {
            0.0
        } else {
            active as f32 / total as f32
        }
    }

    /// Energia livre variacional (surpresa + custo metabólico).
    ///
    /// `prediction_error` é o MSE médio `(1/L) Σ (u − v̂)²`; o termo
    /// metabólico é `λ · Σ|J|`.
    pub fn free_energy(&self, prediction_error: f32) -> f32 {
        let metabolic: f32 = self.j.iter().map(|v| v.abs()).sum();
        prediction_error + self.cfg.lambda_metabolic * metabolic
    }

    /// Um passo morfogenético.
    ///
    /// * `prediction_error`: MSE da predição (surpresa) do ciclo atual;
    /// * `active_spins`: estado motor/emergente atual (len n, valores contínuos
    ///   — o produto `s_i·s_j` modula onde crescer conexão);
    /// * retorna a energia livre atualizada `F(t)`.
    pub fn evolve_topology(&mut self, prediction_error: f32, active_spins: &[f32], dt: f32) -> f32 {
        let n = self.n;
        assert_eq!(active_spins.len(), n, "active_spins precisa ter n entradas");

        let surprise = self.free_energy(prediction_error);
        self.surprise = surprise;
        let growth = self.cfg.k_grow * surprise.tanh();

        for i in 0..n {
            for k in (i + 1)..n {
                let idx_ik = i * n + k;
                let idx_ki = k * n + i;
                let corr = active_spins[i] * active_spins[k];
                let dj = growth * (corr - self.j[idx_ik]) - self.cfg.k_decay * self.j[idx_ik];
                let new_val = self.j[idx_ik] + dj * dt;

                // cristalização/dissolução simétrica
                let v = if new_val.abs() < self.cfg.dissolve {
                    0.0
                } else {
                    new_val
                };
                self.j[idx_ik] = v;
                self.j[idx_ki] = v;
            }
            self.j[i * n + i] = 0.0;
        }

        self.cycles += 1;
        if self.cfg.homeostasis_period > 0 && self.cycles % self.cfg.homeostasis_period == 0 {
            self.homeostasis();
        }

        self.free_energy = self.free_energy(prediction_error);
        self.free_energy
    }

    /// Homeostase espectral: normaliza as linhas para ρ(J) ≈ alvo.
    ///
    /// Aproximação de raio espectral: soma absoluta das linhas (limite superior
    /// de Gershgorin) — barata, determinística e suficiente para manter o
    /// sistema no regime crítico.
    pub fn homeostasis(&mut self) {
        let n = self.n;
        let target = self.cfg.target_spectral_radius;
        for i in 0..n {
            let row = &self.j[i * n..(i + 1) * n];
            let norm: f32 = row.iter().map(|v| v.abs()).sum();
            if norm > target * 1.5 {
                let scale = target / norm;
                for v in self.j[i * n..(i + 1) * n].iter_mut() {
                    *v *= scale;
                }
            } else if norm < target * 0.5 {
                // linhas famintas ganham nutrição proporcional (mas não explosiva)
                let boost = 1.0 + (target - norm) / target.max(1e-6) * 0.1;
                for v in self.j[i * n..(i + 1) * n].iter_mut() {
                    *v *= boost;
                }
            }
        }
        // re-simetria após o escalonamento por linha
        for i in 0..n {
            for k in (i + 1)..n {
                let avg = 0.5 * (self.j[i * n + k] + self.j[k * n + i]);
                self.j[i * n + k] = avg;
                self.j[k * n + i] = avg;
            }
            self.j[i * n + i] = 0.0;
        }
    }

    /// Raio espectral aproximado (Gershgorin) — diagnóstico.
    pub fn spectral_radius_bound(&self) -> f32 {
        let n = self.n;
        let mut max = 0.0f32;
        for i in 0..n {
            let norm: f32 = self.j[i * n..(i + 1) * n].iter().map(|v| v.abs()).sum();
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
}
