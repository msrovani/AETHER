//! `aether_governor` — escalonador por núcleos do laço cognitivo.
//!
//! Topologia (pinning via `core_affinity`, *graceful no-op* se indisponível):
//!
//! ```text
//!        produtor ─► SPSC ─►┌───────────────────────────────────────────┐
//!   (sensores/rede)         │ NÚCLEO 0 (1º físico, irmãos SMT ociosos)   │
//!                           │ thread REFLEXIVA RT                        │
//!                           │  EventDrivenCore                           │
//!                           │   ├ ActiveInferenceEngine (EFE + Boltzmann)│
//!                           │   ├ LyapunovCbf   (V(z⁺) ≤ V_max)          │
//!                           │   ├ SindyIntegrator (ponteiro JIT atômico) │
//!                           │   └ SiliconEntropy  (u_rand do planejador) │
//!                           └──────┬───────────────────────┬─────────────┘
//!                       SPSC       │                       │     SPSC
//!                  StatePair       ▼                       ▼   SwarmObs
//!                 ┌───────────────────────┐   ┌───────────────────────────┐
//!                 │ NÚCLEO 1 — Koopman-   │   │ NÚCLEO 2 — SwarmSpeciation│
//!                 │ SINDy (RLS + STLSQ)   │   │ (Φ_niche, papéis)         │
//!                 │ → SindyJit::publish ──┼─► ponteiro atômico (acima)    │
//!                 └───────────────────────┘   └───────────────────────────┘
//! ```
//!
//! * **Reflexo** nunca bloqueia: só `try_pop`/`try_push` em filas SPSC; se uma
//!   fila secundária encher, a amostra é **descartada e contada**
//!   (`dropped_*`), jamais esperada.
//! * **Zero alocação** em [`EventDrivenCore::process`] (buffers fixos).
//! * [`CorePlan`] escolhe o 1º núcleo físico para o reflexo e **reserva os
//!   irmãos SMT** (não atribui nada a eles); os secundários usam os núcleos
//!   físicos seguintes.
//!
//! ## Limites honestos
//!
//! * "Isolado" aqui = *afinidade exclusiva do nosso lado*; o escalonador do SO
//!   ainda pode agendar outros processos nesse núcleo (isolamento total exige
//!   `isolcpus`/`SetProcessAffinityMask` no boot ou no *launcher*).
//! * A detecção de irmãos SMT supõe `logical / physical` ids adjacentes
//!   (enumeração típica do Windows). Em outras topologias use
//!   [`GovernorConfig::reflex_core`]/`koopman_core`/`swarm_core`.
//! * Sem prioridade de tempo real do SO (exigiria `SetThreadPriority` /
//!   `SCHED_FIFO`); o reflexo faz *spin* curto e depois `yield_now`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crate::core::active_inference::{ActiveInferenceEngine, LyapunovCbf, Zone};
use crate::core::koopman_sindy::{OnlineKoopmanOperator, SindyEquationDiscoverer, KOOPMAN_D};
use crate::core::silicon_entropy::SiliconEntropy;
use crate::core::sindy_jit::{SindyIntegrator, SindyJit};
use crate::core::spsc_ring::{SpscConsumer, SpscProducer, SpscRingBuffer};
use crate::core::swarm_speciation::{NodeState, SwarmSpeciationEngine, GENOME_DIM};
use crate::error::{AetherError, Result};

pub use crate::core::core_topology::{pin_current_thread, CorePlan};

/// Dimensão do estado latente (= dimensão do Koopman *lifted*).
pub const LATENT_DIM: usize = KOOPMAN_D;
/// Políticas do planejador: manter / seguir o fluxo SINDy / ir à meta.
pub const POLICIES: usize = 3;
/// Capacidade das filas SPSC (potência de dois).
pub const RING_CAPACITY: usize = 1024;

/// Amostra de sensor entregue ao reflexo.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReflexEvent {
    /// Instante da amostra (ns, definido pelo produtor).
    pub t_ns: u64,
    /// Estado latente medido.
    pub z: [f32; LATENT_DIM],
}

/// Par de estados consecutivos (treino do Koopman/SINDy).
#[derive(Clone, Copy, Debug)]
pub struct StatePair {
    /// `z_t`.
    pub x: [f32; LATENT_DIM],
    /// `z_{t+1}`.
    pub x_next: [f32; LATENT_DIM],
}

/// Observação de nó para a especiação do enxame.
#[derive(Clone, Copy, Debug)]
pub struct SwarmObs {
    /// Índice do nó.
    pub node: u32,
    /// Estado observado.
    pub state: NodeState,
}

/// Parâmetros do núcleo reflexivo.
#[derive(Clone, Copy, Debug)]
pub struct ReflexConfig {
    /// Meta `z*`.
    pub z_star: [f32; LATENT_DIM],
    /// Limite de Lyapunov `V_max`.
    pub v_max: f32,
    /// Temperatura do Boltzmann das políticas.
    pub temperature: f32,
    /// Ganho `γ` do CBF.
    pub gamma: f32,
    /// Peso epistêmico `λ`.
    pub lambda_epistemic: f32,
    /// Variância de observação das zonas.
    pub obs_var: f32,
    /// Passo do integrador SINDy (s).
    pub dt: f32,
}

impl Default for ReflexConfig {
    fn default() -> Self {
        Self {
            z_star: [0.0; LATENT_DIM],
            v_max: 4.0,
            temperature: 0.5,
            gamma: 0.5,
            lambda_epistemic: 1.0,
            obs_var: 0.05,
            dt: 1e-2,
        }
    }
}

/// Resultado de um evento processado.
#[derive(Clone, Copy, Debug)]
pub struct ReflexOutcome {
    /// Política escolhida.
    pub policy: usize,
    /// `V(z⁺)` do estado seguro.
    pub v: f32,
    /// Estado seguro proposto.
    pub z_next: [f32; LATENT_DIM],
    /// `true` se `V(z⁺) > V_max` (não deve ocorrer).
    pub violated: bool,
}

/// Núcleo orientado a eventos: um evento de sensor ⇒ um ciclo de decisão.
#[derive(Debug)]
pub struct EventDrivenCore {
    cfg: ReflexConfig,
    ai: ActiveInferenceEngine,
    cbf: LyapunovCbf,
    integrator: SindyIntegrator,
    entropy: SiliconEntropy,
    z_hats: [f32; POLICIES * LATENT_DIM],
    z_next: [f32; LATENT_DIM],
    last_policy: usize,
    events: u64,
    violations: u64,
    policy_counts: [u64; POLICIES],
}

impl EventDrivenCore {
    /// Constrói o núcleo (aloca motor e CBF **uma vez**).
    pub fn new(
        cfg: ReflexConfig,
        integrator: SindyIntegrator,
        entropy: SiliconEntropy,
    ) -> Result<Self> {
        if !(cfg.dt.is_finite() && cfg.dt > 0.0) {
            return Err(AetherError::Config("Governor: dt > 0".into()));
        }
        let zones = vec![
            Zone {
                mean: 0.0,
                var: 1.0,
                cost: 0.0
            };
            POLICIES
        ];
        let ai = ActiveInferenceEngine::new(zones, cfg.lambda_epistemic, cfg.obs_var)?
            .with_policy_dim(LATENT_DIM);
        let cbf = LyapunovCbf::new(LATENT_DIM, cfg.v_max)?;
        Ok(Self {
            cfg,
            ai,
            cbf,
            integrator,
            entropy,
            z_hats: [0.0; POLICIES * LATENT_DIM],
            z_next: [0.0; LATENT_DIM],
            last_policy: 0,
            events: 0,
            violations: 0,
            policy_counts: [0; POLICIES],
        })
    }

    /// Eventos processados.
    pub fn events(&self) -> u64 {
        self.events
    }

    /// Violações de `V_max` observadas (invariante: 0).
    pub fn violations(&self) -> u64 {
        self.violations
    }

    /// Histograma de políticas escolhidas.
    pub fn policy_counts(&self) -> [u64; POLICIES] {
        self.policy_counts
    }

    /// Processa um evento: **zero alocação**.
    pub fn process(&mut self, ev: &ReflexEvent) -> Result<ReflexOutcome> {
        let z = &ev.z;
        // 1. Previsão pelo fluxo SINDy (JIT ou interpretado) em (z0, z1).
        let mut x2 = [z[0], z[1]];
        self.integrator.rk4_step(&mut x2, self.cfg.dt);
        let d = [x2[0] - z[0], x2[1] - z[1]];

        // 2. Candidatos ẑ_k: manter | seguir o fluxo | ir à meta.
        let mut cost = 0.0f32;
        for i in 0..LATENT_DIM {
            let e = z[i] - self.cfg.z_star[i];
            cost += e * e;
            self.z_hats[i] = z[i];
            self.z_hats[LATENT_DIM + i] = z[i] + if i < 2 { d[i] } else { 0.0 };
            self.z_hats[2 * LATENT_DIM + i] = z[i] + 0.25 * (self.cfg.z_star[i] - z[i]);
        }
        // 3. Crença da zona escolhida no passo anterior ← custo observado.
        self.ai.observe(self.last_policy, 0.5 * cost);

        // 4. Planejamento com barreira de Lyapunov.
        let u_rand = self.entropy.next_f32();
        let (policy, v) = self.ai.plan_step(
            &mut self.cbf,
            z,
            &self.z_hats,
            &self.cfg.z_star,
            self.cfg.temperature,
            u_rand,
            self.cfg.gamma,
            &mut self.z_next,
        )?;
        self.last_policy = policy;
        self.events += 1;
        self.policy_counts[policy.min(POLICIES - 1)] += 1;
        let violated = v > self.cfg.v_max * (1.0 + 1e-4) + 1e-6;
        if violated {
            self.violations += 1;
        }
        Ok(ReflexOutcome {
            policy,
            v,
            z_next: self.z_next,
            violated,
        })
    }
}

/// Configuração do governador.
#[derive(Clone, Debug)]
pub struct GovernorConfig {
    /// Parâmetros do reflexo.
    pub reflex: ReflexConfig,
    /// Liga o pinning (no-op se indisponível).
    pub pin: bool,
    /// Sobrescreve o núcleo do reflexo.
    pub reflex_core: Option<usize>,
    /// Sobrescreve o núcleo do Koopman-SINDy.
    pub koopman_core: Option<usize>,
    /// Sobrescreve o núcleo do SwarmSpeciation.
    pub swarm_core: Option<usize>,
    /// Nós do enxame.
    pub swarm_nodes: usize,
    /// Passo de amostragem dos dados do SINDy (s).
    pub sindy_dt: f32,
    /// Limiar de esparsidade do STLSQ.
    pub sindy_tau: f64,
    /// Reajusta o SINDy a cada `refit_every` pares.
    pub refit_every: usize,
    /// Iterações de *spin* antes de `yield_now`.
    pub idle_spins: u32,
    /// Semente determinística do reflexo (`None` = silício).
    pub entropy_seed: Option<u64>,
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            reflex: ReflexConfig::default(),
            pin: true,
            reflex_core: None,
            koopman_core: None,
            swarm_core: None,
            swarm_nodes: 8,
            sindy_dt: 1e-2,
            sindy_tau: 0.02,
            refit_every: 32,
            idle_spins: 2048,
            entropy_seed: None,
        }
    }
}

/// Contadores compartilhados (apenas atômicos relaxados, sem `Mutex`).
#[derive(Debug, Default)]
struct GovStats {
    reflex_events: AtomicU64,
    reflex_errors: AtomicU64,
    cbf_violations: AtomicU64,
    dropped_koopman: AtomicU64,
    dropped_swarm: AtomicU64,
    koopman_updates: AtomicU64,
    sindy_fits: AtomicU64,
    sindy_publishes: AtomicU64,
    swarm_steps: AtomicU64,
    jit_generations: AtomicU64,
    jit_active: AtomicBool,
    pinned_reflex: AtomicBool,
    pinned_koopman: AtomicBool,
    pinned_swarm: AtomicBool,
}

/// Relatório final do governador.
#[derive(Clone, Debug, Default)]
pub struct GovernorReport {
    /// Eventos aceitos pela fila de entrada.
    pub submitted: u64,
    /// Eventos rejeitados por fila cheia na entrada.
    pub rejected: u64,
    /// Eventos processados pelo reflexo.
    pub reflex_events: u64,
    /// Erros do reflexo.
    pub reflex_errors: u64,
    /// Violações de `V_max` (invariante: 0).
    pub cbf_violations: u64,
    /// Amostras descartadas rumo ao Koopman (fila cheia).
    pub dropped_koopman: u64,
    /// Amostras descartadas rumo ao enxame (fila cheia).
    pub dropped_swarm: u64,
    /// Atualizações RLS do Koopman.
    pub koopman_updates: u64,
    /// Ajustes STLSQ do SINDy.
    pub sindy_fits: u64,
    /// Publicações de `Ξ` ao integrador.
    pub sindy_publishes: u64,
    /// Versões do integrador no fim.
    pub jit_generations: u64,
    /// `true` se o integrador terminou rodando código JIT.
    pub jit_active: bool,
    /// Passos do enxame.
    pub swarm_steps: u64,
    /// Plano de núcleos usado.
    pub plan: CorePlan,
    /// Pinning efetivamente aplicado: (reflexo, koopman, swarm).
    pub pinned: (bool, bool, bool),
}

/// Governador: possui as três threads e a fila de entrada.
#[derive(Debug)]
pub struct Governor {
    tx: SpscProducer<ReflexEvent, RING_CAPACITY>,
    stop: Arc<AtomicBool>,
    stats: Arc<GovStats>,
    handles: Vec<JoinHandle<()>>,
    plan: CorePlan,
    submitted: u64,
    rejected: u64,
}

impl Governor {
    /// Sobe as threads reflexiva, Koopman-SINDy e SwarmSpeciation.
    pub fn spawn(cfg: GovernorConfig) -> Result<Self> {
        if cfg.refit_every == 0 || cfg.swarm_nodes == 0 {
            return Err(AetherError::Config(
                "Governor: refit_every e swarm_nodes > 0".into(),
            ));
        }
        let mut plan = if cfg.pin {
            CorePlan::detect()
        } else {
            CorePlan::default()
        };
        if cfg.reflex_core.is_some() {
            plan.reflex = cfg.reflex_core;
        }
        if cfg.koopman_core.is_some() {
            plan.koopman = cfg.koopman_core;
        }
        if cfg.swarm_core.is_some() {
            plan.swarm = cfg.swarm_core;
        }

        let (tx_in, rx_in) = SpscRingBuffer::<ReflexEvent, RING_CAPACITY>::new().split();
        let (tx_k, rx_k) = SpscRingBuffer::<StatePair, RING_CAPACITY>::new().split();
        let (tx_s, rx_s) = SpscRingBuffer::<SwarmObs, RING_CAPACITY>::new().split();
        let (jit, integrator) = SindyJit::new();
        let entropy = match cfg.entropy_seed {
            Some(s) => SiliconEntropy::from_seed(s),
            None => SiliconEntropy::new(),
        };
        let core = EventDrivenCore::new(cfg.reflex, integrator, entropy)?;
        let koopman = OnlineKoopmanOperator::lifted()?;
        let sindy = SindyEquationDiscoverer::streaming(cfg.sindy_dt, cfg.sindy_tau)?;
        let swarm = SwarmSpeciationEngine::new(cfg.swarm_nodes, 0.5)?;

        let stop = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(GovStats::default());
        let mut handles = Vec::with_capacity(3);

        let spawn_err = |e: std::io::Error| AetherError::Config(format!("Governor: spawn: {e}"));

        // Secundárias primeiro: o reflexo já encontra os consumidores vivos.
        handles.push(
            thread::Builder::new()
                .name("aether-swarm".into())
                .spawn({
                    let (stop, stats) = (Arc::clone(&stop), Arc::clone(&stats));
                    let (pin, core_id, spins) = (cfg.pin, plan.swarm, cfg.idle_spins);
                    move || swarm_loop(swarm, rx_s, stop, stats, pin, core_id, spins)
                })
                .map_err(spawn_err)?,
        );
        handles.push(
            thread::Builder::new()
                .name("aether-koopman".into())
                .spawn({
                    let (stop, stats) = (Arc::clone(&stop), Arc::clone(&stats));
                    let (pin, core_id, spins) = (cfg.pin, plan.koopman, cfg.idle_spins);
                    let refit = cfg.refit_every;
                    move || {
                        koopman_loop(
                            koopman,
                            sindy,
                            jit,
                            rx_k,
                            stop,
                            stats,
                            (pin, core_id, spins),
                            refit,
                        )
                    }
                })
                .map_err(spawn_err)?,
        );
        handles.push(
            thread::Builder::new()
                .name("aether-reflex".into())
                .spawn({
                    let (stop, stats) = (Arc::clone(&stop), Arc::clone(&stats));
                    let (pin, core_id, spins) = (cfg.pin, plan.reflex, cfg.idle_spins);
                    let nodes = cfg.swarm_nodes as u32;
                    move || {
                        reflex_loop(
                            core,
                            rx_in,
                            tx_k,
                            tx_s,
                            stop,
                            stats,
                            (pin, core_id, spins),
                            nodes,
                        )
                    }
                })
                .map_err(spawn_err)?,
        );

        Ok(Self {
            tx: tx_in,
            stop,
            stats,
            handles,
            plan,
            submitted: 0,
            rejected: 0,
        })
    }

    /// Entrega uma amostra ao reflexo; `false` se a fila estiver cheia.
    #[inline]
    pub fn submit(&mut self, ev: ReflexEvent) -> bool {
        if self.tx.try_push(ev).is_ok() {
            self.submitted += 1;
            true
        } else {
            self.rejected += 1;
            false
        }
    }

    /// Plano de núcleos em uso.
    pub fn plan(&self) -> &CorePlan {
        &self.plan
    }

    /// Versão atual do integrador SINDy (0 = sem equações).
    pub fn jit_generation(&self) -> u64 {
        self.stats.jit_generations.load(Ordering::Acquire)
    }

    /// Eventos já processados pelo reflexo.
    pub fn processed(&self) -> u64 {
        self.stats.reflex_events.load(Ordering::Acquire)
    }

    /// Encerra: drena as filas, junta as threads e devolve o relatório.
    pub fn shutdown(mut self) -> Result<GovernorReport> {
        self.stop.store(true, Ordering::Release);
        for h in self.handles.drain(..) {
            h.join()
                .map_err(|_| AetherError::Config("Governor: thread entrou em panico".into()))?;
        }
        let s = &self.stats;
        let ld = |a: &AtomicU64| a.load(Ordering::Acquire);
        Ok(GovernorReport {
            submitted: self.submitted,
            rejected: self.rejected,
            reflex_events: ld(&s.reflex_events),
            reflex_errors: ld(&s.reflex_errors),
            cbf_violations: ld(&s.cbf_violations),
            dropped_koopman: ld(&s.dropped_koopman),
            dropped_swarm: ld(&s.dropped_swarm),
            koopman_updates: ld(&s.koopman_updates),
            sindy_fits: ld(&s.sindy_fits),
            sindy_publishes: ld(&s.sindy_publishes),
            jit_generations: ld(&s.jit_generations),
            jit_active: s.jit_active.load(Ordering::Acquire),
            swarm_steps: ld(&s.swarm_steps),
            plan: self.plan.clone(),
            pinned: (
                s.pinned_reflex.load(Ordering::Acquire),
                s.pinned_koopman.load(Ordering::Acquire),
                s.pinned_swarm.load(Ordering::Acquire),
            ),
        })
    }
}

impl Drop for Governor {
    fn drop(&mut self) {
        // Se `shutdown` não foi chamado, sinaliza e junta (sem panicar).
        self.stop.store(true, Ordering::Release);
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}

/// Espera ociosa: *spin* curto e depois `yield_now`.
#[inline]
fn idle(counter: &mut u32, spins: u32) {
    *counter = counter.saturating_add(1);
    if *counter < spins {
        std::hint::spin_loop();
    } else {
        thread::yield_now();
    }
}

#[allow(clippy::too_many_arguments)]
fn reflex_loop(
    mut core: EventDrivenCore,
    mut rx: SpscConsumer<ReflexEvent, RING_CAPACITY>,
    mut tx_k: SpscProducer<StatePair, RING_CAPACITY>,
    mut tx_s: SpscProducer<SwarmObs, RING_CAPACITY>,
    stop: Arc<AtomicBool>,
    stats: Arc<GovStats>,
    (pin, core_id, spins): (bool, Option<usize>, u32),
    nodes: u32,
) {
    if pin {
        stats
            .pinned_reflex
            .store(pin_current_thread(core_id), Ordering::Release);
    }
    let mut prev: Option<[f32; LATENT_DIM]> = None;
    let mut seq = 0u32;
    let mut idle_n = 0u32;
    loop {
        let Some(ev) = rx.try_pop() else {
            if stop.load(Ordering::Acquire) && rx.is_empty() {
                break;
            }
            idle(&mut idle_n, spins);
            continue;
        };
        idle_n = 0;
        match core.process(&ev) {
            Ok(out) => {
                stats.reflex_events.fetch_add(1, Ordering::Relaxed);
                if out.violated {
                    stats.cbf_violations.fetch_add(1, Ordering::Relaxed);
                }
                if let Some(x) = prev {
                    let pair = StatePair { x, x_next: ev.z };
                    if tx_k.try_push(pair).is_err() {
                        stats.dropped_koopman.fetch_add(1, Ordering::Relaxed);
                    }
                }
                prev = Some(ev.z);
                let obs = swarm_observation(seq % nodes, &ev.z, &out);
                if tx_s.try_push(obs).is_err() {
                    stats.dropped_swarm.fetch_add(1, Ordering::Relaxed);
                }
                seq = seq.wrapping_add(1);
            }
            Err(_) => {
                stats.reflex_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Constrói a observação de enxame a partir do estado e da decisão.
fn swarm_observation(node: u32, z: &[f32; LATENT_DIM], out: &ReflexOutcome) -> SwarmObs {
    let mut genome = [0.0f32; GENOME_DIM];
    genome[..LATENT_DIM].copy_from_slice(z);
    genome[LATENT_DIM] = out.policy as f32;
    genome[LATENT_DIM + 1] = out.v;
    let norm = z.iter().map(|v| v * v).sum::<f32>().sqrt();
    SwarmObs {
        node,
        state: NodeState {
            genome,
            phi_auto: norm,
            spectral_radius: out.v,
            plasticity: (out.policy as f32) / (POLICIES as f32),
            coherence: 1.0 / (1.0 + out.v),
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn koopman_loop(
    mut koopman: OnlineKoopmanOperator,
    mut sindy: SindyEquationDiscoverer,
    mut jit: SindyJit,
    mut rx: SpscConsumer<StatePair, RING_CAPACITY>,
    stop: Arc<AtomicBool>,
    stats: Arc<GovStats>,
    (pin, core_id, spins): (bool, Option<usize>, u32),
    refit_every: usize,
) {
    if pin {
        stats
            .pinned_koopman
            .store(pin_current_thread(core_id), Ordering::Release);
    }
    let mut since_fit = 0usize;
    let mut idle_n = 0u32;
    loop {
        let Some(p) = rx.try_pop() else {
            if stop.load(Ordering::Acquire) && rx.is_empty() {
                break;
            }
            idle(&mut idle_n, spins);
            continue;
        };
        idle_n = 0;
        if koopman.observe_state(&p.x, &p.x_next).is_ok() {
            stats.koopman_updates.fetch_add(1, Ordering::Relaxed);
        }
        sindy.push([p.x_next[0], p.x_next[1]]);
        since_fit += 1;
        if since_fit >= refit_every {
            since_fit = 0;
            if sindy.fit_window().is_ok() {
                stats.sindy_fits.fetch_add(1, Ordering::Relaxed);
                if let Ok(outcome) = jit.publish(&sindy) {
                    use crate::core::sindy_jit::PublishOutcome as O;
                    if outcome != O::Unchanged {
                        stats.sindy_publishes.fetch_add(1, Ordering::Relaxed);
                    }
                    let generation = match outcome {
                        O::Jitted { generation, .. } | O::Interpreted { generation } => generation,
                        O::Unchanged => stats.jit_generations.load(Ordering::Relaxed),
                    };
                    stats.jit_generations.store(generation, Ordering::Release);
                    if outcome != O::Unchanged {
                        stats
                            .jit_active
                            .store(matches!(outcome, O::Jitted { .. }), Ordering::Release);
                    }
                }
            }
        }
    }
}

fn swarm_loop(
    mut swarm: SwarmSpeciationEngine,
    mut rx: SpscConsumer<SwarmObs, RING_CAPACITY>,
    stop: Arc<AtomicBool>,
    stats: Arc<GovStats>,
    pin: bool,
    core_id: Option<usize>,
    spins: u32,
) {
    if pin {
        stats
            .pinned_swarm
            .store(pin_current_thread(core_id), Ordering::Release);
    }
    let n = swarm.len() as u32;
    let mut seen = 0u32;
    let mut idle_n = 0u32;
    loop {
        let Some(o) = rx.try_pop() else {
            if stop.load(Ordering::Acquire) && rx.is_empty() {
                break;
            }
            idle(&mut idle_n, spins);
            continue;
        };
        idle_n = 0;
        if swarm.set_node(o.node as usize, o.state).is_ok() {
            seen += 1;
            if seen >= n {
                seen = 0;
                swarm.step();
                stats.swarm_steps.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
