//! Dashboard Windows de métricas AETHER (hover explica cada medidor).
//!
//! ```text
//! cargo run --release --bin aether-dashboard --features dashboard
//! ```
//!
//! Lê `target/aether_scorecard.json` (rode antes a auditoria / throughput).
//! Somente Windows nesta entrega.

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
        dimension_tooltip, global_tooltip, load_scorecard, throughput_tooltip, ScorecardView,
    };

    /// App do dashboard.
    pub struct DashboardApp {
        view: ScorecardView,
        status: String,
        path_arg: Option<String>,
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
    }

    impl App for DashboardApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::TopBottomPanel::top("top").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(RichText::new("AETHER").strong().size(22.0));
                    ui.label(
                        RichText::new("dashboard de métricas")
                            .color(Color32::from_rgb(120, 140, 150)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Recarregar").clicked() {
                            self.reload();
                        }
                    });
                });
                ui.label(RichText::new(&self.status).small().color(Color32::GRAY));
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_globals(ui);
                    ui.add_space(12.0);
                    self.draw_dimensions(ui);
                    ui.add_space(12.0);
                    self.draw_throughput(ui);
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "Passe o mouse sobre qualquer medidor para ver o que ele demonstra \
                             (inferência, coerência, RAG, vazão…).",
                        )
                        .small()
                        .color(Color32::from_rgb(140, 150, 160)),
                    );
                });
            });
        }
    }

    impl DashboardApp {
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

        fn draw_dimensions(&self, ui: &mut egui::Ui) {
            ui.label(RichText::new("Inteligência cognitiva (T01–T12)").strong().size(16.0));
            if self.view.dimensions.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(180, 100, 60),
                    "Nenhuma dimensão no scorecard. Rode:\n  cargo test --release --test cognitive_audit",
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
                        dim_card(ui, &d.id, &d.name, &body, score_color(d.score), tip);
                        if (i + 1) % 4 == 0 {
                            ui.end_row();
                        }
                    }
                });
        }

        fn draw_throughput(&self, ui: &mut egui::Ui) {
            ui.label(RichText::new("Vazão (cycles/s · eq-t/s · SOP/s · TOP/s)").strong().size(16.0));
            let Some(tp) = &self.view.throughput else {
                ui.label(
                    RichText::new(
                        "Sem bloco throughput. Rode: cargo bench --bench throughput_tps --features m4",
                    )
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
                    .stroke(Stroke::new(1.0, Color32::from_rgb(55, 70, 80)))
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
        ui.painter()
            .rect_filled(rect, 4.0, fill);
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(1.5, accent));
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
    ) {
        let size = Vec2::new(200.0, 96.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter()
            .rect_filled(rect, 4.0, Color32::from_rgb(22, 28, 34));
        ui.painter()
            .rect_stroke(rect, 4.0, Stroke::new(1.5, accent));
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
        resp.on_hover_text(tip);
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
                        "aether-dashboard — métricas AETHER (Windows)\n\
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
        };
        app.reload();

        let native = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1100.0, 760.0])
                .with_title("AETHER — Dashboard de métricas"),
            ..Default::default()
        };
        eframe::run_native(
            "AETHER Dashboard",
            native,
            Box::new(|_cc| Ok(Box::new(app))),
        )
    }
}
