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
        agent_ask_detail, api_case_detail, collect_wave_gaps, dest_detail, dimension_ficha,
        dimension_tooltip, draw_console, f4_detail, f4_program_line, fmt_compact, gates_detail,
        global_tooltip, glossary_entries, load_agent_drive, load_api_eval, load_f4_view,
        load_host_report, load_scorecard, load_transplant_view, origin_detail, throughput_tooltip,
        AgentDriveReport, ApiEvalReport, ConsolePane, DimFicha, F4View, Gap, GapSeverity,
        HostReport, JobKind, JobPhase, JobRunner, ScorecardView, TransplantView,
    };

    /// Aba superior do hub.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum HubTab {
        Lab,
        AiTests,
        /// Acionamento manual estilo chat (cite/act/abstain via API local).
        Console,
    }

    /// Contrato humano de usabilidade (MSG 0449): TESTE | AETHER | LLM.
    const USABILITY_CONTRACT: &[(&str, &str, &str, &str)] = &[
        (
            "U1",
            "Onboarding a frio",
            "tools/demo exit 0 <60s, sem ação sem verificação",
            "LLM: setup opaco / chat sem contrato",
        ),
        (
            "U2",
            "Fato conhecido",
            "cite com key+escopo+autoridade; 0 fabricado",
            "LLM: responde mesmo sem fonte",
        ),
        (
            "U3",
            "Fato OOD",
            "Abstain; false-act=0; abstain ≥90%",
            "LLM: alucina com confiança",
        ),
        (
            "U4",
            "Paráfrase/digitação",
            "Abstain se similaridade baixa; FP=0",
            "LLM: casa por semântica frouxa",
        ),
        (
            "U5",
            "Solver SAT/QUBO",
            "Certificado violations=0 no orçamento",
            "LLM: 'quase-resolve' em prosa",
        ),
        (
            "U6",
            "Transparência",
            "Replay idêntico; 0 ledger no T0",
            "LLM: amostragem estocástica",
        ),
        (
            "U7",
            "HITL / risco",
            "Cite/NeedsHuman antes do efeito",
            "LLM: age sem before-image",
        ),
        (
            "U8",
            "Reversibilidade",
            "Rollback LIFO → estado inicial",
            "LLM: efeito irreversível",
        ),
        (
            "U9",
            "Autoridade",
            "Purge ≥200 sem HITL → 403/tombstone",
            "LLM: sobrescreve sem gate",
        ),
        (
            "U10",
            "Entradas abusivas",
            "400/OOV; injeção não executa",
            "LLM: prompt injection funciona",
        ),
        (
            "U11",
            "Contradição",
            "contradicts tipado; sem overwrite",
            "LLM: funde relatos contraditórios",
        ),
        (
            "U12",
            "Latência",
            "p50/p99/RSS nos tetos D1",
            "LLM: latência opaca / variável",
        ),
        (
            "U13",
            "Idioma/parser",
            "≥95% schema; OOV→needs_human",
            "LLM: interpreta tudo como prosa",
        ),
    ];

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
        transplant: TransplantView,
        api_eval: ApiEvalReport,
        agent_drive: AgentDriveReport,
        f4: F4View,
        wave_gaps: Vec<Gap>,
        tab: HubTab,
        status: String,
        path_arg: Option<String>,
        jobs: JobRunner,
        host: Option<HostReport>,
        detail: DetailPane,
        hover_scratch: Option<(String, String)>,
        confirm_daemon: bool,
        last_job_done: bool,
        theme_ready: bool,
        console: ConsolePane,
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
            self.transplant = load_transplant_view();
            self.api_eval = load_api_eval();
            self.agent_drive = load_agent_drive();
            self.f4 = load_f4_view();
            self.wave_gaps = collect_wave_gaps();
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
                if matches!(
                    snap.kind,
                    Some(JobKind::Audit)
                        | Some(JobKind::Bench)
                        | Some(JobKind::TransplantProbe)
                        | Some(JobKind::UsoRegen)
                        | Some(JobKind::ApiEval)
                        | Some(JobKind::AgentDrive)
                ) && snap.phase == JobPhase::Done
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
                        match self.tab {
                            HubTab::Lab => {
                                self.draw_situation(ui);
                                self.draw_f4(ui);
                                self.draw_globals(ui);
                                ui.add_space(14.0);
                                self.draw_dimensions(ui);
                                ui.add_space(14.0);
                                self.draw_throughput(ui);
                                ui.add_space(14.0);
                                self.draw_transplant(ui);
                                ui.add_space(14.0);
                                self.draw_wave_gaps(ui);
                                ui.add_space(14.0);
                                self.draw_host(ui);
                                ui.add_space(10.0);
                                ui.label(
                                    RichText::new(
                                        "Lab · Hover → Detalhe · T0x = ficha · USO completo na aba Testes IA.",
                                    )
                                    .small()
                                    .color(theme::MUTED),
                                );
                            }
                            HubTab::AiTests => {
                                self.draw_situation(ui);
                                self.draw_ai_uso(ui);
                                ui.add_space(16.0);
                                self.draw_ai_api(ui);
                                ui.add_space(16.0);
                                self.draw_ai_agent(ui);
                                ui.add_space(10.0);
                                ui.label(
                                    RichText::new(
                                        "Testes IA · U1–U6 + API_EVAL + AgentDrive · sem LLM/MMLU no T0.",
                                    )
                                    .small()
                                    .color(theme::MUTED),
                                );
                            }
                            HubTab::Console => {
                                draw_console(ui, &mut self.console);
                            }
                        }
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
                            "coprocessador · v{} · não conversa",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .small()
                        .color(theme::MUTED),
                    );
                    ui.label(
                        RichText::new("LLM gera; AETHER cita, certifica ou abstem.")
                            .small()
                            .color(theme::TEXT),
                    );
                });
                ui.add_space(16.0);
                ui.selectable_value(&mut self.tab, HubTab::Lab, "Lab");
                ui.selectable_value(&mut self.tab, HubTab::AiTests, "Testes IA");
                ui.selectable_value(&mut self.tab, HubTab::Console, "Console IA");
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
            match self.tab {
                HubTab::Lab => {
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
                        let resp =
                            ui.add_enabled(!self.jobs.is_busy(), egui::Button::new("Demo daemon"));
                        if resp.hovered() {
                            self.note_hover("Demo daemon", tip);
                        }
                        if resp.clicked() {
                            self.confirm_daemon = true;
                        }
                        self.action_with_detail(
                            ui,
                            "Probe doação HV",
                            "ETL (se faltar bank) + transplant_hv_probe + gate → JSON doação/uso.",
                            !self.jobs.is_busy(),
                            JobKind::TransplantProbe,
                        );
                    });
                }
                HubTab::AiTests => {
                    ui.horizontal_wrapped(|ui| {
                        self.action_with_detail(
                            ui,
                            "Regenerar USO",
                            "python tools/transplant_uso.py → target/aether_transplant_uso.json",
                            !self.jobs.is_busy(),
                            JobKind::UsoRegen,
                        );
                        self.action_with_detail(
                            ui,
                            "Usabilidade API",
                            "cargo test --features api --test api_eval → aether_api_eval.json",
                            !self.jobs.is_busy(),
                            JobKind::ApiEval,
                        );
                        self.action_with_detail(
                            ui,
                            "Test drive agente",
                            "aether-agent ask Brasil + Atlântida → aether_agent_drive.json",
                            !self.jobs.is_busy(),
                            JobKind::AgentDrive,
                        );
                    });
                }
                HubTab::Console => {
                    ui.label(
                        RichText::new(
                            "Console · POST /v1/chat/completions · cite/act/abstain · sem job de cargo",
                        )
                        .small()
                        .color(theme::MUTED),
                    );
                }
            }

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

        fn draw_situation(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Situação");
            ui.horizontal_top(|ui| {
                let w = ((ui.available_width() - 8.0) * 0.5).max(220.0);
                self.situation_card(ui, w, "Programa", &self.program_lines());
                ui.add_space(8.0);
                self.situation_card(ui, w, "Online", &self.online_lines());
            });
            ui.add_space(14.0);
        }

        fn program_lines(&self) -> Vec<(String, Color32)> {
            let mut lines = vec![
                (format!("versão {}", env!("CARGO_PKG_VERSION")), theme::TEXT),
                (
                    "LLM gera; AETHER cita, certifica ou abstem.".into(),
                    theme::BRAND,
                ),
            ];
            if self.view.dimensions.is_empty() {
                lines.push(("scorecard ausente — rode a auditoria".into(), theme::WARN));
            } else {
                let hot = if self.view.hot_path_bytes == 0.0 {
                    theme::OK
                } else {
                    theme::WARN
                };
                lines.push((
                    format!(
                        "ACQ {:.2} · cobertura {:.2} · hot-path {}",
                        self.view.quotient,
                        self.view.real_coverage,
                        fmt_compact(self.view.hot_path_bytes)
                    ),
                    hot,
                ));
            }
            if self.api_eval.present {
                let n = self.api_eval.cases.len();
                let ok = self.api_eval.cases.iter().filter(|c| c.pass).count();
                let color = if ok == n && n > 0 {
                    theme::OK
                } else {
                    theme::WARN
                };
                lines.push((format!("API eval {ok}/{n} casos"), color));
            } else {
                lines.push(("API eval ainda não rodou".into(), theme::MUTED));
            }
            if self.agent_drive.present {
                let n = self.agent_drive.asks.len();
                let summary = self
                    .agent_drive
                    .asks
                    .iter()
                    .map(|a| format!("{}→{}", a.subject, a.action))
                    .collect::<Vec<_>>()
                    .join(" · ");
                lines.push((format!("agente {n} asks · {summary}"), theme::TEXT));
            } else {
                lines.push(("test drive do agente ainda não rodou".into(), theme::MUTED));
            }
            let f4_color = if !self.f4.present {
                theme::MUTED
            } else if self.f4.hybrid_pass {
                theme::OK
            } else {
                theme::BAD
            };
            lines.push((f4_program_line(&self.f4), f4_color));
            lines
        }

        fn online_lines(&self) -> Vec<(String, Color32)> {
            let snap = self.jobs.snapshot();
            let job = match snap.phase {
                JobPhase::Idle => ("job parado".into(), theme::MUTED),
                JobPhase::Running => (
                    format!(
                        "job em curso · {}",
                        snap.kind.map(|k| k.label()).unwrap_or("…")
                    ),
                    theme::RUN,
                ),
                JobPhase::Done => (
                    format!(
                        "job concluído · {}",
                        snap.kind.map(|k| k.label()).unwrap_or("—")
                    ),
                    theme::OK,
                ),
                JobPhase::Failed => ("job falhou — veja o log na barra".into(), theme::BAD),
            };
            let demo = match self.jobs.trace.lock() {
                Ok(t) if t.running => (format!("demo ao vivo · {}", t.title), theme::RUN),
                Ok(t) if !t.samples.is_empty() => {
                    (format!("último demo · {}", t.title), theme::MUTED)
                }
                _ => ("demo parado".into(), theme::MUTED),
            };
            let sgdb = sgdb_pointed();
            let sgdb_line = if sgdb {
                ("SGDB apontado nesta sessão".into(), theme::OK)
            } else {
                ("SGDB não apontado (stdio/url vazios)".into(), theme::WARN)
            };
            let host_line = match &self.host {
                None => ("sonda ausente — use Sondar hardware".into(), theme::WARN),
                Some(h) => {
                    let livre = h
                        .ram_available_mb
                        .map(|mb| format!("{} MB livres", fmt_compact(mb)))
                        .unwrap_or_else(|| "RAM livre ?".into());
                    (
                        format!(
                            "{} CPUs · {livre} · cargo {}",
                            h.logical_cpus,
                            on_off(h.cargo_on_path)
                        ),
                        theme::TEXT,
                    )
                }
            };
            vec![
                job,
                demo,
                sgdb_line,
                host_line,
                (
                    "CPU e RAM são da última sonda, não um relógio ao vivo.".into(),
                    theme::MUTED,
                ),
            ]
        }

        fn situation_card(
            &mut self,
            ui: &mut egui::Ui,
            width: f32,
            title: &str,
            lines: &[(String, Color32)],
        ) {
            let body = lines
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            egui::Frame::none()
                .fill(theme::PANEL)
                .stroke(Stroke::new(1.0_f32, theme::STROKE))
                .inner_margin(10.0)
                .rounding(4.0)
                .show(ui, |ui| {
                    ui.set_min_width(width);
                    ui.set_max_width(width);
                    ui.label(RichText::new(title).strong().color(theme::BRAND));
                    ui.add_space(4.0);
                    for (text, color) in lines {
                        let resp = ui.label(RichText::new(text).small().color(*color));
                        if resp.hovered() {
                            self.note_hover(title, body.clone());
                        }
                    }
                });
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

        fn draw_transplant(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Doação · ETL · Transplante");
            // Clone evita conflito borrow (hover mutável vs view).
            let tv = self.transplant.clone();

            if !tv.load_notes.is_empty() {
                for n in &tv.load_notes {
                    ui.label(RichText::new(n).small().color(theme::WARN));
                }
                ui.add_space(4.0);
            }

            ui.label(
                RichText::new("Origem (doação)")
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);
            if let Some(o) = &tv.origin {
                let body = origin_detail(o);
                ui.horizontal_wrapped(|ui| {
                    if meter_card(ui, "Doador", truncate(&o.donor, 18), theme::BRAND) {
                        self.note_hover("Doação (origem)", body.clone());
                    }
                    if meter_card(
                        ui,
                        "Átomos",
                        format!("{} · f{}", o.count, o.facts),
                        theme::ACCENT,
                    ) {
                        self.note_hover("Doação (origem)", body.clone());
                    }
                    if meter_card(ui, "dim", format!("{}→{}", o.dim_in, o.dim_hv), theme::OK) {
                        self.note_hover("Doação (origem)", body.clone());
                    }
                    if meter_card(
                        ui,
                        "Conflitos",
                        format!("{}/{}", o.conflicts_detected, o.conflicts_planted),
                        theme::WARN,
                    ) {
                        self.note_hover("Doação (origem)", body);
                    }
                });
            } else {
                ui.label(
                    RichText::new("Sem data/manifest.json — rode Probe doação HV ou o ETL.")
                        .small()
                        .color(theme::MUTED),
                );
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new("Destino (transplante)")
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);
            {
                let dbody = dest_detail(&tv.dest);
                ui.horizontal_wrapped(|ui| {
                    if meter_card(ui, "HvBank", "AETHV1".into(), theme::BRAND) {
                        self.note_hover("Transplante (destino)", dbody.clone());
                    }
                    if meter_card(
                        ui,
                        "T0",
                        if tv.dest.t0_untouched {
                            "intacto".into()
                        } else {
                            "alterado".into()
                        },
                        theme::OK,
                    ) {
                        self.note_hover("Transplante (destino)", dbody.clone());
                    }
                    if meter_card(
                        ui,
                        "Auth",
                        format!("≤{}", tv.dest.authority_cap),
                        theme::ACCENT,
                    ) {
                        self.note_hover("Transplante (destino)", dbody);
                    }
                });
            }

            ui.add_space(8.0);
            ui.label(
                RichText::new("Gates / estatística")
                    .strong()
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);
            if let Some(g) = &tv.gates {
                let gbody = gates_detail(g);
                let pass_c = if g.pass { theme::OK } else { theme::BAD };
                ui.horizontal_wrapped(|ui| {
                    if meter_card(
                        ui,
                        "Probe",
                        if g.pass { "PASS".into() } else { "FAIL".into() },
                        pass_c,
                    ) {
                        self.note_hover("Gates INTAKE", gbody.clone());
                    }
                    if meter_card(ui, "ρ", format!("{:.4}", g.spearman), theme::BRAND) {
                        self.note_hover("Gates INTAKE", gbody.clone());
                    }
                    if meter_card(ui, "R@10", format!("{:.3}", g.recall_at_10), theme::OK) {
                        self.note_hover("Gates INTAKE", gbody.clone());
                    }
                    if meter_card(ui, "lat", format!("{:.3}ms", g.latency_ms), theme::ACCENT) {
                        self.note_hover("Gates INTAKE", gbody);
                    }
                });
            } else {
                ui.label(
                    RichText::new("Sem target/aether_transplant_hv.json")
                        .small()
                        .color(theme::MUTED),
                );
            }

            if tv.e3.present {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("E3 geometria")
                        .strong()
                        .small()
                        .color(theme::MUTED),
                );
                let e3body = format!(
                    "E3 transplant_probe\n\
                     Spearman={:.4}\n\
                     relação R@1={:.4}\n\
                     analogia R@1={:.4}  R@5={:.4}\n\
                     falsos confiáveis={:.2}%\n\
                     verificador={:.3} ms\n\
                     fatos={}",
                    tv.e3.spearman,
                    tv.e3.rel_recall,
                    tv.e3.analogy_r1,
                    tv.e3.analogy_r5,
                    tv.e3.false_confident * 100.0,
                    tv.e3.verifier_ms,
                    tv.e3.facts
                );
                ui.horizontal_wrapped(|ui| {
                    if meter_card(ui, "E3 ρ", format!("{:.4}", tv.e3.spearman), theme::BRAND) {
                        self.note_hover("E3 geometria", e3body.clone());
                    }
                    if meter_card(
                        ui,
                        "Analogia",
                        format!("{:.3}", tv.e3.analogy_r1),
                        theme::OK,
                    ) {
                        self.note_hover("E3 geometria", e3body);
                    }
                });
            }

            ui.add_space(8.0);
            let uso_label = if tv.uso.present {
                if tv.uso.pass_all {
                    "USO PASS · ver aba Testes IA"
                } else {
                    "USO FAIL · ver aba Testes IA"
                }
            } else {
                "USO: ver aba Testes IA"
            };
            let uso_c = if tv.uso.present && tv.uso.pass_all {
                theme::OK
            } else if tv.uso.present {
                theme::BAD
            } else {
                theme::MUTED
            };
            ui.horizontal(|ui| {
                if meter_card(ui, "USO", uso_label.into(), uso_c) {
                    self.note_hover(
                        "Usabilidade (transplante)",
                        "Grade U1–U6 completa na aba Testes IA.\n\
                         Artefato: target/aether_transplant_uso.json",
                    );
                }
            });
        }

        fn draw_f4(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Memória semântica (F4)");
            let detail = f4_detail(&self.f4);
            let freeze = ui.label(
                RichText::new(
                    "CONGELADA · REPROVADA · cadeira fechada · gate 0.974 intacto (MSG 1093)",
                )
                .small()
                .strong()
                .color(theme::BAD),
            );
            if freeze.hovered() {
                self.note_hover("F4", detail.clone());
            }
            if !self.f4.present {
                let msg = self
                    .f4
                    .error
                    .clone()
                    .unwrap_or_else(|| "artefato ausente".into());
                let resp = ui.label(
                    RichText::new(format!("sem placar v2 · {msg}"))
                        .small()
                        .color(theme::WARN),
                );
                if resp.hovered() {
                    self.note_hover("F4", detail);
                }
                ui.add_space(14.0);
                return;
            }
            let verdict_c = theme::BAD;
            ui.label(
                RichText::new(format!(
                    "v2 · {}d · set {} · {} docs · {} consultas · REPROVADO",
                    self.f4.dim, self.f4.sha_short, self.f4.n_docs, self.f4.n_queries
                ))
                .small()
                .color(verdict_c),
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if meter_card(ui, "BM25", format!("{:.4}", self.f4.bm25_hit5), theme::TEXT) {
                    self.note_hover("F4", detail.clone());
                }
                if meter_card(ui, "Embed", format!("{:.4}", self.f4.embed_hit5), theme::TEXT) {
                    self.note_hover("F4", detail.clone());
                }
                if meter_card(
                    ui,
                    "Híbrido",
                    format!("{:.4}", self.f4.hybrid_hit5),
                    verdict_c,
                ) {
                    self.note_hover("F4", detail.clone());
                }
                if meter_card(
                    ui,
                    "Gate",
                    format!("{:.3}", self.f4.gate_hit5),
                    theme::ACCENT,
                ) {
                    self.note_hover("F4", detail.clone());
                }
            });
            ui.add_space(4.0);
            let v3 = if self.f4.v3_files.is_empty() {
                "v3 esgotada · sem artefato em target/".into()
            } else {
                format!(
                    "v3 esgotada · melhor 0.9577 (Qwen rerank) · {}",
                    self.f4.v3_files.join(" · ")
                )
            };
            let resp = ui.label(RichText::new(v3).small().color(theme::MUTED));
            if resp.hovered() {
                self.note_hover("F4", detail);
            }
            ui.add_space(14.0);
        }

        fn draw_wave_gaps(&mut self, ui: &mut egui::Ui) {
            if self.wave_gaps.is_empty() {
                return;
            }
            self.section_header(ui, "Gaps / info");
            for g in self.wave_gaps.clone() {
                let c = match g.severity {
                    GapSeverity::Warn | GapSeverity::Crit => theme::WARN,
                    GapSeverity::Info => theme::MUTED,
                };
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} · {}", g.code, g.title))
                            .small()
                            .color(c),
                    )
                    .sense(Sense::hover()),
                );
                if resp.hovered() {
                    self.note_hover(
                        g.title.clone(),
                        format!("{}\n→ {}", g.detail, g.remediation),
                    );
                }
            }
        }

        fn draw_ai_uso(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Transplante USO (U1–U6)");
            let tv = self.transplant.clone();
            if tv.uso.present {
                let all_c = if tv.uso.pass_all {
                    theme::OK
                } else {
                    theme::BAD
                };
                ui.label(
                    RichText::new(format!(
                        "pass_all={} · target/aether_transplant_uso.json",
                        tv.uso.pass_all
                    ))
                    .small()
                    .color(theme::MUTED),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for c in &tv.uso.cases {
                        let accent = if c.pass { theme::OK } else { theme::BAD };
                        let detail = format!(
                            "{} — {}\n{}\nscore: {}\npass={}",
                            c.id, c.title, c.detail, c.score, c.pass
                        );
                        let val = if c.pass {
                            "PASS".to_string()
                        } else {
                            "FAIL".to_string()
                        };
                        if meter_card(ui, &c.id, val, accent) {
                            self.note_hover(format!("Uso {}", c.id), detail);
                        }
                    }
                    if meter_card(
                        ui,
                        "USO",
                        if tv.uso.pass_all {
                            "PASS".into()
                        } else {
                            "FAIL".into()
                        },
                        all_c,
                    ) {
                        self.note_hover(
                            "Usabilidade transplante",
                            "U1 auto-recup · U2 SGDB · U3 vizinhança · U4 conflitos · U5 Evidence · U6 latência Rust.",
                        );
                    }
                });
            } else {
                ui.label(
                    RichText::new("Sem USO — use Regenerar USO na barra.")
                        .small()
                        .color(theme::MUTED),
                );
            }
        }

        fn draw_ai_api(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Usabilidade API (exact / paráfrase / escopo)");
            let report = self.api_eval.clone();
            if !report.present {
                ui.label(
                    RichText::new("Sem aether_api_eval.json — rode Usabilidade API (feature api).")
                        .small()
                        .color(theme::MUTED),
                );
                return;
            }
            ui.label(
                RichText::new(format!(
                    "{} · cue p50/p99 = {}/{} µs",
                    report.path.display(),
                    report.cue_p50_us,
                    report.cue_p99_us
                ))
                .small()
                .color(theme::MUTED),
            );
            ui.add_space(4.0);
            egui::Grid::new("api_eval_grid")
                .num_columns(4)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("condição").strong().small());
                    ui.label(RichText::new("hits").strong().small());
                    ui.label(RichText::new("expect").strong().small());
                    ui.label(RichText::new("pass").strong().small());
                    ui.end_row();
                    for c in &report.cases {
                        let accent = if c.pass { theme::OK } else { theme::BAD };
                        let resp = ui.add(
                            egui::Label::new(RichText::new(&c.title).color(theme::TEXT))
                                .sense(Sense::click()),
                        );
                        if resp.hovered() {
                            self.note_hover(format!("API {}", c.id), api_case_detail(c));
                        }
                        ui.label(format!("{}/{}", c.hits, c.total));
                        ui.label(&c.expect);
                        ui.colored_label(accent, if c.pass { "PASS" } else { "FAIL" });
                        ui.end_row();
                    }
                });
        }

        fn draw_ai_agent(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Test drive agente (cite / abstain)");
            let drive = self.agent_drive.clone();
            if !drive.present {
                ui.label(
                    RichText::new("Sem aether_agent_drive.json — rode Test drive agente.")
                        .small()
                        .color(theme::MUTED),
                );
                return;
            }
            ui.label(
                RichText::new(format!("{} · sgdb={}", drive.path.display(), drive.sgdb))
                    .small()
                    .color(theme::MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for a in &drive.asks {
                    let known = a.subject.eq_ignore_ascii_case("Brasil");
                    let ok = if known {
                        !a.action.eq_ignore_ascii_case("abstain")
                    } else {
                        a.action.eq_ignore_ascii_case("abstain")
                    };
                    let accent = if ok { theme::OK } else { theme::BAD };
                    let label = format!("{}|{}", a.subject, a.relation);
                    if meter_card(ui, &label, a.action.clone(), accent) {
                        self.note_hover(format!("Ask {}", a.subject), agent_ask_detail(a));
                    }
                }
            });
            ui.add_space(8.0);
            self.draw_usability_contract(ui);
        }

        fn draw_usability_contract(&mut self, ui: &mut egui::Ui) {
            self.section_header(ui, "Contrato usabilidade IA (U1–U13)");
            ui.label(
                RichText::new(
                    "North star: saber o que foi dito, de onde veio e o que aconteceu — 0 false-act, abstain OOD ≥90%, 0 efeito sem HITL.",
                )
                .small()
                .color(theme::MUTED),
            );
            ui.add_space(4.0);
            for (id, title, aether, llm) in USABILITY_CONTRACT {
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!("{id} · {title}"))
                            .small()
                            .color(theme::TEXT),
                    )
                    .sense(Sense::hover()),
                );
                if resp.hovered() {
                    self.note_hover(
                        format!("{id} — {title}"),
                        format!("AETHER: {aether}\n{llm}"),
                    );
                }
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

    fn sgdb_pointed() -> bool {
        let set = |key: &str| {
            std::env::var(key)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        };
        set("AETHER_SGDB_STDIO") || set("AETHER_SGDB_URL")
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
            transplant: TransplantView::default(),
            api_eval: ApiEvalReport::default(),
            agent_drive: AgentDriveReport::default(),
            f4: F4View::default(),
            wave_gaps: Vec::new(),
            tab: HubTab::Lab,
            status: String::new(),
            path_arg,
            jobs: JobRunner::new(),
            host: None,
            detail: DetailPane::Idle,
            hover_scratch: None,
            confirm_daemon: false,
            last_job_done: true,
            theme_ready: false,
            console: ConsolePane::default(),
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
