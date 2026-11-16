//! Hub Windows AETHER: scorecard, jobs, hardware e fichas T01–T12.
//!
//! ```text
//! cargo run --release --bin aether-dashboard --features dashboard
//! ```

#![deny(unsafe_code)]

#[cfg(all(windows, feature = "dashboard"))]
fn main() -> eframe::Result<()> {
    aether_dashboard_win::run()
}

#[cfg(not(all(windows, feature = "dashboard")))]
fn main() {
    eprintln!(
        "aether-dashboard: disponivel apenas no Windows com --features dashboard\n\
         Exemplo: cargo run --release --bin aether-dashboard --features dashboard"
    );
    std::process::exit(2);
}

#[cfg(all(windows, feature = "dashboard"))]
mod aether_dashboard_win {
    use eframe::egui::{self, Color32, FontId, RichText, Sense, Stroke, Vec2};
    use eframe::App;

    use aether_engine::dashboard::{
        dimension_ficha, dimension_tooltip, fmt_compact, global_tooltip, glossary_entries,
        load_host_report, load_scorecard, throughput_tooltip, DimFicha, HostReport, JobKind,
        JobPhase, JobRunner, ScorecardView,
    };

    /// Paleta do hub (lab slate + teal — sem glow/roxo).
    mod theme {
        use eframe::egui::Color32;

        pub const BG: Color32 = Color32::from_rgb(14, 18, 22);
        pub const PANEL: Color32 = Color32::from_rgb(20, 26, 32);
        pub const PANEL_ALT: Color32 = Color32::from_rgb(24, 32, 38);
        pub const CARD: Color32 = Color32::from_rgb(26, 34, 40);
        pub const CARD_HOVER: Color32 = Color32::from_rgb(32, 42, 50);
        pub const CARD_SEL: Color32 = Color32::from_rgb(28, 44, 48);
        pub const STROKE: Color32 = Color32::from_rgb(48, 62, 72);
        pub const MUTED: Color32 = Color32::from_rgb(130, 146, 156);
        pub const TEXT: Color32 = Color32::from_rgb(224, 232, 236);
        pub const BRAND: Color32 = Color32::from_rgb(56, 168, 148);
        pub const ACCENT: Color32 = Color32::from_rgb(72, 148, 168);
        pub const WARN: Color32 = Color32::from_rgb(188, 148, 56);
        pub const BAD: Color32 = Color32::from_rgb(196, 84, 64);
        pub const OK: Color32 = Color32::from_rgb(48, 152, 108);
        pub const RUN: Color32 = Color32::from_rgb(72, 148, 188);
    }

    /// Conteúdo da coluna direita (explicação).
    #[derive(Clone, Debug)]
    enum DetailPane {
        Idle,
        Hover { title: String, body: String },
        Ficha(DimFicha),
    }

    impl DetailPane {
        fn mode_label(&self) -> &'static str {
            match self {
                Self::Idle => "glossário",
                Self::Hover { .. } => "hover",
                Self::Ficha(_) => "ficha",
            }
        }

        fn ficha_id(&self) -> Option<&str> {
            match self {
                Self::Ficha(f) => Some(f.id),
                _ => None,
            }
        }
    }

    /// App do hub.
    pub struct DashboardApp {
        view: ScorecardView,
        status: String,
        path_arg: Option<String>,
        jobs: JobRunner,
        host: Option<HostReport>,
        detail: DetailPane,
        hover_scratch: Option<(String, String)>,
        confirm_daemon: bool,
        last_job_done: bool,
        theme_ready: bool,
    }

    impl DashboardApp {
        fn reload(&mut self) {
            let path = self.path_arg.as_ref().map(std::path::Path::new);
            match load_scorecard(path) {
                Ok(v) => {
                    self.status = format!("carregado: {}", v.path.display());
                    self.view = v;
                }
                Err(e) => {
                    self.status = format!("erro: {e}");
                    self.view.load_error = Some(e.to_string());
                }
            }
        }

        fn try_load_host(&mut self) {
            if let Ok(r) = load_host_report() {
                self.host = Some(r);
            }
        }

        fn start_job(&mut self, kind: JobKind) {
            let sc = if kind == JobKind::HostProbe {
                Some(self.view.clone())
            } else {
                None
            };
            if self.jobs.try_start(kind, sc) {
                self.last_job_done = false;
                self.status = format!("job: {}…", kind.label());
            } else {
                self.status = "já há um job em execução (single-flight)".into();
            }
        }

        fn note_hover(&mut self, title: impl Into<String>, body: impl Into<String>) {
            self.hover_scratch = Some((title.into(), body.into()));
        }

        fn commit_hover(&mut self) {
            if let Some((title, body)) = self.hover_scratch.take() {
                // Ficha fixa não cede a hover; limpa só no botão / novo clique T0x.
                if matches!(self.detail, DetailPane::Ficha(_)) {
                    return;
                }
                self.detail = DetailPane::Hover { title, body };
            } else if !matches!(self.detail, DetailPane::Ficha(_)) {
                self.detail = DetailPane::Idle;
            }
        }

        fn poll_jobs(&mut self) {
            let snap = self.jobs.snapshot();
            if snap.phase == JobPhase::Running {
                self.status = format!(
                    "[{}] {}",
                    snap.kind.map(|k| k.label()).unwrap_or("job"),
                    snap.log
                );
                return;
            }
            if !self.last_job_done && matches!(snap.phase, JobPhase::Done | JobPhase::Failed) {
                self.last_job_done = true;
                self.status = format!("[{}] {}", snap.phase.label(), snap.log);
                if snap.kind == Some(JobKind::HostProbe) && snap.phase == JobPhase::Done {
                    // Job já gravou o JSON — só relê (não re-sonda).
                    self.try_load_host();
                }
                if matches!(snap.kind, Some(JobKind::Audit) | Some(JobKind::Bench))
                    && snap.phase == JobPhase::Done
                {
                    self.reload();
                }
            }
        }

        fn ensure_theme(&mut self, ctx: &egui::Context) {
            if self.theme_ready {
                return;
            }
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = theme::PANEL;
            visuals.window_fill = theme::PANEL_ALT;
            visuals.extreme_bg_color = theme::BG;
            visuals.widgets.noninteractive.bg_fill = theme::CARD;
            visuals.widgets.inactive.bg_fill = theme::CARD;
            visuals.widgets.hovered.bg_fill = theme::CARD_HOVER;
            visuals.widgets.active.bg_fill = theme::CARD_SEL;
            visuals.selection.bg_fill = Color32::from_rgb(40, 90, 100);
            visuals.override_text_color = Some(theme::TEXT);
            ctx.set_visuals(visuals);
            self.theme_ready = true;
        }
    }

    impl App for DashboardApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.ensure_theme(ctx);
            self.poll_jobs();
            self.hover_scratch = None;
            let demo_live = self.jobs.trace.lock().map(|t| t.running).unwrap_or(false);
            if self.jobs.snapshot().phase == JobPhase::Running || demo_live {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }

            egui::TopBottomPanel::top("top")
                .frame(
                    egui::Frame::none()
                        .fill(theme::PANEL)
                        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                        .stroke(Stroke::new(1.0_f32, theme::STROKE)),
                )
                .show(ctx, |ui| {
                    self.draw_top_bar(ui);
                });

            egui::SidePanel::right("detail")
                .default_width(380.0)
                .min_width(300.0)
                .resizable(true)
                .frame(
                    egui::Frame::none()
                        .fill(theme::PANEL_ALT)
                        .inner_margin(egui::Margin::same(12.0))
                        .stroke(Stroke::new(1.0_f32, theme::STROKE)),
                )
                .show(ctx, |ui| {
                    self.draw_detail_pane(ui);
                });

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(theme::BG)
                        .inner_margin(egui::Margin::same(14.0)),
                )
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.draw_globals(ui);
                        ui.add_space(14.0);
                        self.draw_dimensions(ui);
                        ui.add_space(14.0);
                        self.draw_throughput(ui);
                        ui.add_space(14.0);
                        self.draw_host(ui);
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(
                                "Hover → Detalhe · clique T0x = ficha fixa · demos → gráficos.",
                            )
                            .small()
                            .color(theme::MUTED),
                        );
                    });
                });

            self.commit_hover();

            if self.confirm_daemon {
                egui::Window::new("Demo daemon — aviso multicast")
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(
                            "O aether-daemon abre UDP multicast 239.255.42.99:7777 \
                             (~10 s). Sem peers na LAN, R(LAN) não é auditável.",
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Cancelar").clicked() {
                                self.confirm_daemon = false;
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Confirmar e lançar").color(theme::TEXT),
                                    )
                                    .fill(theme::BRAND),
                                )
                                .clicked()
                            {
                                self.confirm_daemon = false;
                                self.start_job(JobKind::DaemonDemo);
                            }
                        });
                    });
            }
        }
    }

    impl DashboardApp {
        fn section_header(&self, ui: &mut egui::Ui, title: &str) {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 18.0), Sense::hover());
                ui.painter().rect_filled(rect, 1.0, theme::BRAND);
                ui.label(RichText::new(title).strong().size(15.0).color(theme::TEXT));
            });
            ui.add_space(6.0);
        }

        fn draw_detail_pane(&mut self, ui: &mut egui::Ui) {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("Detalhe")
                        .strong()
                        .size(18.0)
                        .color(theme::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let chip = self.detail.mode_label();
                    ui.label(RichText::new(chip).small().strong().color(theme::BRAND));
                });
            });
            ui.add_space(4.0);
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &self.detail {
                    DetailPane::Idle => {
                        ui.label(
                            RichText::new(
                                "Passe o mouse sobre um medidor. Clique T0x para fixar a ficha.",
                            )
                            .small()
                            .color(theme::MUTED),
                        );
                        ui.add_space(10.0);
                        ui.label(RichText::new("Glossário").strong().color(theme::ACCENT));
                        ui.add_space(4.0);
                        for (k, v) in glossary_entries() {
                            ui.label(RichText::new(*k).strong().small().color(theme::TEXT));
                            ui.label(RichText::new(*v).small().color(theme::MUTED));
                            ui.add_space(6.0);
                        }
                    }
                    DetailPane::Hover { title, body } => {
                        ui.label(RichText::new(title).strong().size(15.0).color(theme::TEXT));
                        ui.add_space(6.0);
                        for para in body.split('\n') {
                            if para.is_empty() {
                                ui.add_space(4.0);
                            } else {
                                ui.label(RichText::new(para).size(13.0).color(theme::TEXT));
                            }
                        }
                    }
                    DetailPane::Ficha(ficha) => {
                        let ficha = *ficha;
                        ui.label(
                            RichText::new(format!("{} — {}", ficha.id, ficha.title))
                                .strong()
                                .size(15.0)
                                .color(theme::BRAND),
                        );
                        ui.add_space(6.0);
                        for para in ficha.tooltip.split('\n') {
                            if para.is_empty() {
                                ui.add_space(4.0);
                            } else {
                                ui.label(RichText::new(para).small().color(theme::TEXT));
                            }
                        }
                        ui.separator();
                        ui.label(
                            RichText::new("Como interpretar")
                                .strong()
                                .color(theme::ACCENT),
                        );
                        ui.label(RichText::new(ficha.interpret).color(theme::TEXT));
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Se baixo, o que falha")
                                .strong()
                                .color(theme::ACCENT),
                        );
                        ui.label(RichText::new(ficha.if_low).color(theme::TEXT));
                        ui.add_space(4.0);
                        ui.label(RichText::new("Proveniência").strong().color(theme::ACCENT));
                        ui.label(
                            RichText::new(ficha.provenance)
                                .italics()
                                .small()
                                .color(theme::MUTED),
                        );
                        ui.add_space(10.0);
                        if ui.button("Limpar ficha").clicked() {
                            self.detail = DetailPane::Idle;
                        }
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                self.draw_demo_graphs(ui);
            });
        }

        fn draw_demo_graphs(&self, ui: &mut egui::Ui) {
            let trace = self
                .jobs
                .trace
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();

            ui.label(
                RichText::new("Gráficos do demo")
                    .strong()
                    .size(15.0)
                    .color(theme::TEXT),
            );
            if trace.title.is_empty() && trace.samples.is_empty() {
                ui.label(
                    RichText::new("Ainda sem série. Clique em “Demo organismo” ou “Demo daemon”.")
                        .small()
                        .color(theme::MUTED),
                );
                return;
            }

            let status = if trace.running {
                "ao vivo"
            } else {
                "última corrida"
            };
            ui.label(
                RichText::new(format!(
                    "{} · {} · {} pts",
                    trace.title,
                    status,
                    trace.samples.len()
                ))
                .small()
                .color(theme::ACCENT),
            );
            ui.add_space(4.0);

            if !trace.samples.is_empty() {
                let r = trace.series(|s| s.r);
                let e = trace.series(|s| s.energy);
                let s = trace.series(|s| s.entropy);
                let c = trace.series(|s| s.chatter);
                let cost = trace.series(|s| s.cost_us);

                sparkline(ui, "R / R(LAN)", &r, theme::BRAND);
                sparkline(ui, "H / F (energia ou surpresa)", &e, theme::WARN);
                sparkline(ui, "S (entropia)", &s, theme::ACCENT);
                if c.iter().any(|v| *v > 0.0) {
                    sparkline(ui, "chatter ‖Δu‖", &c, Color32::from_rgb(180, 96, 120));
                }
                if cost.iter().any(|v| *v > 0.0) {
                    sparkline(ui, "custo quadro (µs)", &cost, theme::MUTED);
                }
            }

            if !trace.energy_trace.is_empty() {
                ui.add_space(4.0);
                sparkline(
                    ui,
                    "H(t) último relaxamento",
                    &trace.energy_trace,
                    Color32::from_rgb(100, 190, 150),
                );
            }
        }

        fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("AETHER")
                        .strong()
                        .size(24.0)
                        .color(theme::BRAND),
                );
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "hub · métricas + testes · v{}",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .small()
                        .color(theme::MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Recarregar").clicked() {
                        self.reload();
                        self.try_load_host();
                    }
                    if self.jobs.is_busy() && ui.button("Cancelar job").clicked() {
                        self.jobs.request_cancel();
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                self.action_with_detail(
                    ui,
                    "Rodar auditoria cognitiva",
                    "Regenera target/aether_scorecard.json (T01–T12 / ACQ).",
                    !self.jobs.is_busy(),
                    JobKind::Audit,
                );
                self.action_with_detail(
                    ui,
                    "Benchmark de vazão",
                    "Basal/Vigilante/Crítico (~5 s/regime) via throughput_tps.",
                    !self.jobs.is_busy(),
                    JobKind::Bench,
                );
                self.action_with_detail(
                    ui,
                    "Sondar hardware",
                    "CPU/RAM/features + gaps → target/aether_host_report.json.",
                    !self.jobs.is_busy(),
                    JobKind::HostProbe,
                );
                self.action_with_detail(
                    ui,
                    "Demo organismo",
                    "Organismo in-process 15 s @ 30 Hz → gráficos R/H/S.",
                    !self.jobs.is_busy(),
                    JobKind::OrganismDemo,
                );
                let tip = "Lança aether-daemon curto (exige confirmação — multicast).";
                let resp = ui.add_enabled(!self.jobs.is_busy(), egui::Button::new("Demo daemon"));
                if resp.hovered() {
                    self.note_hover("Demo daemon", tip);
                }
                if resp.clicked() {
                    self.confirm_daemon = true;
                }
            });

            let snap = self.jobs.snapshot();
            let phase_color = match snap.phase {
                JobPhase::Idle => theme::MUTED,
                JobPhase::Running => theme::RUN,
                JobPhase::Done => theme::OK,
                JobPhase::Failed => theme::BAD,
            };
            ui.add_space(6.0);
            egui::Frame::none()
                .fill(Color32::from_rgb(18, 28, 34))
                .stroke(Stroke::new(1.0_f32, phase_color))
                .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                .rounding(3.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("job · {}", snap.phase.label()))
                                .strong()
                                .color(phase_color),
                        );
                        ui.label(RichText::new(&self.status).small().color(theme::MUTED));
                    });
                });
        }

        fn action_with_detail(
            &mut self,
            ui: &mut egui::Ui,
            label: &str,
            tip: &str,
            enabled: bool,
            kind: JobKind,
        ) {
            let resp = ui.add_enabled(enabled, egui::Button::new(label));
            if resp.hovered() {
                self.note_hover(label, tip);
            }
            if resp.clicked() {
                self.start_job(kind);
            }
        }

        fn draw_globals(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Visão global");
            if let Some(err) = &self.view.load_error {
                ui.colored_label(theme::BAD, format!("Scorecard: {err}"));
                ui.add_space(4.0);
            }
            ui.horizontal_wrapped(|ui| {
                if meter_card(
                    ui,
                    "ACQ",
                    format!("{:.2}", self.view.quotient),
                    score_color(self.view.quotient),
                ) {
                    self.note_hover("ACQ", global_tooltip("acq"));
                }
                if meter_card(
                    ui,
                    "Cobertura",
                    format!("{:.2}", self.view.real_coverage),
                    theme::BRAND,
                ) {
                    self.note_hover("Cobertura", global_tooltip("coverage"));
                }
                if meter_card(
                    ui,
                    "Hot-path B",
                    fmt_compact(self.view.hot_path_bytes),
                    if self.view.hot_path_bytes == 0.0 {
                        theme::OK
                    } else {
                        theme::WARN
                    },
                ) {
                    self.note_hover("Hot-path B", global_tooltip("hot_path"));
                }
                if meter_card(
                    ui,
                    "Dimensões",
                    format!("{}", self.view.dimensions.len()),
                    theme::ACCENT,
                ) {
                    self.note_hover("Dimensões", global_tooltip("dims"));
                }
            });
        }

        fn draw_dimensions(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Inteligência cognitiva (T01–T12)");
            if self.view.dimensions.is_empty() {
                egui::Frame::none()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::WARN))
                    .inner_margin(12.0)
                    .rounding(4.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Nenhuma dimensão no scorecard.")
                                .strong()
                                .color(theme::WARN),
                        );
                        ui.label(
                            RichText::new("Use “Rodar auditoria cognitiva” na barra superior.")
                                .small()
                                .color(theme::MUTED),
                        );
                    });
                return;
            }
            let selected = self.detail.ficha_id().map(str::to_string);
            let mut hovered: Option<(String, String)> = None;
            let mut clicked: Option<DimFicha> = None;
            egui::Grid::new("dims")
                .num_columns(4)
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    for (i, d) in self.view.dimensions.iter().enumerate() {
                        let tip = dimension_tooltip(&d.id);
                        let mut body = format!("{:.1}\n{}", d.score, d.status);
                        if let Some((k, v)) = d.metrics.first() {
                            body.push_str(&format!("\n{k}={}", fmt_compact(*v)));
                        }
                        let is_sel = selected.as_deref() == Some(d.id.as_str());
                        let (hov, clk) = dim_card(
                            ui,
                            &d.id,
                            &d.name,
                            &body,
                            score_color(d.score),
                            is_sel,
                            d.score,
                        );
                        if hov {
                            hovered = Some((d.id.clone(), tip.to_string()));
                        }
                        if clk {
                            clicked = Some(dimension_ficha(&d.id));
                        }
                        if (i + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
            if let Some(f) = clicked {
                self.detail = DetailPane::Ficha(f);
            } else if let Some((title, body)) = hovered {
                self.note_hover(title, body);
            }
        }

        fn draw_throughput(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Vazão (cycles/s · eq-t/s · SOP/s · TOP/s)");
            let Some(tp) = self.view.throughput.clone() else {
                ui.label(
                    RichText::new("Sem throughput. Use “Benchmark de vazão”.").color(theme::MUTED),
                );
                return;
            };
            ui.horizontal(|ui| {
                if meter_card(
                    ui,
                    "Heap MB",
                    format!("{:.2}", tp.peak_heap_mb),
                    theme::MUTED,
                ) {
                    self.note_hover(
                        "Heap MB",
                        "Pico de heap do processo durante o bench de vazão.",
                    );
                }
                if meter_card(ui, "RSS MB", format!("{:.2}", tp.peak_rss_mb), theme::MUTED) {
                    self.note_hover("RSS MB", "Pico de memória residente (RSS) do processo.");
                }
                if meter_card(
                    ui,
                    "Allocs",
                    format!("{}", tp.hot_path_allocations),
                    if tp.hot_path_allocations == 0 {
                        theme::OK
                    } else {
                        theme::BAD
                    },
                ) {
                    self.note_hover("Allocs", throughput_tooltip("allocs"));
                }
            });
            for r in &tp.regimes {
                let regime_key = match r.name.to_lowercase().as_str() {
                    "basal" => "basal",
                    "vigilante" => "vigilante",
                    "critico" | "crítico" => "critico",
                    _ => "basal",
                };
                egui::Frame::none()
                    .fill(theme::PANEL)
                    .stroke(Stroke::new(1.0_f32, theme::STROKE))
                    .inner_margin(10.0)
                    .rounding(4.0)
                    .show(ui, |ui| {
                        let title_resp = ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} ({} passos)", r.name, r.steps))
                                    .strong()
                                    .color(theme::TEXT),
                            );
                        });
                        if title_resp.response.hovered() {
                            self.note_hover(
                                format!("Regime {}", r.name),
                                throughput_tooltip(regime_key),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            if meter_card(ui, "p50 µs", format!("{:.1}", r.p50_us), theme::BRAND) {
                                self.note_hover("p50 µs", throughput_tooltip("p50"));
                            }
                            if meter_card(ui, "p99.9 µs", format!("{:.1}", r.p99_9_us), theme::WARN)
                            {
                                self.note_hover("p99.9 µs", throughput_tooltip("p99.9"));
                            }
                            if meter_card(
                                ui,
                                "cycles/s",
                                fmt_compact(r.cycles_per_s),
                                theme::ACCENT,
                            ) {
                                self.note_hover("cycles/s", throughput_tooltip("cycles"));
                            }
                            if meter_card(ui, "eq-t/s", fmt_compact(r.eq_t_per_s), theme::ACCENT) {
                                self.note_hover("eq-t/s", throughput_tooltip("eq-t"));
                            }
                            if meter_card(ui, "SOP/s", fmt_compact(r.sop_per_s), theme::ACCENT) {
                                self.note_hover("SOP/s", throughput_tooltip("sop"));
                            }
                            if meter_card(ui, "TOP/s", fmt_compact(r.tops()), theme::ACCENT) {
                                self.note_hover("TOP/s", throughput_tooltip("tops"));
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        }

        fn draw_host(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Máquina hospedeira");
            let Some(h) = self.host.clone() else {
                ui.label(
                    RichText::new(
                        "Ainda sem sonda. Use “Sondar hardware” (ou recarregue se já houver JSON).",
                    )
                    .color(theme::MUTED),
                );
                return;
            };
            ui.horizontal_wrapped(|ui| {
                if meter_card(
                    ui,
                    "CPU lógico",
                    format!("{}", h.logical_cpus),
                    theme::ACCENT,
                ) {
                    self.note_hover("CPU lógico", "Núcleos lógicos detectados (SMT incluso).");
                }
                if meter_card(
                    ui,
                    "CPU físico",
                    format!("{}", h.physical_cpus),
                    theme::ACCENT,
                ) {
                    self.note_hover(
                        "CPU físico",
                        "Núcleos físicos (CIM Win32_Processor / topologia).",
                    );
                }
                if meter_card(
                    ui,
                    "RAM MB",
                    h.ram_total_mb
                        .map(fmt_compact)
                        .unwrap_or_else(|| "?".into()),
                    theme::MUTED,
                ) {
                    self.note_hover("RAM MB", "Memória física total (CIM).");
                }
                if meter_card(
                    ui,
                    "Livre MB",
                    h.ram_available_mb
                        .map(fmt_compact)
                        .unwrap_or_else(|| "?".into()),
                    theme::MUTED,
                ) {
                    self.note_hover("Livre MB", "Memória física disponível.");
                }
            });
            ui.label(
                RichText::new(format!(
                    "arch={} os={} · affinity={} jit={} hw-entropy={} · reflex={:?} · L1d={} B",
                    h.arch,
                    h.os,
                    on_off(h.affinity_feature),
                    on_off(h.jit_feature),
                    on_off(h.hw_entropy_feature),
                    h.reflex_core,
                    h.l1d_budget_bytes
                ))
                .small()
                .color(theme::MUTED),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new("Gaps detectados")
                    .strong()
                    .size(13.0)
                    .color(theme::TEXT),
            );
            if h.gaps.is_empty() {
                ui.colored_label(theme::OK, "Nenhum gap crítico nesta sonda.");
            } else {
                for g in &h.gaps {
                    let color = match g.severity.as_str() {
                        "crítico" => theme::BAD,
                        "aviso" => theme::WARN,
                        _ => theme::MUTED,
                    };
                    let resp = egui::Frame::none()
                        .fill(theme::PANEL)
                        .stroke(Stroke::new(1.0_f32, color))
                        .inner_margin(8.0)
                        .rounding(3.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("[{}] {} — {}", g.severity, g.code, g.title))
                                    .color(color)
                                    .strong(),
                            );
                            ui.label(RichText::new(&g.detail).small().color(theme::TEXT));
                            ui.label(
                                RichText::new(format!("→ {}", g.remediation))
                                    .small()
                                    .italics()
                                    .color(theme::MUTED),
                            );
                        });
                    if resp.response.hovered() {
                        self.note_hover(
                            format!("Gap {}", g.code),
                            format!("{}\n\n→ {}", g.detail, g.remediation),
                        );
                    }
                }
            }
        }
    }

    fn on_off(v: bool) -> &'static str {
        if v {
            "on"
        } else {
            "off"
        }
    }

    /// Sparkline com preenchimento sob a curva.
    fn sparkline(ui: &mut egui::Ui, label: &str, ys: &[f32], color: Color32) {
        if ys.is_empty() {
            return;
        }
        let last = *ys.last().unwrap_or(&0.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).small().color(theme::MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(fmt_compact(last as f64))
                        .small()
                        .strong()
                        .color(color),
                );
            });
        });
        let height = 56.0;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(40.0), height),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, theme::BG);
        painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE));

        let min = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let max = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let span = (max - min).abs().max(1e-6);
        let n = ys.len();
        let w = rect.width().max(1.0);
        let h = rect.height().max(1.0);
        let pad = 3.0;

        let mut points: Vec<egui::Pos2> = Vec::with_capacity(n);
        for (i, y) in ys.iter().enumerate() {
            let x = rect.left()
                + pad
                + (i as f32 / (n.saturating_sub(1).max(1) as f32)) * (w - 2.0 * pad);
            let yn = (*y - min) / span;
            let py = rect.bottom() - pad - yn * (h - 2.0 * pad);
            points.push(egui::Pos2::new(x, py));
        }
        if points.len() >= 2 {
            let mut fill = points.clone();
            if let (Some(first), Some(last)) = (fill.first().copied(), fill.last().copied()) {
                fill.push(egui::Pos2::new(last.x, rect.bottom() - pad));
                fill.push(egui::Pos2::new(first.x, rect.bottom() - pad));
                let mut fill_color = color;
                fill_color = Color32::from_rgba_unmultiplied(
                    fill_color.r(),
                    fill_color.g(),
                    fill_color.b(),
                    40,
                );
                painter.add(egui::Shape::convex_polygon(fill, fill_color, Stroke::NONE));
            }
            painter.add(egui::Shape::line(points, Stroke::new(1.8_f32, color)));
        }
        ui.add_space(4.0);
    }

    fn score_color(score: f64) -> Color32 {
        if score >= 99.0 {
            theme::OK
        } else if score >= 90.0 {
            theme::WARN
        } else {
            theme::BAD
        }
    }

    /// Card de medidor; `true` se hovered.
    fn meter_card(ui: &mut egui::Ui, title: &str, value: String, accent: Color32) -> bool {
        let size = Vec2::new(122.0, 74.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
        let fill = if resp.hovered() {
            theme::CARD_HOVER
        } else {
            theme::CARD
        };
        ui.painter().rect_filled(rect, 4.0, fill);
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(1.0_f32, theme::STROKE));
        // Faixa de acento à esquerda.
        let bar = egui::Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height()));
        ui.painter().rect_filled(bar, 0.0, accent);
        ui.painter().text(
            rect.left_top() + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            title,
            FontId::proportional(11.0),
            theme::MUTED,
        );
        ui.painter().text(
            rect.left_top() + Vec2::new(10.0, 30.0),
            egui::Align2::LEFT_TOP,
            value,
            FontId::proportional(18.0),
            theme::TEXT,
        );
        resp.hovered()
    }

    /// Card T0x; `(hovered, clicked)`.
    fn dim_card(
        ui: &mut egui::Ui,
        id: &str,
        name: &str,
        body: &str,
        accent: Color32,
        selected: bool,
        score: f64,
    ) -> (bool, bool) {
        let size = Vec2::new(204.0, 100.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let fill = if selected {
            theme::CARD_SEL
        } else if resp.hovered() {
            theme::CARD_HOVER
        } else {
            theme::CARD
        };
        let stroke = if selected {
            Stroke::new(2.0_f32, theme::BRAND)
        } else {
            Stroke::new(1.0_f32, theme::STROKE)
        };
        ui.painter().rect_filled(rect, 4.0, fill);
        ui.painter().rect_stroke(rect, 4.0, stroke);
        // Barra de score na base.
        let frac = (score / 100.0).clamp(0.0, 1.0) as f32;
        let score_bar = egui::Rect::from_min_max(
            egui::Pos2::new(rect.left(), rect.bottom() - 3.0),
            egui::Pos2::new(rect.left() + rect.width() * frac, rect.bottom()),
        );
        ui.painter().rect_filled(score_bar, 0.0, accent);

        ui.painter().text(
            rect.left_top() + Vec2::new(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            id,
            FontId::proportional(14.0),
            accent,
        );
        ui.painter().text(
            rect.left_top() + Vec2::new(10.0, 28.0),
            egui::Align2::LEFT_TOP,
            truncate(name, 28),
            FontId::proportional(11.0),
            theme::MUTED,
        );
        ui.painter().text(
            rect.left_top() + Vec2::new(10.0, 50.0),
            egui::Align2::LEFT_TOP,
            body,
            FontId::proportional(12.0),
            theme::TEXT,
        );
        (resp.hovered(), resp.clicked())
    }

    fn truncate(s: &str, max: usize) -> String {
        let mut t: String = s.chars().take(max).collect();
        if s.chars().count() > max {
            t.push('…');
        }
        t
    }

    pub fn run() -> eframe::Result<()> {
        let mut path_arg = None;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--help" | "-h" => {
                    println!(
                        "aether-dashboard — hub de métricas AETHER (Windows)\n\
                         Uso: aether-dashboard [--scorecard caminho.json]\n\
                         Fonte padrão: target/aether_scorecard.json"
                    );
                    std::process::exit(0);
                }
                "--scorecard" => {
                    i += 1;
                    path_arg = args.get(i).cloned();
                }
                other => {
                    eprintln!("flag desconhecida: {other} (use --help)");
                    std::process::exit(2);
                }
            }
            i += 1;
        }

        let mut app = DashboardApp {
            view: ScorecardView::default(),
            status: String::new(),
            path_arg,
            jobs: JobRunner::new(),
            host: None,
            detail: DetailPane::Idle,
            hover_scratch: None,
            confirm_daemon: false,
            last_job_done: true,
            theme_ready: false,
        };
        app.reload();
        app.try_load_host();

        let title = format!("AETHER — Hub de métricas v{}", env!("CARGO_PKG_VERSION"));
        let native = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1320.0, 860.0])
                .with_min_inner_size([960.0, 640.0])
                .with_title(title),
            ..Default::default()
        };
        eframe::run_native(
            "AETHER Dashboard",
            native,
            Box::new(|_cc| Ok(Box::new(app))),
        )
    }
}
