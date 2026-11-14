//! Painel de terminal do organismo (Módulo 4).
//!
//! Duas implementações de [`PanelSink`]:
//! * [`PlainPanel`] — ANSI puro, sem dependências, funciona em qualquer
//!   terminal (e em CI). É o padrão.
//! * [`RatatuiPanel`] — TUI completa, habilitada pela feature `tui`.

use crate::error::Result;
use crate::simulation::closed_loop_organism::OrganismFrame;

/// Destino de quadros renderizados.
pub trait PanelSink {
    /// Desenha (ou reprinta) um quadro.
    fn draw(&mut self, frame: &OrganismFrame) -> Result<()>;
    /// Restaura o terminal (usado no encerramento).
    fn teardown(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Barra horizontal proporcional (sparkline) para uma série `f64`.
pub fn sparkline(values: &[f64], width: usize) -> String {
    if values.is_empty() || width == 0 {
        return String::new();
    }
    const GLYPHS: &[u8] = b" .:-=+*#%@";
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let span = (max - min).abs().max(1e-12);
    let n = values.len().min(width);
    let start = values.len() - n;
    let mut out = String::with_capacity(n);
    for v in &values[start..] {
        let t = ((*v - min) / span).clamp(0.0, 1.0);
        let idx = (t * (GLYPHS.len() - 1) as f64).round() as usize;
        out.push(GLYPHS[idx] as char);
    }
    out
}

/// Barra de progresso `0..1`.
pub fn bar(value: f64, width: usize) -> String {
    let filled = ((value.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    let mut s = String::with_capacity(width);
    for i in 0..width {
        s.push(if i < filled { '#' } else { '.' });
    }
    s
}

/// Painel ANSI sem dependências — reprinta o bloco no lugar.
#[derive(Debug)]
pub struct PlainPanel {
    lines: usize,
    throttle: std::time::Duration,
    last: std::time::Instant,
}

impl Default for PlainPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl PlainPanel {
    /// Cria o painel (imprime 10 linhas de espaço para reservar a área).
    pub fn new() -> Self {
        let p = Self {
            lines: 0,
            throttle: std::time::Duration::from_millis(33),
            last: std::time::Instant::now(),
        };
        print!("{}", "\n".repeat(10));
        p
    }

    /// Redesenha o bloco completo.
    fn render(&mut self, frame: &OrganismFrame) {
        let mut out = String::new();
        out.push_str("\x1b[H\x1b[2J"); // home + clear
        out.push_str("\x1b[1mAETHER\x1b[0m — organismo pós-token (física contínua)\n");
        out.push_str(&format!(
            "t = {:>7.2} s   |   quadro = {:>5} µs   |   drops = {}\n",
            frame.elapsed.as_secs_f64(),
            frame.frame_cost_us,
            frame.dropped_frames
        ));
        out.push('\n');

        out.push_str("\x1b[1mCoerência de fase R(t)\x1b[0m\n");
        out.push_str(&format!(
            "  R = {:.4}  [{}]   nós: {:.3} {:.3} {:.3}{}\n",
            frame.order_parameter,
            bar(frame.order_parameter, 40),
            frame.r_per_node[0],
            frame.r_per_node[1],
            frame.r_per_node[2],
            if frame.perturbed {
                "   (pós-perturbação)"
            } else {
                ""
            }
        ));
        out.push('\n');

        out.push_str("\x1b[1mEntropia espectral\x1b[0m\n");
        out.push_str(&format!(
            "  cru = {:.4}  ->  interferido = {:.4}   (redução = {:+.4})\n",
            frame.entropy_raw,
            frame.entropy_wave,
            frame.entropy_wave - frame.entropy_raw
        ));
        out.push('\n');

        out.push_str("\x1b[1mEnergia Hamiltoniana H(t)\x1b[0m\n");
        out.push_str(&format!(
            "  H(s*) = {:+.5}    H(s0) = {:+.5}    Δ = {:+.5}\n",
            frame.energy,
            frame.energy_initial,
            frame.energy - frame.energy_initial
        ));
        out.push_str(&format!("  {}\n", sparkline(&frame.energy_trace, 72)));
        out.push('\n');

        out.push_str("\x1b[1mEstado emergente s*\x1b[0m\n");
        let bits: String = frame
            .states
            .iter()
            .map(|s| if *s > 0 { '1' } else { '0' })
            .collect();
        out.push_str(&format!("  {}\n", bits));
        out.push_str(&format!(
            "  fase Nó_Bancada: [{:.3} {:.3} {:.3} {:.3}]\n",
            frame.phase_bancada[0],
            frame.phase_bancada[1],
            frame.phase_bancada[2],
            frame.phase_bancada[3]
        ));

        print!("{out}");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        self.lines = out.lines().count();
    }
}

impl PanelSink for PlainPanel {
    fn draw(&mut self, frame: &OrganismFrame) -> Result<()> {
        if self.last.elapsed() < self.throttle {
            return Ok(());
        }
        self.last = std::time::Instant::now();
        self.render(frame);
        Ok(())
    }
}

/// Painel TUI completo (ratatui + crossterm), atrás da feature `tui`.
#[cfg(feature = "tui")]
#[derive(Debug)]
pub struct RatatuiPanel {
    terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    history: std::collections::VecDeque<f64>,
}

#[cfg(feature = "tui")]
impl RatatuiPanel {
    /// Cria o terminal em modo alternativo.
    pub fn new() -> Result<Self> {
        use crossterm::ExecutableCommand;
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let terminal = ratatui::Terminal::new(backend)?;
        Ok(Self {
            terminal,
            history: std::collections::VecDeque::with_capacity(256),
        })
    }
}

#[cfg(feature = "tui")]
impl PanelSink for RatatuiPanel {
    fn draw(&mut self, frame: &OrganismFrame) -> Result<()> {
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::text::{Line, Span};
        use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

        self.history.push_back(frame.order_parameter);
        if self.history.len() > 256 {
            self.history.pop_front();
        }

        let terminal = &mut self.terminal;
        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(6),
                    Constraint::Min(3),
                ])
                .split(area);

            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        "AETHER",
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::Cyan),
                    ),
                    Span::raw(format!(
                        "  t={:.2}s  custo={}µs  drops={}",
                        frame.elapsed.as_secs_f64(),
                        frame.frame_cost_us,
                        frame.dropped_frames
                    )),
                ]))
                .block(Block::default().borders(Borders::ALL).title("AETHER")),
                chunks[0],
            );

            f.render_widget(
                Gauge::default()
                    .ratio(frame.order_parameter.clamp(0.0, 1.0))
                    .label(format!("R(t) = {:.4}", frame.order_parameter))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Coerência de fase"),
                    ),
                chunks[1],
            );

            f.render_widget(
                Paragraph::new(format!(
                    "entropia cru = {:.4}   interferido = {:.4}",
                    frame.entropy_raw, frame.entropy_wave
                ))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Entropia espectral"),
                ),
                chunks[2],
            );

            let mut lines = vec![Line::from(format!(
                "H(s*) = {:+.5}   H(s0) = {:+.5}",
                frame.energy, frame.energy_initial
            ))];
            lines.push(Line::from(sparkline(&frame.energy_trace, 72)));
            f.render_widget(
                Paragraph::new(lines).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Energia Hamiltoniana"),
                ),
                chunks[3],
            );

            let bits: String = frame
                .states
                .iter()
                .map(|s| if *s > 0 { '1' } else { '0' })
                .collect();
            f.render_widget(
                Paragraph::new(bits).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Estado emergente s*"),
                ),
                chunks[4],
            );
        })?;
        Ok(())
    }

    fn teardown(&mut self) -> Result<()> {
        use crossterm::ExecutableCommand;
        let _ = self.terminal.show_cursor();
        let mut stdout = std::io::stdout();
        stdout.execute(crossterm::terminal::LeaveAlternateScreen)?;
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }
}
