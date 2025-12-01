use crate::state::SharedState;
use eframe::egui;

#[derive(Default)]
pub struct SetupState {
    secret_buf: String,
    init: bool,
    save_ok: Option<bool>,
    save_err: String,
}

pub fn render(ui: &mut egui::Ui, state: &SharedState, ss: &mut SetupState) {
    if !ss.init {
        if let Ok(s) = state.read() { ss.secret_buf.clone_from(&s.vpn_secret); }
        ss.init = true;
    }
    let gw_id;
    let vpn_txt; let secret_ok; let cfg_path; let fp_data;
    {
        let Ok(s) = state.read() else { return };
        gw_id = s.gateway_id.clone();
        vpn_txt = format!("{}", s.vpn_status);
        secret_ok = s.vpn_secret_configured;
        cfg_path = s.config_path.clone();
        fp_data = s.fingerprint.clone();
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
    ui.heading("Device Setup");
    ui.label("Use this info when registering the gateway in Cloud Desktop.");
    ui.add_space(10.0);

    ui.strong("Gateway Identity");
    ui.horizontal(|ui| {
        ui.label("ID:");
        ui.label(egui::RichText::new(&gw_id).monospace().strong());
    });
    ui.separator();

    vpn_secret_section(ui, state, ss, &vpn_txt, secret_ok, &cfg_path);

    ui.add_space(8.0);
    fingerprint_section(ui, fp_data.as_ref());

    ui.add_space(8.0);
    ui.strong("How to Register");
    ui.add_space(4.0);
    ui.label("1. Open Cloud Desktop > Gateways > Registry");
    ui.label("2. Click + Register Gateway");
    ui.label("3. Enter the Gateway ID above (exact match!)");
    ui.label("4. Optionally enter fingerprint values");
    ui.label("5. Copy the secret (shown only once!)");
    ui.label("6. Paste it above, click Save");
    ui.label("7. Go to Connection tab > Retry VPN");
    }); // scroll
}

// this section got complicated created a function
fn vpn_secret_section(
    ui: &mut egui::Ui,
    state: &SharedState,
    ss: &mut SetupState,
    vpn_txt: &str, configured: bool,
    yaml_path: &str,
) {
    super::section_frame(ui).show(ui, |ui| {
    ui.set_min_width(ui.available_width());
    ui.strong("VPN Configuration");
    ui.add_space(4.0);
    ui.horizontal(|ui| { ui.label("VPN Status:"); ui.label(vpn_txt); });
    ui.horizontal(|ui| {
        ui.label("Secret:");
        if configured { ui.colored_label(super::STATUS_GREEN, "configured"); }
        else { ui.colored_label(super::STATUS_RED, "not set"); }
    });
    ui.add_space(8.0);
    // secret is a one-time base64 token
    ui.label("Pre-Shared Secret:");
    let input_w = (ui.available_width() - 60.0).max(200.0);
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut ss.secret_buf)
            .hint_text("paste from Cloud Desktop")
            .desired_width(input_w)
            .font(egui::TextStyle::Monospace));
        if ui.button("Save").clicked() {
            let val = ss.secret_buf.trim();
            if val.is_empty() {
                ss.save_ok = Some(false);
                ss.save_err = "can't be empty".into();
                return;
            }
            // double lock acquisition is ugly but the borrow checker
            // won't let me combine them (ss is &mut from the closure)
            state.write().unwrap().vpn_secret = val.to_string();
            state.write().unwrap().vpn_secret_configured = true;

            match patch_yaml(yaml_path, val) {
                Ok(()) => {
                    ss.save_ok = Some(true);
                    ss.save_err.clear();
                }
                Err(e) => {
                    ss.save_ok = Some(false);
                    ss.save_err = e.to_string();
                }
            }
        }
    });
    match ss.save_ok {
        Some(true) => { ui.colored_label(super::STATUS_GREEN, "saved - hit Retry VPN on Connection tab"); }
        Some(false) => { ui.colored_label(super::STATUS_RED, &ss.save_err); }
        None => {}
    }
    });
}

// fingerprint weights MUST stay in sync with the plambda
fn fingerprint_section(
    ui: &mut egui::Ui,
    fp_data: Option<&crate::vpn::fingerprint::HardwareFingerprint>,
) {
    super::section_frame(ui).show(ui, |ui| {
    ui.set_min_width(ui.available_width());
    ui.strong("Hardware Fingerprint");
    ui.label(egui::RichText::new("Enter these in Cloud Desktop for trust scoring.").weak());
    ui.add_space(4.0);
    let Some(fp) = fp_data else { ui.label("waiting for hardware probe..."); return; };
    ui.horizontal(|ui| { ui.label("MAC Address (w=30):"); ui.label(egui::RichText::new(&fp.mac_address).monospace()); });
    ui.horizontal(|ui| { ui.label("CPU Serial (w=25):"); ui.label(egui::RichText::new(&fp.cpu_id).monospace()); });
    ui.horizontal(|ui| {
        ui.label("Board Serial (w=20):");
        if fp.board_serial.is_empty() {
            ui.colored_label(egui::Color32::GRAY, "n/a");
        } else {
            ui.label(egui::RichText::new(&fp.board_serial).monospace());
        }
    });
    ui.horizontal(|ui| { ui.label("Hostname (w=15):"); ui.label(egui::RichText::new(&fp.hostname).monospace()); });
    ui.horizontal(|ui| {
        ui.label("OS (w=10):");
        // os_info can be long in spme rapberry pies
        let os = if fp.os_info.len() > 40 { &fp.os_info[..40] } else { &fp.os_info };
        ui.label(egui::RichText::new(os).monospace());
    });
    });
}

fn patch_yaml(path: &str, secret: &str) -> Result<(), Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(path)?;
    let mut lines: Vec<String> = raw.lines().map(String::from).collect();
    let mut done = false;
    for line in &mut lines {
        let t = line.trim_start();
        if t.starts_with("pre_shared_secret:") {
            let indent = &line[..line.len() - t.len()];
            *line = format!("{indent}pre_shared_secret: \"{secret}\"");
            done = true;
            break;
        }
    }
    if !done {
        if let Some(i) = lines.iter().position(|l| l.trim_start().starts_with("vpn:")) {
            lines.insert(i + 1, format!("  pre_shared_secret: \"{secret}\""));
        } else {
            lines.push(String::new());
            lines.push("vpn:".into());
            lines.push(format!("  pre_shared_secret: \"{secret}\""));
        }
    }
    let mut out = lines.join("\n");
    if raw.ends_with('\n') && !out.ends_with('\n') { out.push('\n'); }
    std::fs::write(path, out)?;
    Ok(())
}
