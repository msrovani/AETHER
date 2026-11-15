//! Anel de telemetria dos demos (organismo / daemon) para gráficos no hub.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::simulation::closed_loop_organism::OrganismFrame;
use crate::simulation::panel::PanelSink;

const CAP: usize = 600;

/// Amostra de um quadro do demo.
#[derive(Clone, Copy, Debug, Default)]
pub struct DemoSample {
    pub t: f32,
    pub r: f32,
    pub energy: f32,
    pub entropy: f32,
    pub chatter: f32,
    pub cost_us: f32,
}

/// Buffer compartilhado UI ↔ job.
#[derive(Clone, Debug)]
pub struct DemoTrace {
    pub title: String,
    pub running: bool,
    pub samples: VecDeque<DemoSample>,
    /// Última curva H(t) do relaxamento (organismo).
    pub energy_trace: Vec<f32>,
}

impl Default for DemoTrace {
    fn default() -> Self {
        Self {
            title: String::new(),
            running: false,
            samples: VecDeque::with_capacity(CAP),
            energy_trace: Vec::new(),
        }
    }
}

impl DemoTrace {
    /// Limpa e prepara para um novo demo.
    pub fn begin(&mut self, title: impl Into<String>) {
        self.title = title.into();
        self.running = true;
        self.samples.clear();
        self.energy_trace.clear();
    }

    /// Marca fim.
    pub fn end(&mut self) {
        self.running = false;
    }

    /// Empurra amostra (descarta as mais antigas).
    pub fn push(&mut self, s: DemoSample) {
        if self.samples.len() >= CAP {
            self.samples.pop_front();
        }
        self.samples.push_back(s);
    }

    /// Extrai série `f` das amostras.
    pub fn series<F: Fn(&DemoSample) -> f32>(&self, f: F) -> Vec<f32> {
        self.samples.iter().map(f).collect()
    }
}

/// `PanelSink` que grava no anel.
pub struct TraceSink {
    pub trace: Arc<Mutex<DemoTrace>>,
}

impl PanelSink for TraceSink {
    fn draw(&mut self, frame: &OrganismFrame) -> Result<()> {
        if let Ok(mut g) = self.trace.lock() {
            g.push(DemoSample {
                t: frame.elapsed.as_secs_f32(),
                r: frame.order_parameter as f32,
                energy: frame.energy as f32,
                entropy: frame.entropy_wave as f32,
                chatter: frame.continuous_action_delta,
                cost_us: frame.frame_cost_us as f32,
            });
            g.energy_trace = frame.energy_trace.iter().map(|v| *v as f32).collect();
        }
        Ok(())
    }
}

/// Parse de uma linha de telemetria do `aether-daemon`.
pub fn parse_daemon_line(line: &str) -> Option<DemoSample> {
    // ciclo N | R(LAN)=0.123 peers=… | F=0.456 S=0.789 | …
    let r = find_f32(line, "R(LAN)=")?;
    let f = find_f32(line, "F=")?;
    let s = find_f32(line, "S=").unwrap_or(0.0);
    let ciclo = line
        .split_whitespace()
        .nth(1)
        .and_then(|t| t.parse::<f32>().ok())
        .unwrap_or(0.0);
    Some(DemoSample {
        t: ciclo / 60.0, // aprox. segundos a 60 Hz
        r,
        energy: f,
        entropy: s,
        chatter: 0.0,
        cost_us: find_f32(line, "µs")
            .or_else(|| {
                line.rsplit('|')
                    .nth(1)
                    .and_then(|p| p.trim().trim_end_matches('µ').trim_end_matches('s').parse().ok())
            })
            .unwrap_or(0.0),
    })
}

fn find_f32(line: &str, key: &str) -> Option<f32> {
    let i = line.find(key)?;
    let rest = &line[i + key.len()..];
    let tok = rest
        .split(|c: char| c.is_whitespace() || c == '|')
        .next()?
        .trim()
        .trim_end_matches(',');
    tok.parse().ok()
}
