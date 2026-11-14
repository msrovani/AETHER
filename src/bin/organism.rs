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
//! * `--no-perturb`    desliga a perturbação (controle)

use std::time::Duration;

use aether_engine::simulation::closed_loop_organism::{ClosedLoopOrganism, OrganismConfig};

fn parse_args() -> Result<(OrganismConfig, Duration), String> {
    let mut cfg = OrganismConfig::default();
    let mut duration = Duration::from_secs(15);
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
            "--duration" => {
                let v = value(&mut i)?;
                let secs: f64 = v.parse().map_err(|e| format!("--duration inválido: {e}"))?;
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
                let v = value(&mut i)?;
                let secs: f64 = v.parse().map_err(|e| format!("--perturb-at inválido: {e}"))?;
                cfg.perturb_after = Duration::from_secs_f64(secs);
            }
            "--hz" => {
                let v = value(&mut i)?;
                cfg.hz = v.parse().map_err(|e| format!("--hz inválido: {e}"))?;
            }
            "--no-perturb" => {
                cfg.perturb_after = Duration::from_secs(3600);
            }
            "--plain" | "--tui" => {}
            other => return Err(format!("flag desconhecida: {other}")),
        }
        i += 1;
    }
    cfg.perturb_after = cfg.perturb_after.max(Duration::from_secs_f64(
        cfg.window as f64 / cfg.hz + 0.5,
    ));
    Ok((cfg, duration))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut cfg, duration) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("erro de uso: {e}");
            std::process::exit(2);
        }
    };
    cfg.decision_dim = 64;

    println!("AETHER — organismo pós-token");
    println!(
        "nós=3 canais={} janela={} passos={} hz={} perturb_em={:?}",
        cfg.sensor_channels,
        cfg.window,
        cfg.steps,
        cfg.hz,
        cfg.perturb_after
    );

    let mut organism = ClosedLoopOrganism::new(cfg)?;
    let mut plain = aether_engine::simulation::panel::PlainPanel::new();
    organism.run(Some(duration), &mut plain)?;
    println!("\nsimulação encerrada.");
    Ok(())
}
