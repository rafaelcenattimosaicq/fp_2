use crate::device_descriptor::{Register, RegisterValue};
use crate::modbus::writer::BackgroundCommand;
use crate::state::SharedState;
use eframe::egui;
use std::collections::HashMap;

#[derive(Default)]
pub struct ConfigEditorState {
    pub pending_writes: HashMap<String, String>,
    pub search: String,
    pub status_message: Option<String>,
}

pub fn render(
    ui: &mut egui::Ui,
    state: &SharedState,
    cmd_tx: &std::sync::mpsc::Sender<BackgroundCommand>,
    ed: &mut ConfigEditorState,
) {
    let Ok(st) = state.read() else { return };

    let Some(desc) = &st.descriptor else {
        ui.centered_and_justified(|ui| { ui.label("No device descriptor loaded."); });
        return;
    };
    let Some(chars) = &desc.characteristics else {
        ui.centered_and_justified(|ui| { ui.label("No characteristics in descriptor."); });
        return;
    };

    // Motor control panel for wind turbine (0x0008)
    let is_turbine = desc.device_description.as_ref()
        .and_then(|dd| dd.device_id.as_deref())
        .is_some_and(|id| id.contains("0x0008") || id.contains("0x0008"));
    if is_turbine {
        draw_motor_panel(ui, state, cmd_tx, ed);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
    }

    // header: search box + apply button
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut ed.search);
        ui.separator();

        let n = ed.pending_writes.len();
        let lbl = if n > 0 { format!("Apply ({n})") } else { "Apply".into() };
        if ui.add_enabled(n > 0, egui::Button::new(egui::RichText::new(lbl).strong())).clicked() {
            let writes: Vec<(String, f64)> = ed.pending_writes.iter()
                .filter_map(|(id, v)| v.parse::<f64>().ok().map(|f| (id.clone(), f)))
                .collect();
            if !writes.is_empty() {
                let wc = writes.len();
                let _ = cmd_tx.send(BackgroundCommand::WriteRegs(writes));
                ed.status_message = Some(format!("sent {wc} write(s)"));
                ed.pending_writes.clear();
            }
        }
        if let Some(msg) = &ed.status_message {
            ui.colored_label(super::STATUS_GREEN, msg);
        }
    });
    ui.separator();

    let q = ed.search.to_lowercase();

    egui::ScrollArea::vertical().show(ui, |ui| {
        if desc.services.is_empty() {
            // flat list, older descriptors don't have service grouping
            draw_params(ui, &chars.parameters, &st.parameter_values, &q, &mut ed.pending_writes);
        } else {
            for svc in &desc.services {
                for grp in &svc.config_groups {
                    let hdr = match (grp.name.as_deref(), grp.acronym.as_deref()) {
                        (Some(n), Some(a)) => format!("{n} ({a})"),
                        (Some(n), None) => n.to_string(),
                        _ => "Unnamed".into(),
                    };

                    let params: Vec<&Register> = grp.parameters.iter()
                        .filter_map(|pref| {
                            let id = pref.id.as_deref()?;
                            chars.parameters.iter().find(|p| p.id == id)
                        })
                        .filter(|p| matches_search(p, &q))
                        .collect();

                    if params.is_empty() { continue; }

                    egui::CollapsingHeader::new(&hdr).default_open(false).show(ui, |ui| {
                        param_grid(ui, &params, &st.parameter_values, &mut ed.pending_writes);
                    });
                }
            }
        }
    });
}

// grid with columns: addr | name (unit) | default | current | edit
fn draw_params(
    ui: &mut egui::Ui,
    params: &[Register],
    vals: &HashMap<String, RegisterValue>,
    search: &str,
    pending: &mut HashMap<String, String>,
) {
    let filtered: Vec<&Register> = params.iter().filter(|p| matches_search(p, search)).collect();
    param_grid(ui, &filtered, vals, pending);
}

fn param_grid(
    ui: &mut egui::Ui,
    params: &[&Register],
    vals: &HashMap<String, RegisterValue>,
    pending: &mut HashMap<String, String>,
) {
    let avail = ui.available_width();
    let name_w = (avail - 70.0 - 80.0 - 80.0 - 180.0 - 48.0).max(120.0);

    egui::Grid::new(ui.next_auto_id())
        .num_columns(5).striped(true).spacing([12.0, 4.0]).min_col_width(0.0)
        .show(ui, |ui| {
            // header row
            for (w, label) in [
                (70.0, "Addr"), (name_w, "Parameter"), (80.0, "Default"),
                (80.0, "Current"), (180.0, "New Value"),
            ] {
                ui.add_sized([w, 18.0], egui::Label::new(egui::RichText::new(label).strong()));
            }
            ui.end_row();

            for reg in params {
                let ro = reg.is_read_only.unwrap_or(false);
                let addr = reg.address.map_or_else(|| "-".into(), |a| format!("{a}"));

                ui.add_sized([70.0, 18.0], egui::Label::new(
                    egui::RichText::new(format!("[{addr}]")).monospace().color(egui::Color32::GRAY)
                ));

                let unit = reg.unit.as_deref().unwrap_or("");
                let nm = reg.display_name();
                let name_lbl = if unit.is_empty() { nm.to_string() } else { format!("{nm} ({unit})") };
                ui.add_sized([name_w, 18.0], egui::Label::new(&name_lbl).truncate());

                let def = reg.default_value.map_or_else(|| "-".into(), |d| format!("{d}"));
                ui.add_sized([80.0, 18.0], egui::Label::new(def));

                match vals.get(&reg.id) {
                    Some(v) => {
                        ui.add_sized([80.0, 18.0], egui::Label::new(fmt_val(v)));
                    }
                    None => {
                        ui.add_sized([80.0, 18.0], egui::Label::new(
                            egui::RichText::new("\u{2014}").color(egui::Color32::GRAY)
                        ));
                    }
                }

                if ro {
                    ui.add_sized([180.0, 18.0], egui::Label::new(
                        egui::RichText::new("read-only").small().color(egui::Color32::GRAY)
                    ));
                } else {
                    let entry = pending.entry(reg.id.clone())
                        .or_insert_with(|| seed_val(reg, vals));

                    let rtype = reg.register_type.as_deref().unwrap_or("");
                    match rtype {
                        "enum" => {
                            let idx: u16 = entry.parse().unwrap_or(0);
                            let cur = reg.fields.iter()
                                .find(|f| f.index == idx)
                                .and_then(|f| f.name.as_deref())
                                .unwrap_or("?");
                            egui::ComboBox::from_id_salt(&reg.id)
                                .selected_text(cur)
                                .width(160.0)
                                .height(180.0) // keep popup on 600px screen
                                .show_ui(ui, |ui| {
                                    for f in &reg.fields {
                                        let fl = f.name.as_deref().unwrap_or("?");
                                        if ui.selectable_label(f.index == idx, fl).clicked() {
                                            *entry = format!("{}", f.index);
                                        }
                                    }
                                });
                        }
                        "boolean" => {
                            let mut on = *entry == "1";
                            if ui.checkbox(&mut on, "").changed() {
                                *entry = if on { "1" } else { "0" }.into();
                            }
                        }
                        _ => {
                            let resp = ui.add_sized([180.0, 18.0],
                                egui::TextEdit::singleline(entry).font(egui::TextStyle::Monospace));
                            if let (Some(lo), Some(hi)) = (reg.min_value, reg.max_value) {
                                resp.on_hover_text(format!("{lo} .. {hi}"));
                            }
                        }
                    }
                }
                ui.end_row();
            }
        });
}

fn fmt_val(v: &RegisterValue) -> String {
    match v {
        RegisterValue::Float(f) => format!("{f}"),
        RegisterValue::Unsigned(u) => format!("{u}"),
        RegisterValue::Enum(s) => s.clone(),
        RegisterValue::Boolean(b) => if *b { "On" } else { "Off" }.into(),
        RegisterValue::Bitwise(bits) => {
            let set: Vec<&str> = bits.iter()
                .filter(|(_, v)| **v).map(|(k, _)| k.as_str()).collect();
            if set.is_empty() { "(none)".into() } else { set.join(", ") }
        }
    }
}

fn seed_val(reg: &Register, vals: &HashMap<String, RegisterValue>) -> String {
    match vals.get(&reg.id) {
        Some(RegisterValue::Float(f)) => format!("{f}"),
        Some(RegisterValue::Unsigned(u)) => format!("{u}"),
        Some(RegisterValue::Enum(s)) => {
            reg.fields.iter()
                .find(|f| f.name.as_deref() == Some(s.as_str()))
                .map_or(String::new(), |f| format!("{}", f.index))
        }
        Some(RegisterValue::Boolean(b)) => if *b { "1" } else { "0" }.into(),
        Some(RegisterValue::Bitwise(bits)) => {
            let mut mask: u16 = 0;
            for f in &reg.fields {
                let nm = f.name.clone().unwrap_or_else(|| format!("bit_{}", f.index));
                if bits.get(&nm).copied().unwrap_or(false) {
                    mask |= 1 << f.index;
                }
            }
            format!("{mask}")
        }
        None => reg.default_value.map_or(String::new(), |d| format!("{d}")),
    }
}

fn matches_search(reg: &Register, q: &str) -> bool {
    if q.is_empty() { return true; }
    let check = |o: Option<&str>| o.is_some_and(|s| s.to_lowercase().contains(q));
    check(Some(&reg.id)) || check(reg.name.as_deref())
        || check(reg.acronym.as_deref()) || check(reg.description.as_deref())
}

// ── Wind Turbine Motor Control Panel ──────────────────────────────────

fn draw_motor_panel(
    ui: &mut egui::Ui,
    state: &SharedState,
    cmd_tx: &std::sync::mpsc::Sender<BackgroundCommand>,
    ed: &mut ConfigEditorState,
) {
    let frame = super::section_frame(ui);
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong("Wind Turbine Motor Control");
            ui.add_space(16.0);

            // read regs
            let (cur_state, cur_speed, cur_rpm, cur_dir) = state.read()
                .map(|s| {
                    let st = match s.register_values.get("STATUS_ID_MOTOR_STATE") {
                        Some(RegisterValue::Unsigned(v)) => *v as u16,
                        _ => 0,
                    };
                    let sp = match s.register_values.get("STATUS_ID_MOTOR_SPEED") {
                        Some(RegisterValue::Unsigned(v)) => *v as u16,
                        _ => 0,
                    };
                    let rpm = match s.register_values.get("STATUS_ID_MOTOR_RPM") {
                        Some(RegisterValue::Unsigned(v)) => *v as u16,
                        _ => 0,
                    };
                    let dir = match s.register_values.get("STATUS_ID_MOTOR_DIRECTION") {
                        Some(RegisterValue::Unsigned(v)) => *v as u16,
                        _ => 0,
                    };
                    (st, sp, rpm, dir)
                })
                .unwrap_or((0, 0, 0, 0));

            let state_label = match cur_state {
                1 => "Forward",
                2 => "Reverse",
                3 => "Braking",
                _ => "Stopped",
            };
            let state_color = match cur_state {
                1 | 2 => super::STATUS_GREEN,
                3 => super::STATUS_AMBER,
                _ => egui::Color32::GRAY,
            };

            let (dot_rect, _) = ui.allocate_exact_size(
                egui::vec2(10.0, 10.0), egui::Sense::hover(),
            );
            ui.painter().circle_filled(dot_rect.center(), 5.0, state_color);
            ui.label(egui::RichText::new(state_label).color(state_color));
            ui.separator();
            ui.label(format!("PWM: {cur_speed}"));
            ui.separator();
            ui.label(format!("RPM: {cur_rpm}"));
            if cur_dir == 2 {
                ui.colored_label(super::STATUS_AMBER, "(reverse)");
            }
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            // slider
            let speed_key = "motor_speed_slider";
            let speed: &mut u8 = &mut ed.pending_writes
                .entry(speed_key.to_string())
                .or_insert_with(|| "128".to_string())
                .parse::<u8>()
                .unwrap_or(128);

            // mutable i32 for this slider
            let mut speed_val: i32 = *speed as i32;
            ui.label("Speed:");
            let slider = egui::Slider::new(&mut speed_val, 0..=255)
                .text("PWM")
                .clamp_to_range(true);
            if ui.add(slider).changed() {
                ed.pending_writes.insert(speed_key.to_string(), format!("{speed_val}"));
            }

            ui.add_space(12.0);

            let btn_size = egui::vec2(80.0, super::TOUCH_MIN - 8.0);

            // forward button
            if ui.add_sized(btn_size,
                egui::Button::new(egui::RichText::new("\u{25B6} Forward").color(egui::Color32::WHITE))
                    .fill(super::STATUS_GREEN)
            ).clicked() {
                let spd = ed.pending_writes.get(speed_key)
                    .and_then(|s| s.parse::<f64>().ok()).unwrap_or(128.0);
                let _ = cmd_tx.send(BackgroundCommand::WriteRegs(vec![
                    ("PARAM_MOTOR_SPEED".into(), spd),
                    ("PARAM_MOTOR_COMMAND".into(), 1.0),
                ]));
                ed.status_message = Some("Motor: Forward".into());
            }

            // reverse button
            if ui.add_sized(btn_size,
                egui::Button::new(egui::RichText::new("\u{25C0} Reverse").color(egui::Color32::WHITE))
                    .fill(super::STATUS_AMBER)
            ).clicked() {
                let spd = ed.pending_writes.get(speed_key)
                    .and_then(|s| s.parse::<f64>().ok()).unwrap_or(128.0);
                let _ = cmd_tx.send(BackgroundCommand::WriteRegs(vec![
                    ("PARAM_MOTOR_SPEED".into(), spd),
                    ("PARAM_MOTOR_COMMAND".into(), 2.0),
                ]));
                ed.status_message = Some("Motor: Reverse".into());
            }

            // brake button
            if ui.add_sized(btn_size,
                egui::Button::new(egui::RichText::new("\u{23F9} Brake").color(egui::Color32::WHITE))
                    .fill(super::STATUS_RED)
            ).clicked() {
                let _ = cmd_tx.send(BackgroundCommand::WriteRegs(vec![
                    ("PARAM_MOTOR_COMMAND".into(), 3.0),
                ]));
                ed.status_message = Some("Motor: Brake".into());
            }

            // stop button
            if ui.add_sized(btn_size,
                egui::Button::new("Stop")
            ).clicked() {
                let _ = cmd_tx.send(BackgroundCommand::WriteRegs(vec![
                    ("PARAM_MOTOR_COMMAND".into(), 0.0),
                ]));
                ed.status_message = Some("Motor: Stopped".into());
            }
        });
    });
}
