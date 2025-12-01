mod config_view;
mod connection;
pub mod loading;
mod live_data;
mod log_view;
mod serial_monitor;
mod setup_info;

use crate::state::SharedState;
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Connection,
    LiveData,
    Config,
    SerialMonitor,
    Log,
    Setup,
}

pub struct GatewayApp {
    state: SharedState,
    active_tab: Tab,
    cmd_tx: std::sync::mpsc::Sender<crate::modbus::writer::BackgroundCommand>,
    config_editor: config_view::ConfigEditorState,
    connection_state: connection::ConnectionState,
    setup_state: setup_info::SetupState,
    history_db: crate::storage::HistoryDb,
    chart_state: live_data::ChartState,
}

impl GatewayApp {
    pub fn new(
        state: SharedState,
        cmd_tx: std::sync::mpsc::Sender<crate::modbus::writer::BackgroundCommand>,
        history_db: crate::storage::HistoryDb,
    ) -> Self {
        Self {
            state,
            active_tab: Tab::Connection,
            cmd_tx,
            config_editor: config_view::ConfigEditorState::default(),
            connection_state: connection::ConnectionState::default(),
            setup_state: setup_info::SetupState::default(),
            history_db,
            chart_state: live_data::ChartState::default(),
        }
    }
}

pub static THEME_APPLIED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// 38px is the sweet spot for the 1024x600 display — still hittable
// with fingers but doesn't eat half the screen. The Apple HIG says 44
// but that's for phones where you hold it in one hand; on a desk-mounted
// Pi screen 38 is fine.
const TOUCH_MIN: f32 = 38.0;

const PAD: f32 = 8.0;
/// PAD_I8, used for egui Margin constructors that take i8.
#[allow(clippy::cast_possible_truncation, reason = "PAD is a small constant (8.0) that fits in i8")]
const PAD_I8: i8 = PAD as i8;
const DOT_R: f32 = 5.0;

// tried DOT_R=4.0 but the status dots were hard to see on
// the Waveshare 7" display at arm's length, 5.0 is better
// const _OLD_DOT_R: f32 = 4.0;

impl eframe::App for GatewayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 250ms repaint, tested on Pi 4 (VideoCore VI). Going below 200ms
        // causes visible frame drops when the chart tab has >10k points;
        // going above 300ms makes the status dots feel unresponsive.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        apply_theme(ctx);

        egui::TopBottomPanel::top("status_bar")
            .exact_height(28.0)
            .frame(egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(PAD_I8, 0))
                .fill(BG_SECONDARY)
                .stroke(egui::Stroke::new(1.0, BORDER)))
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let Ok(st) = self.state.read() else { return };
                    ui.label(egui::RichText::new(&st.gateway_id)
                        .strong().color(TEXT_DIM).size(13.0));
                    ui.separator();
                    status_pill(ui, &st.serial_status, "Serial");
                    ui.add_space(4.0);
                    status_pill(ui, &st.mqtt_status, "MQTT");
                });
            });

        // tab bar below
        egui::TopBottomPanel::top("tabs")
            .exact_height(TOUCH_MIN + 4.0)
            .frame(egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(PAD_I8, 4))
                .fill(BG_PRIMARY))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    touch_tab(ui, &mut self.active_tab, Tab::Connection, "Connect");
                    touch_tab(ui, &mut self.active_tab, Tab::LiveData, "Live");
                    touch_tab(ui, &mut self.active_tab, Tab::Config, "Config");
                    touch_tab(ui, &mut self.active_tab, Tab::Log, "Log");
                    touch_tab(ui, &mut self.active_tab, Tab::Setup, "Setup");
                    touch_tab(ui, &mut self.active_tab, Tab::SerialMonitor, "Raw");
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE
                .inner_margin(egui::Margin::same(PAD_I8))
                .fill(BG_PRIMARY))
            .show(ctx, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Body);
                match self.active_tab {
                    Tab::Connection => connection::render(
                        ui, &self.state, &self.cmd_tx, &mut self.connection_state,
                    ),
                    Tab::LiveData => live_data::render(
                        ui, &self.state, &self.history_db, &mut self.chart_state,
                    ),
                    Tab::Config => config_view::render(
                        ui, &self.state, &self.cmd_tx, &mut self.config_editor,
                    ),
                    Tab::SerialMonitor => serial_monitor::render(ui, &self.state),
                    Tab::Log => log_view::render(ui, &self.state),
                    Tab::Setup => setup_info::render(
                        ui, &self.state, &mut self.setup_state,
                    ),
                }
            });
    }
}

/// render a single tab button sized for touchscreen input.
fn touch_tab(ui: &mut egui::Ui, current: &mut Tab, tab: Tab, label: &str) {
    let active = *current == tab;
    let (bg, fg) = if active {
        (ACCENT, egui::Color32::WHITE)
    } else {
        (egui::Color32::TRANSPARENT, TEXT_DIM)
    };

    let btn = egui::Button::new(egui::RichText::new(label).size(13.0).color(fg))
        .fill(bg)
        .corner_radius(egui::CornerRadius::same(6))
        .stroke(egui::Stroke::NONE)
        // 36px is the actual hit area; the panel's 44px height adds
        // vertical padding so fingers still hit it reliably
        .min_size(egui::vec2(0.0, TOUCH_MIN - 8.0));

    if ui.add(btn).clicked() {
        *current = tab;
    }
}

fn status_pill(ui: &mut egui::Ui, status: &crate::state::ConnectionStatus, prefix: &str) {
    let color = match status {
        crate::state::ConnectionStatus::Connected => STATUS_GREEN,
        crate::state::ConnectionStatus::Connecting => STATUS_AMBER,
        crate::state::ConnectionStatus::Disconnected => egui::Color32::GRAY,
        crate::state::ConnectionStatus::Error(_) => STATUS_RED,
    };

    // painted circle instead of Unicode bullet, egui's built-in font
    // doesn't include U+2022 and renders a tofu box on Pi framebuffer.
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(DOT_R * 2.0, DOT_R * 2.0),
        egui::Sense::hover(),
    );
    ui.painter().circle_filled(rect.center(), DOT_R, color);
    ui.label(egui::RichText::new(format!("{prefix}: {status}"))
        .color(TEXT_DIM).size(12.0));
}

// ---------------------------------------------------------------------------
// color palette
// ---------------------------------------------------------------------------


const BG_CANVAS: egui::Color32 = egui::Color32::from_rgb(0xf5, 0xf5, 0xf7);
const BG_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xff, 0xff, 0xff);
const BG_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0xf9, 0xf9, 0xfb);
const BG_HOVER: egui::Color32 = egui::Color32::from_rgb(0xf0, 0xf0, 0xf2);

const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0x1d, 0x1d, 0x1f);
const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(0x86, 0x86, 0x8b);

const BORDER: egui::Color32 = egui::Color32::from_rgb(0xd4, 0xd4, 0xd4);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0xA0, 0xB0); 
const ACCENT_LIT: egui::Color32 = egui::Color32::from_rgb(0x2B, 0xB5, 0xC4);

#[allow(dead_code)]
const SELECTION: egui::Color32 = egui::Color32::from_rgba_premultiplied(0x00, 0xA0, 0xB0, 0x30);

const STATUS_GREEN: egui::Color32 = egui::Color32::from_rgb(0x34, 0xC7, 0x59);
const STATUS_AMBER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x95, 0x00);
const STATUS_RED: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x3B, 0x30);

#[allow(dead_code)] pub const TEXT_SECONDARY: egui::Color32 = TEXT_DIM;
#[allow(dead_code)] pub const TEXT_MUTED: egui::Color32 = TEXT_DIM;
#[allow(dead_code)] pub const BORDER_DEFAULT: egui::Color32 = BORDER;
#[allow(dead_code)] pub const ACCENT_HOVER: egui::Color32 = ACCENT_LIT;

pub fn configure_visuals(ctx: &egui::Context) { apply_theme(ctx); }

fn apply_theme(ctx: &egui::Context) {
    if !THEME_APPLIED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let mut v = egui::Visuals::light();
        v.panel_fill = BG_PRIMARY;
        v.window_fill = BG_SECONDARY;
        v.extreme_bg_color = BG_CANVAS;
        v.override_text_color = Some(TEXT_PRIMARY);
        v.selection.bg_fill = SELECTION;
        v.selection.stroke = egui::Stroke::new(1.0, ACCENT_LIT);
        v.hyperlink_color = ACCENT_LIT;

        // 6px radius matches Cloud Desktop's design tokens
        v.window_corner_radius = egui::CornerRadius::same(6);
        v.window_stroke = egui::Stroke::new(1.0, BORDER);
        v.window_shadow = egui::Shadow {
            offset: [0, 1], blur: 6, spread: 0,
            color: egui::Color32::from_black_alpha(20),
        };
        v.menu_corner_radius = egui::CornerRadius::same(6);
        v.popup_shadow = v.window_shadow;

        for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive] {
            w.bg_fill = BG_SECONDARY;
            w.bg_stroke = egui::Stroke::new(1.0, BORDER);
            w.fg_stroke = egui::Stroke::new(1.0, TEXT_DIM);
            w.corner_radius = egui::CornerRadius::same(6);
        }

        v.widgets.hovered.bg_fill = BG_HOVER;
        v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_LIT);
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
        v.widgets.hovered.corner_radius = egui::CornerRadius::same(6);

        v.widgets.active.bg_fill = ACCENT;
        v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT_LIT);
        v.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        v.widgets.active.corner_radius = egui::CornerRadius::same(6);

        v.widgets.open.bg_fill = BG_HOVER;
        v.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT);
        v.widgets.open.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
        v.widgets.open.corner_radius = egui::CornerRadius::same(6);

        v.faint_bg_color = BG_SECONDARY;
        ctx.set_visuals(v);
    }

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        // 30px  hght
        s.spacing.interact_size.y = 30.0;

        // 13px body
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
    });
}

/// standard card frame 
pub fn section_frame(_ui: &egui::Ui) -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_SECONDARY)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::same(PAD_I8))
        .stroke(egui::Stroke::new(1.0, BORDER))
}

/// wrapper
#[allow(dead_code, reason = "future")]
pub fn bare_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_SECONDARY)
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::same(8))
}
