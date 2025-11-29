use crate::state::SharedState;
use crate::modbus::writer::BackgroundCommand;

#[derive(Default)]
pub struct ConnectionState;

pub fn render(_ui: &mut eframe::egui::Ui, _state: &SharedState, _cmd_tx: &std::sync::mpsc::Sender<BackgroundCommand>, _cs: &mut ConnectionState) {}
