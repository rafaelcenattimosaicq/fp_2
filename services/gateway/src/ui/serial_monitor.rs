use crate::state::{SharedState, TrafficDirection};
use eframe::egui;

//  blue for TX;; green for RX
const TX_COL: egui::Color32 = egui::Color32::from_rgb(100, 149, 237);
const RX_COL: egui::Color32 = egui::Color32::from_rgb(125, 239, 160);

pub fn render(ui: &mut egui::Ui, state: &SharedState) {
    let Ok(mut st) = state.write() else { return };

    ui.horizontal(|ui| {
        ui.heading("Serial Monitor");
        ui.separator();
        ui.label(egui::RichText::new(format!("{} frames", st.serial_traffic.len()))
            .small().color(egui::Color32::GRAY));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Clear").clicked() { st.clear_traffic(); }
        });
    });
    ui.add_space(4.0);

    if st.serial_traffic.is_empty() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No traffic yet").color(egui::Color32::GRAY));
            ui.label(egui::RichText::new("Connect to a device to capture Modbus frames")
                .small().color(egui::Color32::from_rgb(120, 120, 130)));
        });
        return;
    }

    // column headers, deliberately not in a Grid because the hex column
    // is variable width and Grid would either clip or waste space
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Time").small().strong().color(egui::Color32::GRAY));
        ui.add_space(20.0);
        ui.label(egui::RichText::new("Dir").small().strong().color(egui::Color32::GRAY));
        ui.add_space(16.0);
        ui.label(egui::RichText::new("Hex").small().strong().color(egui::Color32::GRAY));
    });
    ui.separator();

    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
        for entry in &st.serial_traffic {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(
                    entry.timestamp.format("%H:%M:%S%.3f").to_string()
                ).monospace().color(egui::Color32::GRAY));

                let (arrow, tag, c) = match entry.direction {
                    TrafficDirection::Tx => (">>", "TX", TX_COL),
                    TrafficDirection::Rx => ("<<", "RX", RX_COL),
                };
                ui.label(egui::RichText::new(format!("{arrow} {tag}")).monospace().color(c));

                let pipe = egui::Color32::from_rgb(60, 60, 70);
                ui.label(egui::RichText::new("|").monospace().color(pipe));

                let hex: String = entry.bytes.iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                ui.label(egui::RichText::new(&hex).monospace());

                ui.label(egui::RichText::new("|").monospace().color(pipe));

                // printable ASCII alongside the hex dump, helps spot
                // modbus exception responses visually (they start with
                // the slave ID followed by FC|0x80)
                let ascii: String = entry.bytes.iter()
                    .map(|&b| if b.is_ascii_graphic() || b == b' ' { char::from(b) } else { '.' })
                    .collect();
                ui.label(egui::RichText::new(ascii).monospace()
                    .color(egui::Color32::from_rgb(160, 160, 170)));
            });
        }
    });
}
