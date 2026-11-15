//! E6 — perfil por estagio do caminho quente (Módulo 4).
//!
//! Responde à pergunta P0 do dossiê (`docs/O_PROBLEMA_DOS_3_CORPOS_NAO_VOL2.md` §12):
//! **onde vão os microssegundos de um quadro do organismo?**
//!
//! Sem `unsafe` e sem custo no caminho normal: a instrumentação vive atrás de um
//! parâmetro `const PROF: bool` ([`ClosedLoopOrganism::step_profiled_into`]); quando
//! desligado, o compilador elimina todo o bloco e [`ClosedLoopOrganism::step_into`]
//! permanece bit-a-bit idêntico ao anterior.
//!
//! ```no_run
//! # use aether_engine::simulation::closed_loop_organism::{ClosedLoopOrganism, OrganismConfig, OrganismFrame};
//! # use aether_engine::simulation::profile::StageProfile;
//! # fn main() -> aether_engine::Result<()> {
//! let cfg = OrganismConfig::default();
//! let mut org = ClosedLoopOrganism::new(cfg.clone())?;
//! let mut frame = OrganismFrame::with_capacity(cfg.steps, cfg.decision_dim);
//! let mut prof = StageProfile::default();
//! prof.reset();
//! org.step_profiled_into(&mut frame, &mut prof)?;
//! println!("cobertura: {:.2}%", 100.0 * prof.coverage());
//! # Ok(()) }
//! ```
//!
//! O perfil usa [`std::time::Instant`] (QPC no Windows), não `rdtsc`: o crate é
//! `#![forbid(unsafe_code)]` e `_rdtsc` exige `unsafe`.

/// Número de estágios instrumentados.
pub const STAGE_COUNT: usize = 10;

/// Rótulos estáveis (usados no relatório e no JSON).
pub const STAGE_NAMES: [&str; STAGE_COUNT] = [
    "sample",    // leitura dos 3 handles Kuramoto + perturbação
    "sensor",    // escrita do buffer circular (fasor + ruído)
    "ring_copy", // linearização do buffer em `u` (com módulo por amostra)
    "wave",      // forward espectral (FFT + interferência + IFFT)
    "entropy",   // entropia espectral crua + entropia do campo transferido
    "readout",   // leitura espectral -> h(t) + normalização
    "solver",    // relaxamento termodinâmico (steps x N^2)
    "frame",     // cópia de energy_trace/states para o quadro
    "actuator",  // campo médio local + EMA da inércia
    "tail",      // carimbos de tempo e campos escalares do quadro
];

/// Índices nomeados dos estágios.
pub mod stage {
    pub const SAMPLE: usize = 0;
    pub const SENSOR: usize = 1;
    pub const RING_COPY: usize = 2;
    pub const WAVE: usize = 3;
    pub const ENTROPY: usize = 4;
    pub const READOUT: usize = 5;
    pub const SOLVER: usize = 6;
    pub const FRAME: usize = 7;
    pub const ACTUATOR: usize = 8;
    pub const TAIL: usize = 9;
}

/// Acumulador de tempos por estágio (nanossegundos).
///
/// `reset()` é responsabilidade do chamador: para medir por quadro, resete antes
/// de cada `step_profiled_into`; para medir um lote inteiro, deixe acumular.
#[derive(Clone, Debug, Default)]
pub struct StageProfile {
    /// Tempo acumulado por estágio, em ns.
    pub ns: [u64; STAGE_COUNT],
    /// Tempo total do último quadro medido (`t0..fim`), em ns.
    pub total_ns: u64,
}

impl StageProfile {
    /// Zera tempos e total.
    pub fn reset(&mut self) {
        self.ns = [0; STAGE_COUNT];
        self.total_ns = 0;
    }

    /// Soma dos estágios instrumentados (ns).
    pub fn accounted_ns(&self) -> u64 {
        self.ns.iter().copied().sum()
    }

    /// Fração do quadro coberta pela instrumentação (`0..=1`).
    pub fn coverage(&self) -> f64 {
        if self.total_ns == 0 {
            0.0
        } else {
            self.accounted_ns() as f64 / self.total_ns as f64
        }
    }

    /// Fração do total atribuída ao estágio `i`.
    pub fn fraction(&self, i: usize) -> f64 {
        if self.total_ns == 0 {
            0.0
        } else {
            self.ns[i] as f64 / self.total_ns as f64
        }
    }

    /// Estágio dominante e sua fração, se houver tempo medido.
    pub fn top(&self) -> Option<(usize, f64)> {
        if self.total_ns == 0 {
            return None;
        }
        let mut best = 0usize;
        for i in 1..STAGE_COUNT {
            if self.ns[i] > self.ns[best] {
                best = i;
            }
        }
        Some((best, self.fraction(best)))
    }

    /// Nome do estágio `i`.
    pub fn name(i: usize) -> &'static str {
        STAGE_NAMES.get(i).copied().unwrap_or("?")
    }
}
