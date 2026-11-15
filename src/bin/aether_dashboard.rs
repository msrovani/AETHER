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
    use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
    use eframe::App;

    use aether_engine::dashboard::{
        dimension_ficha, dimension_tooltip, global_tooltip, glossary_entries, load_scorecard,
        probe_and_save, throughput_tooltip, DimFicha, HostReport, JobKind, JobRunner,
        ScorecardView,
    };

    /// Conteúdo da coluna direita (explicação).
    #[derive(Clone, Debug)]
    enum DetailPane {
        /// Nada sob o mouse: glossário curto.
        Idle,
        /// Hover sobre medidor/botão.
        Hover { title: String, body: String },
        /// Clique em T0x: ficha fixa até outro clique/hover.
        Ficha(DimFicha),
    }

    /// App do hub.
    pub struct DashboardApp {
        view: ScorecardView,
        status: String,
        path_arg: Option<String>,
        jobs: JobRunner,
        host: Option<HostReport>,
        detail: DetailPane,
        /// Preenchido durante o frame se algo estiver sob o mouse.
        hover_scratch: Option<(String, String)>,
        confirm_daemon: bool,
        last_job_done: bool,
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
                self.detail = DetailPane::Hover { title, body };
            } else if !matches!(self.detail, DetailPane::Ficha(_)) {
                self.detail = DetailPane::Idle;
            }
        }

        fn poll_jobs(&mut self) {
            let snap = self.jobs.snapshot();
            if snap.phase == aether_engine::dashboard::JobPhase::Running {
                self.status = format!(
                    "[{}] {}",
                    snap.kind.map(|k| k.label()).unwrap_or("job"),
                    snap.log
                );
                return;
            }
            if !self.last_job_done
                && matches!(
                    snap.phase,
                    aether_engine::dashboard::JobPhase::Done
                        | aether_engine::dashboard::JobPhase::Failed
                )
            {
                self.last_job_done = true;
                self.status = format!("[{}] {}", snap.phase.label(), snap.log);
                if snap.kind == Some(JobKind::HostProbe)
                    && snap.phase == aether_engine::dashboard::JobPhase::Done
                {
                    if let Ok(r) = probe_and_save(Some(&self.view)) {
                        self.host = Some(r);
                    }
                }
                if matches!(snap.kind, Some(JobKind::Audit) | Some(JobKind::Bench))
                    && snap.phase == aether_engine::dashboard::JobPhase::Done
                {
                    self.reload();
                }
            }
        }
    }

    impl App for DashboardApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.poll_jobs();
            self.hover_scratch = None;
            let demo_live = self
                .jobs
                .trace
                .lock()
                .map(|t| t.running)
                .unwrap_or(false);
            if self.jobs.snapshot().phase == aether_engine::dashboard::JobPhase::Running
                || demo_live
            {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }

            egui::TopBottomPanel::top("top").show(ctx, |ui| {
                self.draw_top_bar(ui);
            });

            egui::SidePanel::right("detail")
                .default_width(360.0)
                .min_width(280.0)
                .resizable(true)
                .show(ctx, |ui| {
                    self.draw_detail_pane(ui);
                });

            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_globals(ui);
                    ui.add_space(12.0);
                    self.draw_dimensions(ui);
                    ui.add_space(12.0);
                    self.draw_throughput(ui);
                    ui.add_space(12.0);
                    self.draw_host(ui);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Passe o mouse nos medidores → coluna direita · clique em T0x = ficha.",
                        )
                        .small()
                        .color(Color32::from_rgb(140, 150, 160)),
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
                            if ui.button("Confirmar e lançar").clicked() {
                                self.confirm_daemon = false;
                                self.start_job(JobKind::DaemonDemo);
                            }
                        });
                    });
            }
        }
    }

    impl DashboardApp {
        fn draw_detail_pane(&mut self, ui: &mut egui::Ui) {
            ui.heading(RichText::new("Detalhe").strong().size(18.0));
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                match &self.detail {
                    DetailPane::Idle => {
                        ui.label(
                            RichText::new(
                                "Passe o mouse sobre um medidor · clique T0x = ficha · \
                                 demos organism/daemon desenham gráficos abaixo.",
                            )
                            .small()
                            .color(Color32::from_rgb(160, 170, 180)),
                        );
                        ui.add_space(8.0);
                        ui.label(RichText::new("Glossário").strong());
                        for (k, v) in glossary_entries() {
                            ui.label(RichText::new(*k).strong().small());
                            ui.label(RichText::new(*v).small());
                            ui.add_space(4.0);
                        }
                    }
                    DetailPane::Hover { title, body } => {
                        ui.label(RichText::new(title).strong().size(15.0));
                        ui.add_space(6.0);
                        for para in body.split('\n') {
                            if para.is_empty() {
                                ui.add_space(4.0);
                            } else {
                                ui.label(RichText::new(para).size(13.0));
                            }
                        }
                    }
                    DetailPane::Ficha(ficha) => {
                        let ficha = *ficha;
                        ui.label(
                            RichText::new(format!("{} — {}", ficha.id, ficha.title))
                                .strong()
                                .size(15.0),
                        );
                        ui.add_space(6.0);
                        for para in ficha.tooltip.split('\n') {
                            if para.is_empty() {
                                ui.add_space(4.0);
                            } else {
                                ui.label(RichText::new(para).small());
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new("Como interpretar").strong());
                        ui.label(ficha.interpret);
                        ui.add_space(4.0);
                        ui.label(RichText::new("Se baixo, o que falha").strong());
                        ui.label(ficha.if_low);
                        ui.add_space(4.0);
                        ui.label(RichText::new("Proveniência").strong());
                        ui.label(RichText::new(ficha.provenance).italics().small());
                        ui.add_space(8.0);
                        if ui.button("Limpar ficha").clicked() {
                            self.detail = DetailPane::Idle;
                        }
                    }
                }

                ui.add_space(10.0);
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

            ui.label(RichText::new("Gráficos do demo").strong().size(15.0));
            if trace.title.is_empty() && trace.samples.is_empty() {
                ui.label(
                    RichText::new(
                        "Ainda sem série. Clique em “Demo organismo” ou “Demo daemon”.",
                    )
                    .small()
                    .color(Color32::GRAY),
                );
                return;
            }

            let status = if trace.running { "ao vivo" } else { "última corrida" };
            ui.label(
                RichText::new(format!("{} · {} · {} pts", trace.title, status, trace.samples.len()))
                    .small()
                    .color(Color32::from_rgb(120, 160, 180)),
            );
            ui.add_space(4.0);

            if !trace.samples.is_empty() {
                let r = trace.series(|s| s.r);
                let e = trace.series(|s| s.energy);
                let s = trace.series(|s| s.entropy);
                let c = trace.series(|s| s.chatter);
                let cost = trace.series(|s| s.cost_us);

                // Legenda: organismo = R/H/S/chatter; daemon = R(LAN)/F/S
                sparkline(ui, "R / R(LAN)", &r, Color32::from_rgb(60, 180, 140));
                sparkline(ui, "H / F (energia ou surpresa)", &e, Color32::from_rgb(200, 140, 70));
                sparkline(ui, "S (entropia)", &s, Color32::from_rgb(90, 140, 200));
                if c.iter().any(|v| *v > 0.0) {
                    sparkline(ui, "chatter ‖Δu‖", &c, Color32::from_rgb(180, 90, 120));
                }
                if cost.iter().any(|v| *v > 0.0) {
                    sparkline(ui, "custo quadro (µs)", &cost, Color32::from_rgb(140, 140, 160));
                }
            }

            if !trace.energy_trace.is_empty() {
                ui.add_space(4.0);
                sparkline(
                    ui,
                    "H(t) último relaxamento",
                    &trace.energy_trace,
                    Color32::from_rgb(100, 200, 160),
                );
            }
        }

        fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("AETHER").strong().size(22.0));
                ui.label(
                    RichText::new("hub · métricas + testes")
                        .color(Color32::from_rgb(120, 140, 150)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Recarregar").clicked() {
                        self.reload();
                    }
                    if self.jobs.is_busy() && ui.button("Cancelar job").clicked() {
                        self.jobs.request_cancel();
                    }
                });
            });
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
                    "Lança aether-organism --duration 15s --plain.",
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
                aether_engine::dashboard::JobPhase::Idle => Color32::GRAY,
                aether_engine::dashboard::JobPhase::Running => Color32::from_rgb(80, 160, 200),
                aether_engine::dashboard::JobPhase::Done => Color32::from_rgb(40, 160, 100),
                aether_engine::dashboard::JobPhase::Failed => Color32::from_rgb(200, 80, 60),
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("job: {}", snap.phase.label()))
                        .strong()
                        .color(phase_color),
                );
                ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
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
            ui.label(RichText::new("Visão global").strong().size(16.0));
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
                    Color32::from_rgb(40, 140, 130),
                ) {
                    self.note_hover("Cobertura", global_tooltip("coverage"));
                }
                if meter_card(
                    ui,
                    "Hot-path B",
                    format!("{:.0}", self.view.hot_path_bytes),
                    if self.view.hot_path_bytes == 0.0 {
                        Color32::from_rgb(40, 140, 90)
                    } else {
                        Color32::from_rgb(180, 90, 50)
                    },
                ) {
                    self.note_hover("Hot-path B", global_tooltip("hot_path"));
                }
                if meter_card(
                    ui,
                    "Dimensões",
                    format!("{}", self.view.dimensions.len()),
                    Color32::from_rgb(70, 110, 150),
                ) {
                    self.note_hover("Dimensões", global_tooltip("dims"));
                }
            });
        }

        fn draw_dimensions(&mut self, ui: &mut egui::Ui) {
            ui.label(
                RichText::new("Inteligência cognitiva (T01–T12)")
                    .strong()
                    .size(16.0),
            );
            if self.view.dimensions.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(180, 100, 60),
                    "Nenhuma dimensão. Use “Rodar auditoria cognitiva”.",
                );
                return;
            }
            // Coleta hover/click sem emprestar self no loop.
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
                            body.push_str(&format!("\n{k}={v:.3}"));
                        }
                        let (hov, clk) = dim_card(ui, &d.id, &d.name, &body, score_color(d.score));
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
            ui.label(
                RichText::new("Vazão (cycles/s · eq-t/s · SOP/s · TOP/s)")
                    .strong()
                    .size(16.0),
            );
            let Some(tp) = self.view.throughput.clone() else {
                ui.label(
                    RichText::new("Sem throughput. Use “Benchmark de vazão”.").color(Color32::GRAY),
                );
                return;
            };
            ui.horizontal(|ui| {
                if meter_card(
                    ui,
                    "Heap MB",
                    format!("{:.2}", tp.peak_heap_mb),
                    Color32::from_rgb(80, 100, 120),
                ) {
                    self.note_hover(
                        "Heap MB",
                        "Pico de heap do processo durante o bench de vazão.",
                    );
                }
                if meter_card(
                    ui,
                    "RSS MB",
                    format!("{:.2}", tp.peak_rss_mb),
                    Color32::from_rgb(80, 100, 120),
                ) {
                    self.note_hover("RSS MB", "Pico de memória residente (RSS) do processo.");
                }
                if meter_card(
                    ui,
                    "Allocs",
                    format!("{}", tp.hot_path_allocations),
                    if tp.hot_path_allocations == 0 {
                        Color32::from_rgb(40, 140, 90)
                    } else {
                        Color32::from_rgb(180, 90, 50)
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
                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(28, 34, 40))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(55, 70, 80)))
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        let title_resp = ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} ({} passos)", r.name, r.steps))
                                    .strong()
                                    .color(Color32::from_rgb(200, 210, 215)),
                            );
                        });
                        if title_resp.response.hovered() {
                            self.note_hover(
                                format!("Regime {}", r.name),
                                throughput_tooltip(regime_key),
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            if meter_card(
                                ui,
                                "p50 µs",
                                format!("{:.1}", r.p50_us),
                                Color32::from_rgb(50, 130, 120),
                            ) {
                                self.note_hover("p50 µs", throughput_tooltip("p50"));
                            }
                            if meter_card(
                                ui,
                                "p99.9 µs",
                                format!("{:.1}", r.p99_9_us),
                                Color32::from_rgb(150, 100, 60),
                            ) {
                                self.note_hover("p99.9 µs", throughput_tooltip("p99.9"));
                            }
                            if meter_card(
                                ui,
                                "cycles/s",
                                fmt_compact(r.cycles_per_s),
                                Color32::from_rgb(60, 120, 150),
                            ) {
                                self.note_hover("cycles/s", throughput_tooltip("cycles"));
                            }
                            if meter_card(
                                ui,
                                "eq-t/s",
                                fmt_compact(r.eq_t_per_s),
                                Color32::from_rgb(70, 110, 160),
                            ) {
                                self.note_hover("eq-t/s", throughput_tooltip("eq-t"));
                            }
                            if meter_card(
                                ui,
                                "SOP/s",
                                fmt_compact(r.sop_per_s),
                                Color32::from_rgb(70, 100, 140),
                            ) {
                                self.note_hover("SOP/s", throughput_tooltip("sop"));
                            }
                            if meter_card(
                                ui,
                                "TOP/s",
                                fmt_compact(r.tops()),
                                Color32::from_rgb(90, 90, 140),
                            ) {
                                self.note_hover("TOP/s", throughput_tooltip("tops"));
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        }

        fn draw_host(&mut self, ui: &mut egui::Ui) {
            ui.label(RichText::new("Máquina hospedeira").strong().size(16.0));
            let Some(h) = self.host.clone() else {
                ui.label(
                    RichText::new("Ainda sem sonda. Use “Sondar hardware”.").color(Color32::GRAY),
                );
                return;
            };
            ui.horizontal_wrapped(|ui| {
                if meter_card(
                    ui,
                    "CPU lógico",
                    format!("{}", h.logical_cpus),
                    Color32::from_rgb(70, 120, 140),
                ) {
                    self.note_hover("CPU lógico", "Núcleos lógicos detectados (SMT incluso).");
                }
                if meter_card(
                    ui,
                    "CPU físico",
                    format!("{}", h.physical_cpus),
                    Color32::from_rgb(70, 120, 140),
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
                        .map(|v| format!("{v:.0}"))
                        .unwrap_or_else(|| "?".into()),
                    Color32::from_rgb(90, 110, 130),
                ) {
                    self.note_hover("RAM MB", "Memória física total (CIM).");
                }
                if meter_card(
                    ui,
                    "Livre MB",
                    h.ram_available_mb
                        .map(|v| format!("{v:.0}"))
                        .unwrap_or_else(|| "?".into()),
                    Color32::from_rgb(90, 110, 130),
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
                .color(Color32::from_rgb(150, 160, 170)),
            );
            ui.add_space(6.0);
            ui.label(RichText::new("Gaps detectados").strong().size(14.0));
            if h.gaps.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(40, 150, 100),
                    "Nenhum gap crítico nesta sonda.",
                );
            } else {
                for g in &h.gaps {
                    let color = match g.severity.as_str() {
                        "crítico" => Color32::from_rgb(200, 80, 60),
                        "aviso" => Color32::from_rgb(180, 140, 50),
                        _ => Color32::from_rgb(120, 140, 160),
                    };
                    let resp = ui.group(|ui| {
                        ui.label(
                            RichText::new(format!("[{}] {} — {}", g.severity, g.code, g.title))
                                .color(color)
                                .strong(),
                        );
                        ui.label(RichText::new(&g.detail).small());
                        ui.label(
                            RichText::new(format!("→ {}", g.remediation))
                                .small()
                                .italics(),
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

    /// Sparkline simples (sem crate plot).
    fn sparkline(ui: &mut egui::Ui, label: &str, ys: &[f32], color: Color32) {
        if ys.is_empty() {
            return;
        }
        let last = *ys.last().unwrap_or(&0.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(fmt_compact(last as f64))
                        .small()
                        .strong()
                        .color(color),
                );
            });
        });
        let height = 52.0;
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().max(40.0), height),
            Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3.0, Color32::from_rgb(20, 26, 32));
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0_f32, Color32::from_rgb(50, 60, 70)),
        );

        let min = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let max = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let span = (max - min).abs().max(1e-6);
        let n = ys.len();
        let w = rect.width().max(1.0);
        let h = rect.height().max(1.0);
        let pad = 3.0;

        let mut points: Vec<egui::Pos2> = Vec::with_capacity(n);
        for (i, y) in ys.iter().enumerate() {
            let x = rect.left() + pad + (i as f32 / (n.saturating_sub(1).max(1) as f32)) * (w - 2.0 * pad);
            let yn = (*y - min) / span;
            let py = rect.bottom() - pad - yn * (h - 2.0 * pad);
            points.push(egui::Pos2::new(x, py));
        }
        if points.len() >= 2 {
            painter.add(egui::Shape::line(
                points,
                Stroke::new(1.6_f32, color),
            ));
        }
        ui.add_space(4.0);
    }

    /// Formata com **4 dígitos** + sufixo `k`/`kk`/`kkk` (sem `eN`).
    /// Exemplos: `1,586` · `15,86` · `158,6` · `87k` · `235kk` · `3,180kkk` · `0,003182`.
    fn fmt_compact(n: f64) -> String {
        if !n.is_finite() {
            return "—".into();
        }
        let sign = if n.is_sign_negative() && n != 0.0 {
            "-"
        } else {
            ""
        };
        let mut x = n.abs();
        if x == 0.0 {
            return format!("{sign}0");
        }

        // Escala milhar → mantissa em [1, 1000) + sufixo k/kk/kkk/kkkk.
        const SUFFIXES: [&str; 5] = ["", "k", "kk", "kkk", "kkkk"];
        let mut scale = 0usize;
        while x >= 1000.0 && scale + 1 < SUFFIXES.len() {
            x /= 1000.0;
            scale += 1;
        }
        let suffix = SUFFIXES[scale];

        if x < 1.0 {
            // Frações: até 4 dígitos significativos (ex.: 0,003182).
            return format!("{sign}{}", four_digits_lt1(x));
        }

        format!("{sign}{}{suffix}", four_digits_ge1(x))
    }

    /// Mantissa ∈ [1, 1000): sempre 4 dígitos, vírgula decimal, zeros finais removidos.
    fn four_digits_ge1(x: f64) -> String {
        let raw = if x < 10.0 {
            format!("{x:.3}") // 1,586
        } else if x < 100.0 {
            format!("{x:.2}") // 15,86
        } else {
            format!("{x:.1}") // 158,6
        };
        trim_comma_zeros(&raw.replace('.', ","))
    }

    /// Mantissa ∈ (0, 1): 4 dígitos significativos com vírgula.
    fn four_digits_lt1(x: f64) -> String {
        // Casas = 3 + ordem (ex.: 0,003182 → 6 casas, 4 sig).
        let exp = x.log10().floor() as i32; // -3 for 0.003…
        let frac_digits = ((-exp) + 3).clamp(3, 9) as usize;
        let raw = format!("{x:.frac_digits$}");
        trim_comma_zeros(&raw.replace('.', ","))
    }

    fn trim_comma_zeros(s: &str) -> String {
        if !s.contains(',') {
            return s.to_string();
        }
        let t = s.trim_end_matches('0').trim_end_matches(',');
        if t.is_empty() {
            "0".into()
        } else {
            t.to_string()
        }
    }

    fn score_color(score: f64) -> Color32 {
        if score >= 99.0 {
            Color32::from_rgb(35, 145, 100)
        } else if score >= 90.0 {
            Color32::from_rgb(160, 140, 50)
        } else {
            Color32::from_rgb(180, 80, 60)
        }
    }

    /// Card de medidor; retorna `true` se o mouse está sobre ele.
    fn meter_card(ui: &mut egui::Ui, title: &str, value: String, accent: Color32) -> bool {
        let size = Vec2::new(118.0, 72.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
        let fill = Color32::from_rgb(24, 30, 36);
        ui.painter().rect_filled(rect, 4.0, fill);
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(1.5_f32, accent));
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            title,
            egui::FontId::proportional(12.0),
            Color32::from_rgb(160, 170, 175),
        );
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 30.0),
            egui::Align2::LEFT_TOP,
            value,
            egui::FontId::proportional(18.0),
            Color32::from_rgb(230, 235, 238),
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
    ) -> (bool, bool) {
        let size = Vec2::new(200.0, 96.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        ui.painter()
            .rect_filled(rect, 4.0, Color32::from_rgb(22, 28, 34));
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(1.5_f32, accent));
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            id,
            egui::FontId::proportional(14.0),
            accent,
        );
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 26.0),
            egui::Align2::LEFT_TOP,
            truncate(name, 28),
            egui::FontId::proportional(11.0),
            Color32::from_rgb(170, 180, 185),
        );
        ui.painter().text(
            rect.left_top() + Vec2::new(8.0, 48.0),
            egui::Align2::LEFT_TOP,
            body,
            egui::FontId::proportional(12.0),
            Color32::from_rgb(220, 225, 228),
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
        };
        app.reload();

        let native = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 820.0])
                .with_title("AETHER — Hub de métricas"),
            ..Default::default()
        };
        eframe::run_native(
            "AETHER Dashboard",
            native,
            Box::new(|_cc| Ok(Box::new(app))),
        )
    }
}
