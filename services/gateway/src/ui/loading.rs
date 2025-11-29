use crate::state::{LogLevel, ServiceStatus, SharedState};
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingOutcome {
    Ready,
    Cancelled,
}

pub struct LoadingApp {
    state: SharedState,
    outcome: Option<LoadingOutcome>,
}

#[allow(dead_code)]
impl LoadingApp {
    pub const fn new(state: SharedState) -> Self {
        Self { state, outcome: None }
    }
    pub const fn outcome(&self) -> Option<LoadingOutcome> { self.outcome }
}

impl eframe::App for LoadingApp {
    #[allow(clippy::too_many_lines)]
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        super::configure_visuals(ctx);

        if self.state.read().map(|s| s.all_services_running()).unwrap_or(false) {
            self.outcome = Some(LoadingOutcome::Ready);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::same(20)).fill(super::BG_PRIMARY))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("IoT Gateway - Starting")
                        .strong().size(18.0).color(super::TEXT_PRIMARY));
                });
                ui.add_space(16.0);

                // service status list
                if let Ok(st) = self.state.read() {
                    super::section_frame(ui).show(ui, |ui| {
                        for (name, status) in &st.docker_services {
                            ui.horizontal(|ui| {
                                // ascii status icons, [ok], [..], [!!], [ ]
                                // tried Unicode checkmarks but they render as
                                // boxes on the Pi framebuffer (no emoji font)
                                let (icon, ic) = match status {
                                    ServiceStatus::Running => ("[ok]", super::STATUS_GREEN),
                                    ServiceStatus::Pulling | ServiceStatus::Starting =>
                                        ("[..]", super::STATUS_AMBER),
                                    ServiceStatus::Error(_) => ("[!!]", super::STATUS_RED),
                                    ServiceStatus::Pending => ("[ ]", super::TEXT_MUTED),
                                };
                                ui.label(egui::RichText::new(icon).color(ic).size(13.0).monospace());
                                ui.label(egui::RichText::new(format!("{name:<22}"))
                                    .color(super::TEXT_SECONDARY).size(13.0).monospace());
                                // status text color matches the icon
                                ui.label(egui::RichText::new(status.to_string()).color(ic).size(13.0));
                            });
                        }
                    });
                }

                ui.add_space(12.0);
                ui.label(egui::RichText::new("Log").color(super::TEXT_MUTED).size(11.0));

                // mini log viewer, shares state.log with the main UI's log tab
                super::section_frame(ui).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(200.0).stick_to_bottom(true).show(ui, |ui| {
                        if let Ok(st) = self.state.read() {
                            for e in &st.log {
                                let c = match e.level {
                                    LogLevel::Error => super::STATUS_RED,
                                    LogLevel::Warn => super::STATUS_AMBER,
                                    LogLevel::Info => super::TEXT_MUTED,
                                };
                                ui.label(egui::RichText::new(
                                    format!("[{}] {}", e.timestamp.format("%H:%M:%S"), e.message)
                                ).color(c).size(11.0).monospace());
                            }
                        }
                    });
                });

                if self.state.read().map(|s| s.any_service_error()).unwrap_or(false) {
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Setup failed. Check log above.")
                            .color(super::STATUS_RED).size(13.0));
                    });
                }
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.outcome.is_none() {
            self.outcome = Some(LoadingOutcome::Cancelled);
        }
    }
}
