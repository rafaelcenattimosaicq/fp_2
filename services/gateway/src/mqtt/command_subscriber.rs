use crate::modbus::writer::BackgroundCommand;
use rumqttc::{AsyncClient, QoS};
use std::sync::mpsc::Sender;

#[derive(serde::Serialize)]
pub struct WriteAck {
    pub register_id: String,
    pub success: bool,
    pub error: Option<String>,
}

pub async fn subscribe_commands(
    client: &AsyncClient, gw_id: &str,
) -> Result<(), rumqttc::ClientError> {
    let topic = format!("commands/{gw_id}/write");
    client.subscribe(&topic, QoS::AtLeastOnce).await
}

pub fn handle_incoming_publish(
    topic: &str,
    payload: &[u8],
    gw_id: &str,
    cmd_tx: &Sender<BackgroundCommand>,
) -> Option<WriteAck> {
    None // TODO: parse command and dispatch
}
