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

    /// App do hub.
    pub struct DashboardApp {
        view: ScorecardView,
        status: String,
        path_arg: Option<String>,
        jobs: JobRunner,
        host: Option<HostReport>,
        selected: Option<DimFicha>,
        show_help: bool,
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
                if matches!(
                    snap.kind,
                    Some(JobKind::Audit) | Some(JobKind::Bench)
                ) && snap.phase == aether_engine::dashboard::JobPhase::Done
                {
                    self.reload();
                }
            }
        }
    }

    impl App for DashboardApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.poll_jobs();
            if self.jobs.snapshot().phase == aether_engine::dashboard::JobPhase::Running {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }

            egui::TopBottomPanel::top("top").show(ctx, |ui| {
                self.draw_top_bar(ui);
            });

            if self.show_help {
                egui::SidePanel::right("help")
                    .default_width(280.0)
                    .show(ctx, |ui| {
                        ui.heading("Ajuda / glossário");
                        ui.separator();
                        for (k, v) in glossary_entries() {
                            ui.label(RichText::new(*k).strong());
                            ui.label(RichText::new(*v).small());
                            ui.add_space(6.0);
                        }
                        ui.separator();
                        ui.label(
                            RichText::new(
                                "Docs: docs/USER_GUIDE.md · docs/DASHBOARD_METRICS.md",
                            )
                            .small()
                            .color(Color32::GRAY),
                        );
                    });
            }

            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_globals(ui);
                    ui.add_space(8.0);
                    self.draw_glossary_inline(ui);
                    ui.add_space(12.0);
                    self.draw_dimensions(ui);
                    ui.add_space(12.0);
                    self.draw_throughput(ui);
                    ui.add_space(12.0);
                    self.draw_host(ui);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Hover = tooltip · clique em T0x = ficha · botões = testes práticos.",
                        )
                        .small()
                        .color(Color32::from_rgb(140.0 as u8, 150, 160)),
                    );
                });
            });

            if let Some(ficha) = self.selected {
                egui::Window::new(format!("{} — {}", ficha.id, ficha.title))
                    .collapsible(false)
                    .resizable(true)
                    .default_width(420.0)
                    .show(ctx, |ui| {
                        ui.label(RichText::new(ficha.tooltip).small());
                        ui.separator();
                        ui.label(RichText::new("Como interpretar").strong());
                        ui.label(ficha.interpret);
                        ui.add_space(4.0);
                        ui.label(RichText::new("Se baixo, o que falha").strong());
                        ui.label(ficha.if_low);
                        ui.add_space(4.0);
                        ui.label(RichText::new("Proveniência").strong());
                        ui.label(RichText::new(ficha.provenance).italics().small());
                        if ui.button("Fechar").clicked() {
                            self.selected = None;
                        }
                    });
            }

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
        fn draw_top_bar(&mut self, ui: &mut egui::Ui) {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("AETHER").strong().size(22.0));
                ui.label(
                    RichText::new("hub · métricas + testes")
                        .color(Color32::from_rgb(120, 140, 150)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Ajuda").clicked() {
                        self.show_help = !self.show_help;
                    }
                    if ui.button("Recarregar").clicked() {
                        self.reload();
                    }
                    if self.jobs.is_busy() && ui.button("Cancelar job").clicked() {
                        self.jobs.request_cancel();
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                action_btn(
                    ui,
                    "Rodar auditoria cognitiva",
                    "Regenera target/aether_scorecard.json (T01–T12 / ACQ).",
                    !self.jobs.is_busy(),
                )
                .then(|| self.start_job(JobKind::Audit));
                action_btn(
                    ui,
                    "Benchmark de vazão",
                    "Basal/Vigilante/Crítico (~5 s/regime) via throughput_tps.",
                    !self.jobs.is_busy(),
                )
                .then(|| self.start_job(JobKind::Bench));
                action_btn(
                    ui,
                    "Sondar hardware",
                    "CPU/RAM/features + gaps → target/aether_host_report.json.",
                    !self.jobs.is_busy(),
                )
                .then(|| self.start_job(JobKind::HostProbe));
                action_btn(
                    ui,
                    "Demo organismo",
                    "Lança aether-organism --duration 15s --plain.",
                    !self.jobs.is_busy(),
                )
                .then(|| self.start_job(JobKind::OrganismDemo));
                action_btn(
                    ui,
                    "Demo daemon",
                    "Lança aether-daemon curto (exige confirmação — multicast).",
                    !self.jobs.is_busy(),
                )
                .then(|| {
                    self.confirm_daemon = true;
                });
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

        fn draw_globals(&self, ui: &mut egui::Ui) {
            ui.label(RichText::new("Visão global").strong().size(16.0));
            ui.horizontal_wrapped(|ui| {
                meter_card(
                    ui,
                    "ACQ",
                    format!("{:.2}", self.view.quotient),
                    score_color(self.view.quotient),
                    global_tooltip("acq"),
                );
                meter_card(
                    ui,
                    "Cobertura",
                    format!("{:.2}", self.view.real_coverage),
                    Color32::from_rgb(40, 140, 130),
                    global_tooltip("coverage"),
                );
                meter_card(
                    ui,
                    "Hot-path B",
                    format!("{:.0}", self.view.hot_path_bytes),
                    if self.view.hot_path_bytes == 0.0 {
                        Color32::from_rgb(40, 140, 90)
                    } else {
                        Color32::from_rgb(180, 90, 50)
                    },
                    global_tooltip("hot_path"),
                );
                meter_card(
                    ui,
                    "Dimensões",
                    format!("{}", self.view.dimensions.len()),
                    Color32::from_rgb(70, 110, 150),
                    global_tooltip("dims"),
                );
            });
        }

        fn draw_glossary_inline(&self, ui: &mut egui::Ui) {
            egui::CollapsingHeader::new("O que estou vendo?")
                .default_open(false)
                .show(ui, |ui| {
                    for (k, v) in glossary_entries() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(format!("{k}:")).strong().small());
                            ui.label(RichText::new(*v).small());
                        });
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
                        if dim_card(ui, &d.id, &d.name, &body, score_color(d.score), tip) {
                            self.selected = Some(dimension_ficha(&d.id));
                        }
                        if (i + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
        }

        fn draw_throughput(&self, ui: &mut egui::Ui) {
            ui.label(
                RichText::new("Vazão (cycles/s · eq-t/s · SOP/s · TOP/s)")
                    .strong()
                    .size(16.0),
            );
            let Some(tp) = &self.view.throughput else {
                ui.label(
                    RichText::new("Sem throughput. Use “Benchmark de vazão”.")
                        .color(Color32::GRAY),
                );
                return;
            };
            ui.horizontal(|ui| {
                meter_card(
                    ui,
                    "Heap MB",
                    format!("{:.2}", tp.peak_heap_mb),
                    Color32::from_rgb(80, 100, 120),
                    "Pico de heap do processo durante o bench de vazão.",
                );
                meter_card(
                    ui,
                    "RSS MB",
                    format!("{:.2}", tp.peak_rss_mb),
                    Color32::from_rgb(80, 100, 120),
                    "Pico de memória residente (RSS) do processo.",
                );
                meter_card(
                    ui,
                    "Allocs",
                    format!("{}", tp.hot_path_allocations),
                    if tp.hot_path_allocations == 0 {
                        Color32::from_rgb(40, 140, 90)
                    } else {
                        Color32::from_rgb(180, 90, 50)
                    },
                    throughput_tooltip("allocs"),
                );
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
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} ({} passos)", r.name, r.steps))
                                    .strong()
                                    .color(Color32::from_rgb(200, 210, 215)),
                            );
                        })
                        .response
                        .on_hover_text(throughput_tooltip(regime_key));
                        ui.horizontal_wrapped(|ui| {
                            meter_card(
                                ui,
                                "p50 µs",
                                format!("{:.1}", r.p50_us),
                                Color32::from_rgb(50, 130, 120),
                                throughput_tooltip("p50"),
                            );
                            meter_card(
                                ui,
                                "p99.9 µs",
                                format!("{:.1}", r.p99_9_us),
                                Color32::from_rgb(150, 100, 60),
                                throughput_tooltip("p99.9"),
                            );
                            meter_card(
                                ui,
                                "cycles/s",
                                format!("{:.0}", r.cycles_per_s),
                                Color32::from_rgb(60, 120, 150),
                                throughput_tooltip("cycles"),
                            );
                            meter_card(
                                ui,
                                "eq-t/s",
                                format!("{:.2e}", r.eq_t_per_s),
                                Color32::from_rgb(70, 110, 160),
                                throughput_tooltip("eq-t"),
                            );
                            meter_card(
                                ui,
                                "SOP/s",
                                format!("{:.2e}", r.sop_per_s),
                                Color32::from_rgb(70, 100, 140),
                                throughput_tooltip("sop"),
                            );
                            meter_card(
                                ui,
                                "TOP/s",
                                format!("{:.3e}", r.tops()),
                                Color32::from_rgb(90, 90, 140),
                                throughput_tooltip("tops"),
                            );
                        });
                    });
                ui.add_space(6.0);
            }
        }

        fn draw_host(&self, ui: &mut egui::Ui) {
            ui.label(RichText::new("Máquina hospedeira").strong().size(16.0));
            let Some(h) = &self.host else {
                ui.label(
                    RichText::new("Ainda sem sonda. Use “Sondar hardware”.")
                        .color(Color32::GRAY),
                );
                return;
            };
            ui.horizontal_wrapped(|ui| {
                meter_card(
                    ui,
                    "CPU lógico",
                    format!("{}", h.logical_cpus),
                    Color32::from_rgb(70, 120, 140),
                    "Núcleos lógicos detectados (SMT incluso).",
                );
                meter_card(
                    ui,
                    "CPU físico",
                    format!("{}", h.physical_cpus),
                    Color32::from_rgb(70, 120, 140),
                    "Núcleos físicos (ou packages via WMIC).",
                );
                meter_card(
                    ui,
                    "RAM MB",
                    h.ram_total_mb
                        .map(|v| format!("{v:.0}"))
                        .unwrap_or_else(|| "?".into()),
                    Color32::from_rgb(90, 110, 130),
                    "Memória física total (WMIC).",
                );
                meter_card(
                    ui,
                    "Livre MB",
                    h.ram_available_mb
                        .map(|v| format!("{v:.0}"))
                        .unwrap_or_else(|| "?".into()),
                    Color32::from_rgb(90, 110, 130),
                    "Memória física disponível.",
                );
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
                    ui.group(|ui| {
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

    fn action_btn(ui: &mut egui::Ui, label: &str, tip: &str, enabled: bool) -> bool {
        let resp = ui.add_enabled(enabled, egui::Button::new(label));
        resp.clone().on_hover_text(tip);
        resp.clicked()
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

    fn meter_card(ui: &mut egui::Ui, title: &str, value: String, accent: Color32, tip: &str) {
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
        resp.on_hover_text(tip);
    }

    fn dim_card(
        ui: &mut egui::Ui,
        id: &str,
        name: &str,
        body: &str,
        accent: Color32,
        tip: &str,
    ) -> bool {
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
        resp.clone().on_hover_text(tip);
        resp.clicked()
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
            selected: None,
            show_help: false,
            confirm_daemon: false,
            last_job_done: true,
        };
        app.reload();

        let native = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1180.0, 820.0])
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
