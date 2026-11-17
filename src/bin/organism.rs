//! Entry point do organismo AETHER.
//!
//! ```text
//! cargo run --release --bin aether-organism -- --duration 15s --plain
//! ```
//!
//! Flags:
//! * `--duration <s>`   duração da simulação (padrão: 15s)
//! * `--plain`         painel ANSI sem dependências (padrão)
//! * `--tui`           painel ratatui (feature `tui`)
//! * `--seed <u64>`    semente do RNG
//! * `--steps <n>`     passos de relaxamento por quadro (padrão: 200)
//! * `--window <n>`    amostras do buffer circular (padrão: 128)
//! * `--perturb-at <s>` momento da perturbação física (padrão: 3s)
//! * `--hz <f>`        taxa do loop (padrão: 60)
//! * `--sensor-noise <f>` ruído do sensor em rad (padrão: 0.35)
//! * `--no-perturb`    desliga a perturbação (controle)

use std::time::Duration;

use aether_engine::simulation::closed_loop_organism::{ClosedLoopOrganism, OrganismConfig};
use aether_engine::simulation::panel::PanelSink;

fn print_help() {
    println!(
        "aether-organism — organismo pós-token AETHER v{}\n\
         \n\
         Uso:\n\
           aether-organism [flags]\n\
         \n\
         Flags:\n\
           --help | -h          esta mensagem (exit 0)\n\
           --duration <s>       duração (padrão: 15s); aceita sufixo s\n\
           --plain              painel ANSI (padrão)\n\
           --tui                pede Ratatui (requer --features tui; senão cai em --plain)\n\
           --seed <u64>         RNG\n\
           --steps <n>          relaxamento/quadro (padrão: 200)\n\
           --window <n>         buffer circular (padrão: 128)\n\
           --perturb-at <s>     instante da perturbação (padrão: 3s)\n\
           --hz <f>             taxa do loop (padrão: 60)\n\
           --sensor-noise <f>   ruído sensor rad (padrão: 0.35)\n\
           --no-perturb         desliga perturbação\n\
         \n\
         Demo rápida: .\\scripts\\run-demo.ps1\n\
         Hub Windows: aether-dashboard --features dashboard\n\
         Non-goal: não expõe BAM/JIT/governor neste CLI.",
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_args() -> Result<(OrganismConfig, Duration, bool), String> {
    let mut cfg = OrganismConfig::default();
    let mut duration = Duration::from_secs(15);
    let mut want_tui = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        let flag = a.clone();
        let value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("faltou valor para {flag}"))
        };
        match a.as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--duration" => {
                let raw = value(&mut i)?;
                let v = raw
                    .strip_suffix('s')
                    .or_else(|| raw.strip_suffix('S'))
                    .unwrap_or(&raw);
                let secs: f64 = v.parse().map_err(|e| format!("--duration inválido: {e}"))?;
                if !secs.is_finite() || secs <= 0.0 {
                    return Err("--duration deve ser > 0 e finito".into());
                }
                duration = Duration::from_secs_f64(secs);
            }
            "--seed" => {
                let v = value(&mut i)?;
                cfg.seed = v.parse().map_err(|e| format!("--seed inválido: {e}"))?;
            }
            "--steps" => {
                let v = value(&mut i)?;
                cfg.steps = v.parse().map_err(|e| format!("--steps inválido: {e}"))?;
            }
            "--window" => {
                let v = value(&mut i)?;
                cfg.window = v.parse().map_err(|e| format!("--window inválido: {e}"))?;
            }
            "--perturb-at" => {
                let raw = value(&mut i)?;
                let v = raw
                    .strip_suffix('s')
                    .or_else(|| raw.strip_suffix('S'))
                    .unwrap_or(&raw);
                let secs: f64 = v
                    .parse()
                    .map_err(|e| format!("--perturb-at inválido: {e}"))?;
                if !secs.is_finite() || secs < 0.0 {
                    return Err("--perturb-at deve ser >= 0 e finito".into());
                }
                cfg.perturb_after = Duration::from_secs_f64(secs);
            }
            "--hz" => {
                let v = value(&mut i)?;
                cfg.hz = v.parse().map_err(|e| format!("--hz inválido: {e}"))?;
            }
            "--sensor-noise" => {
                let v = value(&mut i)?;
                let noise: f64 = v
                    .parse()
                    .map_err(|e| format!("--sensor-noise inválido: {e}"))?;
                if !noise.is_finite() || noise < 0.0 {
                    return Err(format!(
                        "--sensor-noise deve ser finito e >= 0, veio {noise}"
                    ));
                }
                cfg.sensor_noise = noise;
            }
            "--no-perturb" => {
                cfg.perturb_after = Duration::from_secs(3600);
            }
            "--plain" => {}
            "--tui" => {
                want_tui = true;
            }
            other => return Err(format!("flag desconhecida: {other}")),
        }
        i += 1;
    }
    cfg.perturb_after = cfg
        .perturb_after
        .max(Duration::from_secs_f64(cfg.window as f64 / cfg.hz + 0.5));
    Ok((cfg, duration, want_tui))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut cfg, duration, want_tui) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("erro de uso: {e}");
            eprintln!("dica: aether-organism --help");
            std::process::exit(2);
        }
    };
    cfg.decision_dim = 64;

    println!("AETHER — organismo pós-token v{}", env!("CARGO_PKG_VERSION"));
    println!(
        "nós=3 canais={} janela={} passos={} hz={} ruído_sensor={} perturb_em={:?}",
        cfg.sensor_channels, cfg.window, cfg.steps, cfg.hz, cfg.sensor_noise, cfg.perturb_after
    );

    let mut organism = ClosedLoopOrganism::new(cfg)?;

    #[cfg(feature = "tui")]
    if want_tui {
        let mut tui = aether_engine::simulation::panel::RatatuiPanel::new()?;
        let run = organism.run(Some(duration), &mut tui);
        let _ = tui.teardown();
        run?;
        println!("\nsimulação encerrada.");
        return Ok(());
    }

    if want_tui {
        eprintln!(
            "aviso: --tui pediu Ratatui, mas o binário foi compilado sem --features tui; usando --plain.\n\
             rebuild: cargo build --release --bin aether-organism --features tui"
        );
    }
    let mut plain = aether_engine::simulation::panel::PlainPanel::new();
    organism.run(Some(duration), &mut plain)?;
    let _ = plain.teardown();
    println!("\nsimulação encerrada.");
    Ok(())
}
