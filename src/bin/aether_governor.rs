//! `aether-governor` — governador de hardware real (Fase 6).
//!
//! Fluxo fechado, um ciclo por quadro:
//!
//! ```text
//! HostTransducer ──► EventDrivenCore ──► OnlineKoopmanOperator
//!        │                  │                    │
//!        │                  ▼                    ▼
//!        │            WaveInterference      ActiveInference (CBF)
//!        │                  │                    │
//!        ▼                  ▼                    ▼
//!   BamFhrrStore ◄── cadência/dt        RenormalizationHierarchy
//!        │                                         │
//!        └──────────► StateBridge (32 B) ◄── KuramotoPhaseNode (LAN)
//! ```
//!
//! # Uso
//!
//! ```text
//! cargo run --release --bin aether-governor -- --headless --duration-secs 10
//! cargo run --release --bin aether-governor                 # interativo (m4)
//! cargo run --release --bin aether-governor -- --pin-core   # com --features affinity
//! ```
//!
//! Teclas (só o intervalo é medido; nenhum caractere é gravado):
//! `1` pulso térmico · `2` rajada de ruído · `3` gravar assinatura · `q` sair.
//!
//! O frame de 32 bytes vai para `%TEMP%\aether_state.bin` a cada 250 ms.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use aether_engine::core::active_inference::{ActiveInferenceEngine, Zone};
use aether_engine::core::aether_governor::{
    EventDrivenCore, ReflexConfig, ReflexEvent, LATENT_DIM,
};
use aether_engine::core::koopman_sindy::{OnlineKoopmanOperator, KOOPMAN_D};
use aether_engine::core::kuramoto_field::KuramotoPhaseNode;
use aether_engine::core::renormalization::RenormalizationHierarchy;
use aether_engine::core::silicon_entropy::SiliconEntropy;
use aether_engine::core::sindy_jit::SindyJit;
use aether_engine::platform::host_transducer::{HostTransducer, WAVE_CHANNELS};
use aether_engine::platform::lan;
use aether_engine::platform::state_bridge::{GovernorState, StateBridge};

/// Período nominal do ciclo (s).
const PERIOD: Duration = Duration::from_millis(10);
/// Canais micro da hierarquia de renormalização.
const RG_MICRO: usize = 64;

#[derive(Debug)]
struct Cli {
    headless: bool,
    duration: Option<Duration>,
    pin_core: bool,
    node_id: u32,
    state_path: Option<PathBuf>,
    no_lan: bool,
    quiet: bool,
}

fn usage() -> &'static str {
    "aether-governor [--headless] [--duration-secs N] [--pin-core] [--node-id N]\n\
     \x20               [--state-path PATH] [--no-lan] [--quiet]"
}

fn parse_args() -> Result<Cli, String> {
    let mut cli = Cli {
        headless: false,
        duration: None,
        pin_core: false,
        node_id: std::process::id().wrapping_mul(2_654_435_761) | 1,
        state_path: None,
        no_lan: false,
        quiet: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--headless" => cli.headless = true,
            "--pin-core" => cli.pin_core = true,
            "--no-lan" => cli.no_lan = true,
            "--quiet" => cli.quiet = true,
            "--duration-secs" => {
                let raw = args.next().ok_or("--duration-secs exige um valor")?;
                let secs: f64 = raw
                    .parse()
                    .map_err(|_| format!("duracao invalida: {raw}"))?;
                if !(secs.is_finite() && secs > 0.0) {
                    return Err(format!("duracao deve ser > 0: {secs}"));
                }
                cli.duration = Some(Duration::from_secs_f64(secs));
            }
            "--node-id" => {
                let raw = args.next().ok_or("--node-id exige um valor")?;
                cli.node_id = raw
                    .parse()
                    .map_err(|_| format!("node-id invalido: {raw}"))?;
            }
            "--state-path" => {
                let raw = args.next().ok_or("--state-path exige um caminho")?;
                cli.state_path = Some(PathBuf::from(raw));
            }
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other => return Err(format!("argumento desconhecido: {other}\n{}", usage())),
        }
    }
    Ok(cli)
}

fn err<E: std::fmt::Display>(e: E) -> String {
    format!("{e}")
}

/// Regime de fluxo derivado da camada macro (L2) da renormalização.
fn flow_regime(macro_order: f32) -> u8 {
    if macro_order >= 0.90 {
        0 // Basal: coerente e calmo
    } else if macro_order >= 0.60 {
        1 // DeepFocus
    } else if macro_order >= 0.30 {
        2 // Vigilante
    } else {
        3 // Critico
    }
}

/// Papel ecológico a partir da política dominante do reflexo.
fn niche_role(counts: [u64; 3]) -> u8 {
    let mut best = 0usize;
    for (i, v) in counts.iter().enumerate() {
        if *v > counts[best] {
            best = i;
        }
    }
    best as u8
}

fn run() -> Result<(), String> {
    let cli = parse_args()?;

    // ---------------------------------------------------------------- sensores
    let mut transducer = HostTransducer::new(0xA37E_6006).map_err(err)?;
    transducer.calibrate();

    #[cfg(feature = "m4")]
    if !cli.headless {
        let shared = aether_engine::platform::host_transducer::spawn_cadence_tap().map_err(err)?;
        transducer.attach_cadence(shared);
        println!(
            "[governor] interativo: 1=pulso 2=ruido 3=gravar q=sair (apenas Δt; nenhum caractere e gravado)"
        );
    }
    #[cfg(not(feature = "m4"))]
    if !cli.headless {
        eprintln!("[governor] feature `m4` desligada: seguindo sem teclado (use --headless)");
    }

    // ---------------------------------------------------------------- pinning
    if cli.pin_core {
        #[cfg(feature = "affinity")]
        {
            let rep = aether_engine::core::affinity::pin_reflex_core(0);
            println!(
                "[governor] pinning: core={:?} pinned={} logical={} physical={}",
                rep.core, rep.pinned, rep.logical, rep.physical
            );
        }
        #[cfg(not(feature = "affinity"))]
        eprintln!("[governor] --pin-core exige --features affinity; seguindo sem pinning");
    }

    // ---------------------------------------------------------------- núcleo
    let (_jit, integrator) = SindyJit::new();
    let reflex_cfg = ReflexConfig {
        z_star: [0.0; LATENT_DIM],
        ..ReflexConfig::default()
    };
    let mut core = EventDrivenCore::new(
        reflex_cfg,
        integrator,
        SiliconEntropy::from_seed(u64::from(cli.node_id)),
    )
    .map_err(err)?;
    let mut koopman = OnlineKoopmanOperator::lifted().map_err(err)?;
    let zones = vec![
        Zone {
            mean: 0.0,
            var: 1.0,
            cost: 0.0
        };
        3
    ];
    let mut inference = ActiveInferenceEngine::new(zones, 1.0, 0.05)
        .map_err(err)?
        .with_policy_dim(LATENT_DIM);
    let mut hierarchy = RenormalizationHierarchy::new(RG_MICRO).map_err(err)?;

    // ---------------------------------------------------------------- LAN
    let mut node = if cli.no_lan {
        None
    } else {
        match KuramotoPhaseNode::new(lan::node_config(cli.node_id, lan::LAN_HZ)) {
            Ok(n) => Some(n),
            Err(e) => {
                eprintln!("[governor] LAN indisponivel ({e}); seguindo local");
                None
            }
        }
    };

    // ---------------------------------------------------------------- estado
    let mut bridge = StateBridge::new(cli.state_path.clone()).map_err(err)?;
    bridge.clean_stale_tmp();

    let mut wave = [0.0f32; WAVE_CHANNELS];
    let mut z = [0.0f32; LATENT_DIM];
    let mut pred = [0.0f32; KOOPMAN_D];
    let mut phases = [0.0f32; RG_MICRO];
    let mut violations = 0u64;
    let mut cycles = 0u64;
    let start = Instant::now();
    let mut last_report = Instant::now();

    loop {
        let cycle_start = Instant::now();
        let next_tick = cycle_start + PERIOD;

        // 1. Sensores reais (zero alocação).
        transducer.sample_wave_into(&mut wave).map_err(err)?;
        let sample = transducer.sample();

        // 2. Estado latente: onda + jitter + carga + cadência + identidade.
        z[0] = sample.wave[0].tanh();
        z[1] = sample.wave[1].tanh();
        z[2] = (sample.jitter_ns / 100_000.0).tanh();
        z[3] = sample.load;
        z[4] = (sample.dt_us / 100_000.0).tanh();
        z[5] = sample.identity;

        // 3. Reflexo com barreira de Lyapunov.
        let out = core
            .process(&ReflexEvent {
                t_ns: cycle_start.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                z,
            })
            .map_err(err)?;
        if out.violated {
            violations += 1;
        }

        // 4. Koopman on-line (aprende o par z_t → z_{t+1}).
        koopman.observe_state(&z, &out.z_next).map_err(err)?;
        let _ = koopman.predict_steps(&out.z_next, 1, &mut pred);

        // 5. Inferência ativa: energia livre esperada da política escolhida.
        inference.observe(out.policy.min(2), out.v);
        let free_energy = inference.expected_free_energy(out.policy.min(2));

        // 6. Kuramoto LAN (multicast 239.255.42.99:7777).
        let (lan_order, peers, lan_phase) = match node.as_mut() {
            Some(n) => match n.step(PERIOD.as_secs_f64()) {
                Ok(frame) => (
                    frame.order_parameter as f32,
                    frame.peers_heard.min(u16::MAX as u32) as u16,
                    frame.phase,
                ),
                Err(_) => (0.0, 0, [0.0; 4]),
            },
            None => (0.0, 0, [0.0; 4]),
        };

        // 7. Renormalização 64 → 16 → 4 (camada macro = L2).
        for (i, phase) in phases.iter_mut().enumerate() {
            let base = lan_phase[i % 4] as f32;
            *phase = base + 0.05 * (i as f32);
        }
        if let Err(e) = hierarchy.forward(&phases) {
            eprintln!("[governor] RG indisponivel: {e}");
        }
        let rg_order = hierarchy.macro_order().clamp(0.0, 1.0);

        // 8. Coerência: local (RG) + LAN quando há peers.
        let coherence = if peers > 0 {
            0.5 * (rg_order + lan_order.clamp(0.0, 1.0))
        } else {
            rg_order
        };

        let cycle_us = cycle_start.elapsed().as_micros().min(u64::MAX as u128) as u32;
        let state = GovernorState {
            coherence_r: coherence,
            identity_confidence: sample.identity,
            free_energy,
            flow_regime: flow_regime(rg_order),
            niche_role: niche_role(core.policy_counts()),
            lan_peers: peers,
            cycle_us,
        };
        bridge.maybe_write(&state).map_err(err)?;
        cycles += 1;

        // 9. Telemetria viva (a cada 500 ms).
        if !cli.quiet && last_report.elapsed() >= Duration::from_millis(500) {
            last_report = Instant::now();
            println!(
                "[{:>6.2}s] r={:.3} id={:.3} F={:+.4} {} role={} peers={} cycle={}us jitter={:.0}ns load={:.2} sig={}",
                start.elapsed().as_secs_f64(),
                state.coherence_r,
                state.identity_confidence,
                state.free_energy,
                state.flow_label(),
                state.role_label(),
                state.lan_peers,
                state.cycle_us,
                sample.jitter_ns,
                sample.load,
                sample.signatures,
            );
        }

        // 10. Encerramento gracioso (publica o último estado, mesmo em corridas curtas).
        let timed_out = cli.duration.is_some_and(|d| start.elapsed() >= d);
        if timed_out || transducer.quit_requested() {
            bridge.write_now(&state).map_err(err)?;
            break;
        }

        // 11. Cadência do ciclo (o sleep não entra em `cycle_us`).
        let now = Instant::now();
        if now < next_tick {
            std::thread::sleep(next_tick - now);
        }
    }

    #[cfg(feature = "m4")]
    aether_engine::platform::host_transducer::disable_raw_mode();

    if !cli.quiet {
        println!(
            "[governor] fim: {} ciclos, {} escritas, {} violacoes CBF, frame={} ({} B, crc32 ok)",
            cycles,
            bridge.writes(),
            violations,
            bridge.path().display(),
            aether_engine::platform::state_bridge::STATE_BYTES,
        );
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("aether-governor: {e}");
        std::process::exit(1);
    }
}
