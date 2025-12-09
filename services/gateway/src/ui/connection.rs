use crate::modbus::writer::BackgroundCommand;
use crate::state::{ConnectionStatus, NesStatus, SharedState, VpnStatus};
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionTransport {
    #[default]
    Serial,
    Ble,
}

#[derive(Debug, Default)]
pub struct ConnectionState {
    pub selected_port: String,
    pub selected_ble_device: String,
    pub transport: ConnectionTransport,
}

#[allow(clippy::too_many_lines, reason = "connection panel")]
pub fn render(
    ui: &mut egui::Ui,
    state: &SharedState,
    cmd_tx: &std::sync::mpsc::Sender<BackgroundCommand>,
    cs: &mut ConnectionState,
) {
    let (ser_status, ports, ble_devs, last_poll, poll_errs, mqtt_st) = {
        let Ok(s) = state.read() else { return };
        (s.serial_status.clone(), s.available_ports.clone(),
         s.available_ble_devices.clone(), s.last_poll_ms,
         s.poll_error_count, s.mqtt_status.clone())
    };
    let connected = ser_status == ConnectionStatus::Connected;
    let busy = ser_status == ConnectionStatus::Connecting;

    let vpn_st = state.read().map_or(VpnStatus::NotConfigured, |s| s.vpn_status.clone());
    let (nes_st, queries) = state.read()
        .map_or((NesStatus::Disabled, vec![]), |s| {
            (s.nes_status.clone(), s.tracked_queries.clone())
        });

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.set_min_width(ui.available_width());

        // ── modbus ──────────────────────────────────────────────
        super::section_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("Modbus Connection");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("Transport:");
                ui.selectable_value(&mut cs.transport, ConnectionTransport::Serial, "Serial");
                ui.selectable_value(&mut cs.transport, ConnectionTransport::Ble, "BLE");
            });
            ui.add_space(6.0);

            match cs.transport {
                ConnectionTransport::Serial => {
                    ui.horizontal(|ui| {
                        ui.label("Port:");
                        let lbl = if cs.selected_port.is_empty() { "Select\u{2026}" } else { &cs.selected_port };
                        egui::ComboBox::from_id_salt("port_sel")
                            .selected_text(lbl)
                            .width(ui.available_width() - 80.0)
                            .show_ui(ui, |ui| {
                                for (name, desc) in &ports {
                                    let d = if desc.is_empty() { name.clone() } else { format!("{name} - {desc}") };
                                    ui.selectable_value(&mut cs.selected_port, name.clone(), d);
                                }
                            });
                        if ui.button("Scan").clicked() {
                            let _ = cmd_tx.send(BackgroundCommand::ScanPorts);
                        }
                    });
                }
                ConnectionTransport::Ble => {
                    ui.horizontal(|ui| {
                        ui.label("Device:");
                        let lbl = if cs.selected_ble_device.is_empty() {
                            "Select BLE\u{2026}".to_string()
                        } else {
                            ble_devs.iter()
                                .find(|(id, _)| *id == cs.selected_ble_device)
                                .map_or_else(|| cs.selected_ble_device.clone(), |(_, n)| n.clone())
                        };
                        // 200px cap keeps the dropdown from falling off the
                        // bottom of the Waveshare 7" (600px total height)
                        egui::ComboBox::from_id_salt("ble_sel")
                            .selected_text(&lbl)
                            .width(ui.available_width() - 80.0)
                            .height(200.0)
                            .show_ui(ui, |ui| {
                                for (id, name) in &ble_devs {
                                    ui.selectable_value(&mut cs.selected_ble_device, id.clone(), name.clone());
                                }
                            });
                        if ui.button("Scan").clicked() {
                            let _ = cmd_tx.send(BackgroundCommand::ScanBle);
                        }
                    });
                }
            }

            ui.add_space(6.0);

            // connect / disconnect / emulated, all in one horizontal row
            if connected || busy {
                let btn = egui::Button::new(
                    egui::RichText::new("Disconnect").color(super::STATUS_RED)
                ).min_size(egui::vec2(ui.available_width(), 32.0));
                if ui.add(btn).clicked() {
                    let _ = cmd_tx.send(BackgroundCommand::Disconnect);
                }
            } else {
                let has_target = match cs.transport {
                    ConnectionTransport::Serial => !cs.selected_port.is_empty(),
                    ConnectionTransport::Ble => !cs.selected_ble_device.is_empty(),
                };
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 8.0) / 2.0;
                    let connect = egui::Button::new("Connect").min_size(egui::vec2(w, super::TOUCH_MIN));
                    if ui.add_enabled(has_target, connect).clicked() {
                        let cmd = match cs.transport {
                            ConnectionTransport::Serial =>
                                BackgroundCommand::ConnectSerial(cs.selected_port.clone()),
                            ConnectionTransport::Ble =>
                                BackgroundCommand::ConnectBle(cs.selected_ble_device.clone()),
                        };
                        let _ = cmd_tx.send(cmd);
                    }
                    let emu = egui::Button::new("Emulated").min_size(egui::vec2(w, super::TOUCH_MIN));
                    if ui.add(emu).clicked() {
                        let _ = cmd_tx.send(BackgroundCommand::ConnectEmulated);
                    }
                });
            }

            ui.add_space(8.0);

            // status grid, inline here because it's only a few rows and
            // extracting it to a function just adds indirection
            egui::Grid::new("serial_grid").num_columns(2).spacing([20.0, 8.0]).show(ui, |ui| {
                ui.label("Status:");
                ui.colored_label(conn_color(&ser_status), format!("{ser_status}"));
                ui.end_row();
                ui.label("Poll errors:");
                let ec = if poll_errs > 0 { super::STATUS_RED } else { ui.visuals().text_color() };
                ui.colored_label(ec, format!("{poll_errs}"));
                ui.end_row();
                if let Some(ms) = last_poll {
                    ui.label("Last poll:");
                    let ago = u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0).saturating_sub(ms);
                    ui.label(format!("{ago}ms ago"));
                    ui.end_row();
                }
            });
        });

        ui.add_space(8.0);

        // ── mqtt ────────────────────────────────────────────────
        // kept deliberately simple, there's nothing to configure,
        // just show whether we're connected or not
        super::section_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("MQTT");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.colored_label(conn_color(&mqtt_st), format!("{mqtt_st}"));
            });
        });

        ui.add_space(8.0);

        // ── vpn ─────────────────────────────────────────────────
        super::section_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("VPN");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Status:");
                let c = match &vpn_st {
                    VpnStatus::Connected(_) => super::STATUS_GREEN,
                    VpnStatus::Error(_) => super::STATUS_RED,
                    VpnStatus::NotConfigured => egui::Color32::GRAY,
                    _ => super::STATUS_AMBER,
                };
                ui.colored_label(c, format!("{vpn_st}"));
            });

            let retriable = matches!(vpn_st,
                VpnStatus::Error(_) | VpnStatus::Provisioning
                | VpnStatus::Checking | VpnStatus::Installing | VpnStatus::Connecting);
            if retriable {
                ui.add_space(6.0);
                let retry = egui::Button::new("Retry VPN").min_size(egui::vec2(0.0, 32.0));
                if ui.add(retry).clicked()
                    && cmd_tx.send(BackgroundCommand::RetryVpn).is_err() {
                        tracing::error!("cmd channel closed, can't retry VPN");
                    }
            }
        });

        // ── nes ─────────────────────────────────────────────────
        if nes_st != NesStatus::Disabled {
            ui.add_space(8.0);
            super::section_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("NES Worker");
                ui.add_space(6.0);
                let nc = match &nes_st {
                    NesStatus::Connected { .. } => super::STATUS_GREEN,
                    NesStatus::Error(_) => super::STATUS_RED,
                    // blue for "waiting", stands out from the amber "in progress" states
                    // so operators can tell at a glance whether the worker is blocked on
                    // the device connection or actively doing something
                    NesStatus::WaitingForDevice => egui::Color32::from_rgb(147, 197, 253),
                    NesStatus::Disabled => egui::Color32::GRAY,
                    _ => super::STATUS_AMBER,
                };
                ui.horizontal(|ui| {
                    ui.label("Status:");
                    ui.colored_label(nc, format!("{nes_st}"));
                });
                let nrun = queries.iter().filter(|q| q.status == "RUNNING").count();
                let nfail = queries.iter().filter(|q| q.status == "FAILED").count();
                if !queries.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label("Queries:");
                        let col = if nfail > 0 { super::STATUS_RED } else { super::STATUS_GREEN };
                        ui.colored_label(col, format!("{nrun} running, {nfail} failed"));
                    });
                }
            });
        }
    });
}

const fn conn_color(s: &ConnectionStatus) -> egui::Color32 {
    match s {
        ConnectionStatus::Connected => super::STATUS_GREEN,
        ConnectionStatus::Connecting => super::STATUS_AMBER,
        ConnectionStatus::Error(_) => super::STATUS_RED,
        ConnectionStatus::Disconnected => egui::Color32::GRAY,
    }
}
