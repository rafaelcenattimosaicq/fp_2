use crate::state::{LogLevel, SharedState};
use eframe::egui;

pub fn render(ui: &mut egui::Ui, state: &SharedState) {
    let Ok(st) = state.read() else {
        ui.colored_label(super::STATUS_RED, "state lock poisoned");
        return;
    };

    ui.horizontal(|ui| {
        ui.heading("Event Log");
        ui.separator();
        ui.label(egui::RichText::new(format!("{} entries", st.log.len()))
            .small().color(egui::Color32::GRAY));
    });
    ui.add_space(4.0);

    // no section_frame
    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        for e in &st.log {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(e.timestamp.format("%H:%M:%S").to_string())
                    .monospace().color(egui::Color32::GRAY));
                let (tag, c) = match e.level {
                    LogLevel::Info => ("INFO", super::STATUS_GREEN),
                    LogLevel::Warn => ("WARN", super::STATUS_AMBER),
                    LogLevel::Error => ("ERR ", super::STATUS_RED),
                };
                ui.label(egui::RichText::new(tag).monospace().color(c));
                ui.label(&e.message);
            });
        }
    });
}
