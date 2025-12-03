use crate::storage::HistoryDb;
use crate::state::SharedState;
use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::collections::HashSet;

// compressors sometimes skip a poll during defrost cycles
// (the MCU is busy with the valve actuator). 15s covers that gap
// without breaking the chart line on every defrost.
const GAP_SECS: f64 = 15.0;

const INTERVALS: &[(&str, i64)] = &[
    ("Live", 60), ("1m", 60), ("5m", 300), ("15m", 900),
    ("1h", 3600), ("6h", 6 * 3600), ("24h", 86400),
    ("7d", 7 * 86400), ("30d", 30 * 86400), ("90d", 90 * 86400),
];

const COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(0xef, 0x44, 0x44),
    egui::Color32::from_rgb(0x3b, 0x82, 0xf6),
    egui::Color32::from_rgb(0x22, 0xc5, 0x5e),
    egui::Color32::from_rgb(0xf5, 0x9e, 0x0b),
    egui::Color32::from_rgb(0xa8, 0x55, 0xf7),
    egui::Color32::from_rgb(0x06, 0xb6, 0xd4),
    egui::Color32::from_rgb(0xec, 0x48, 0x99),
    egui::Color32::from_rgb(0x84, 0xcc, 0x16),
    egui::Color32::from_rgb(0xf9, 0x73, 0x16),
    egui::Color32::from_rgb(0x64, 0x74, 0x8b),
    egui::Color32::from_rgb(0x14, 0xb8, 0xa6),
    egui::Color32::from_rgb(0xe1, 0x1d, 0x48),
];

// compressor sample rate is ~500ms; 800ms refresh keeps us
// under 2x without hammering the SQLite file
const REFRESH_MS: i64 = 800;
const MIN_H: f32 = 120.0;

#[derive(Debug)]
pub struct ChartState {
    interval: usize,
    selected: HashSet<String>,
    available: Vec<String>,
    last_reg_check: i64,
    auto_picked: bool,
    height: f32,
    cache: Option<CacheEntry>,
    cache_ts: i64,
    cache_interval: usize,
    cache_sel: HashSet<String>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    ids: Vec<String>,
    segments: Vec<Vec<Vec<[f64; 2]>>>,
    t_min: i64,
    t_max: i64,
    empty: bool,
}

impl Default for ChartState {
    fn default() -> Self {
        Self {
            interval: 0, selected: HashSet::new(), available: Vec::new(),
            last_reg_check: 0, auto_picked: false, height: 280.0,
            cache: None, cache_ts: 0, cache_interval: 0,
            cache_sel: HashSet::new(),
        }
    }
}

#[allow(clippy::too_many_lines, reason = "chart rendering requires sequential layout logic; splitting would hurt readability")]
pub fn render(ui: &mut egui::Ui, state: &SharedState, db: &HistoryDb, cs: &mut ChartState) {
    let now = chrono::Utc::now().timestamp_millis();

    // refresh the list of chartable registers every 5s
    if now - cs.last_reg_check > 5000 {
        cs.last_reg_check = now;
        let ids = state.read().ok()
            .map(|s| s.chart_register_ids.clone())
            .unwrap_or_default();
        cs.available = if ids.is_empty() {
            db.available_registers()
        } else {
            let mut v: Vec<String> = ids.into_iter().collect();
            v.sort();
            v
        };
        if !cs.auto_picked && !cs.available.is_empty() {
            for r in &cs.available { cs.selected.insert(r.clone()); }
            cs.auto_picked = true;
        }
    }

    if cs.interval == 0 {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(REFRESH_MS as u64));
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── interval buttons ────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Interval:").color(super::TEXT_MUTED).size(11.0));
            for (i, &(label, _)) in INTERVALS.iter().enumerate() {
                let active = cs.interval == i;
                let is_live = i == 0;

                let green = egui::Color32::from_rgb(0x22, 0xc5, 0x5e);
                let txt_col = if active { egui::Color32::WHITE }
                    else if is_live { green }
                    else { super::TEXT_MUTED };
                let fill = if active { if is_live { green } else { super::ACCENT } }
                    else { egui::Color32::TRANSPARENT };
                let stroke = if active { egui::Stroke::NONE }
                    else if is_live { egui::Stroke::new(1.0, green) }
                    else { egui::Stroke::new(1.0, super::BORDER_DEFAULT) };

                let btn = egui::Button::new(egui::RichText::new(label).size(11.0).color(txt_col))
                    .fill(fill).corner_radius(egui::CornerRadius::same(2)).stroke(stroke);
                if ui.add(btn).clicked() { cs.interval = i; }
            }
        });

        // ── register checkboxes ─────────────────────────────────
        if !cs.available.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Registers:").color(super::TEXT_MUTED).size(11.0));
                let regs = cs.available.clone();
                for (i, reg) in regs.iter().enumerate() {
                    let c = COLORS[i % COLORS.len()];
                    let mut on = cs.selected.contains(reg);
                    if ui.checkbox(&mut on, egui::RichText::new(reg).color(c).size(11.0)).changed() {
                        if on { cs.selected.insert(reg.clone()); } else { cs.selected.remove(reg); }
                    }
                }
            });
        }

        ui.add_space(4.0);

        // ── chart ───────────────────────────────────────────────
        refresh_cache(db, cs, now);
        let Some(c) = &cs.cache else { return };
        let is_live = cs.interval == 0;
        let (_, span_s) = INTERVALS[cs.interval];

        super::section_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            if c.empty {
                ui.set_min_height(cs.height);
                ui.vertical_centered(|ui| {
                    ui.add_space(cs.height / 2.0 - 20.0);
                    ui.label(egui::RichText::new("No data yet")
                        .color(super::TEXT_MUTED).size(13.0));
                });
                return;
            }

            let eff_span = if is_live { (c.t_max - c.t_min) / 1000 } else { span_s };

            let x_fmt = move |gm: egui_plot::GridMark, _: &std::ops::RangeInclusive<f64>| {
                use chrono::{Local, TimeZone};
                #[allow(clippy::cast_possible_truncation)]
                let secs = gm.value as i64;
                let Some(dt) = Local.timestamp_opt(secs, 0).single() else { return String::new() };
                if eff_span <= 3600 { dt.format("%H:%M:%S").to_string() }
                else if eff_span <= 86400 { dt.format("%H:%M").to_string() }
                else { dt.format("%m/%d %H:%M").to_string() }
            };

            #[allow(clippy::cast_precision_loss)]
            let x0 = if is_live {
                let s = (c.t_max - c.t_min) as f64 / 1000.0;
                c.t_min as f64 / 1000.0 - (s * 0.02).max(1.0)
            } else {
                (now - span_s * 1000) as f64 / 1000.0
            };
            #[allow(clippy::cast_precision_loss)]
            let x1 = {
                let edge = if is_live { c.t_max } else { now };
                let pad = if is_live { 2.0 } else { 0.0 };
                edge as f64 / 1000.0 + pad
            };

            let pid = format!("chart_{}_{}", cs.interval, cs.selected.len());
            let mut plot = Plot::new(pid)
                .height(cs.height)
                .show_axes([true, true]).show_grid([true, true])
                .allow_scroll(false)
                .allow_zoom(!is_live).allow_drag(!is_live).allow_boxed_zoom(!is_live)
                .x_axis_formatter(x_fmt)
                .include_x(x0).include_x(x1);
            if is_live { plot = plot.reset(); }
            if c.ids.len() <= 8 { plot = plot.legend(egui_plot::Legend::default()); }

            // TODO: this clone is expensive on 90d ranges. might need to
            // arc the segments or find a way to borrow into the closure.
            let ids = c.ids.clone();
            let segs = c.segments.clone();

            plot.show(ui, |pui| {
                for (ci, id) in ids.iter().enumerate() {
                    let col = COLORS[ci % COLORS.len()];
                    for (si, seg) in segs[ci].iter().enumerate() {
                        let pts: PlotPoints = seg.iter().copied().collect();
                        let line = Line::new(pts).color(col).width(1.5);
                        if si == 0 { pui.line(line.name(id.as_str())); }
                        else { pui.line(line); }
                    }
                }
            });
        });

        // resize handle
        let rect = ui.allocate_space(egui::vec2(ui.available_width(), 6.0)).1;
        let rid = ui.id().with("chart_resize");
        let resp = ui.interact(rect, rid, egui::Sense::drag());
        if resp.hovered() || resp.dragged() {
            ui.painter().rect_filled(rect, 0.0,
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 30));
            let cx = rect.center().x;
            let cy = rect.center().y;
            let gc = egui::Color32::from_gray(120);
            for dy in [-1.0_f32, 1.0] {
                ui.painter().line_segment(
                    [egui::pos2(cx - 20.0, cy + dy), egui::pos2(cx + 20.0, cy + dy)],
                    egui::Stroke::new(1.0, gc));
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.dragged() {
            cs.height = (cs.height + resp.drag_delta().y).max(MIN_H);
        }
    });
}

fn refresh_cache(db: &HistoryDb, cs: &mut ChartState, now: i64) {
    let sel_changed = cs.cache_interval != cs.interval || cs.cache_sel != cs.selected;
    let stale = cs.cache.is_none() || sel_changed || now - cs.cache_ts > REFRESH_MS;
    if !stale { return; }

    let is_live = cs.interval == 0;
    let (_, span_s) = INTERVALS[cs.interval];
    let start = if is_live { now - 86400 * 1000 } else { now - span_s * 1000 };

    let sel: Vec<String> = cs.selected.iter().cloned().collect();
    let rows = db.query_range(start, now, &sel);

    // for live mode, find the start of the current "session" by scanning
    // backwards for the most recent gap > GAP_SECS. this way the chart
    // shows data from the current connection, not stale points from before
    // a disconnect.
    let session_start = if is_live && !rows.is_empty() {
        #[allow(clippy::cast_possible_truncation)]
        let gap_ms = (GAP_SECS * 1000.0) as i64;
        let mut ts: Vec<i64> = rows.iter().map(|(t, _, _)| *t).collect();
        ts.sort_unstable();
        ts.dedup();
        let mut ss = ts[0];
        for w in ts.windows(2).rev() {
            if w[1] - w[0] > gap_ms { ss = w[1]; break; }
        }
        ss
    } else { 0 };

    let mut series: std::collections::HashMap<String, Vec<[f64; 2]>> = std::collections::HashMap::default();
    let (mut tmin, mut tmax) = (now, 0i64);

    for (t, id, val) in &rows {
        if is_live && *t < session_start { continue; }
        if *t < tmin { tmin = *t; }
        if *t > tmax { tmax = *t; }
        #[allow(clippy::cast_precision_loss)]
        series.entry(id.clone()).or_default().push([*t as f64 / 1000.0, *val]);
    }

    let mut ids: Vec<String> = series.keys().cloned().collect();
    ids.sort();
    let segments: Vec<Vec<Vec<[f64; 2]>>> = ids.iter()
        .map(|id| series.get(id).map(|pts| split_gaps(pts)).unwrap_or_default())
        .collect();

    cs.cache = Some(CacheEntry {
        empty: series.is_empty(), t_min: tmin, t_max: tmax, ids, segments,
    });
    cs.cache_ts = now;
    cs.cache_interval = cs.interval;
    cs.cache_sel = cs.selected.clone();
}

fn split_gaps(pts: &[[f64; 2]]) -> Vec<Vec<[f64; 2]>> {
    if pts.is_empty() { return vec![]; }
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut cur = vec![pts[0]];
    for w in pts.windows(2) {
        if w[1][0] - w[0][0] > GAP_SECS { out.push(std::mem::take(&mut cur)); }
        cur.push(w[1]);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}
