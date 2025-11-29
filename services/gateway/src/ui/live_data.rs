use crate::state::SharedState;
use crate::storage::HistoryDb;

#[derive(Default)]
pub struct ChartState;

pub fn render(_ui: &mut eframe::egui::Ui, _state: &SharedState, _db: &HistoryDb, _cs: &mut ChartState) {}
