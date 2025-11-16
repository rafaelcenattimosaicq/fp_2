// handles remote register-write commands from Cloud Desktop.
//
// flow: Cloud Desktop UI -> cloud API -> MQTT broker -> this subscriber
// topic: commands/{gateway_id}/write  (QoS 1)
// ack:   commands/{gateway_id}/write/ack
//
// the write command contains a Modbus register name (e.g. PARAM_TH_SETPOINT)
// and a float value. We forward it to the background Modbus writer thread
// via the cmd_tx channel, which does the actual RTU write.

use crate::modbus::writer::BackgroundCommand;
use rumqttc::{AsyncClient, QoS};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;

/// incoming write command from Cloud Desktop.
/// `device_id` is included for routing but we don't use it yet -
/// the gateway only talks to one device at a time via RS-485.
#[derive(Debug, Clone, Deserialize)]
pub struct WriteCommand {
    pub request_id: String,
    #[allow(dead_code)] // needed for multi-device support later
    pub device_id: String,
    pub register_id: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WriteAck {
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn subscribe_commands(
    client: &AsyncClient, gw_id: &str,
) -> Result<(), rumqttc::ClientError> {
    let topic = format!("commands/{gw_id}/write");
    client.subscribe(&topic, QoS::AtLeastOnce).await
}

/// process an incoming MQTT publish. Returns Some(ack) if the topic matches
/// our command topic, None if it's for a different gateway or unrelated topic.
pub fn handle_incoming_publish(
    topic: &str,
    payload: &[u8],
    gw_id: &str,
    cmd_tx: &Sender<BackgroundCommand>,
) -> Option<WriteAck> {
    // quick check, avoid parsing JSON for messages that aren't for us
    let expected = format!("commands/{gw_id}/write");
    if topic != expected { return None; }

    let cmd: WriteCommand = match serde_json::from_slice(payload) {
        Ok(c) => c,
        Err(e) => {
            // bad JSON from the cloud, shouldn't happen but has happened
            // when someone manually publishes to the topic for debugging
            return Some(WriteAck {
                request_id: String::new(),
                success: false,
                error: Some(format!("bad command: {e}")),
            });
        }
    };

    let req_id = cmd.request_id;
    let writes = vec![(cmd.register_id, cmd.value)];

    // forward to the Modbus writer thread. If the channel is closed the
    // writer panicked, probably USB-serial adapter got yanked.
    match cmd_tx.send(BackgroundCommand::WriteRegs(writes)) {
        Ok(()) => Some(WriteAck { request_id: req_id, success: true, error: None }),
        Err(e) => Some(WriteAck {
            request_id: req_id,
            success: false,
            error: Some(format!("send error: {e}")),
        }),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // real command from Cloud Desktop when user adjusts thermostat setpoint
    #[test]
    fn parse_setpoint_write_command() {
        let json = r#"{
            "request_id": "req-001",
            "device_id": "0x0007",
            "register_id": "PARAM_TH_SETPOINT",
            "value": 25.5
        }"#;
        let cmd: WriteCommand = serde_json::from_str(json).unwrap();

        assert_eq!(cmd.request_id, "req-001");
        assert_eq!(cmd.device_id, "0x0007");
        assert_eq!(cmd.register_id, "PARAM_TH_SETPOINT");
        assert!((cmd.value - 25.5).abs() < f64::EPSILON);
    }

    // missing `value` field, serde should reject
    #[test]
    fn rejects_incomplete_command() {
        let no_value = r#"{
            "request_id": "req-002",
            "device_id": "0x0007",
            "register_id": "PARAM_TH_SETPOINT"
        }"#;
        assert!(serde_json::from_str::<WriteCommand>(no_value).is_err());

        // also reject when register_id is missing
        let no_reg = r#"{"request_id": "req-003", "device_id": "0x0007"}"#;
        assert!(serde_json::from_str::<WriteCommand>(no_reg).is_err());
    }

    #[test]
    fn ack_serialization_omits_null_error() {
        let ok_ack = WriteAck { request_id: "req-001".into(), success: true, error: None };
        let json = serde_json::to_string(&ok_ack).unwrap();
        assert!(json.contains(r#""success":true"#));
        assert!(json.contains(r#""request_id":"req-001""#));
        // serde skip_serializing_if should omit the error key entirely
        assert!(!json.contains("error"));
    }

    #[test]
    fn ack_serialization_includes_error() {
        let err_ack = WriteAck {
            request_id: "req-002".into(),
            success: false,
            error: Some("Device not found".into()),
        };
        let json = serde_json::to_string(&err_ack).unwrap();
        assert!(json.contains(r#""success":false"#));
        assert!(json.contains("Device not found"));
    }

    // happy path: matching topic, valid JSON, writer channel open
    #[test]
    fn dispatches_write_to_modbus_thread() {
        let (tx, rx) = mpsc::channel();
        let gw = "gw-test-01";
        let payload = br#"{
            "request_id": "req-100",
            "device_id": "0x0007",
            "register_id": "PARAM_TH_SETPOINT",
            "value": 30.0
        }"#;

        let ack = handle_incoming_publish("commands/gw-test-01/write", payload, gw, &tx)
            .expect("should produce ack for matching topic");
        assert!(ack.success);
        assert_eq!(ack.request_id, "req-100");

        // verify the BackgroundCommand actually arrived
        match rx.try_recv().unwrap() {
            BackgroundCommand::WriteRegs(w) => {
                assert_eq!(w.len(), 1);
                assert_eq!(w[0].0, "PARAM_TH_SETPOINT");
                assert!((w[0].1 - 30.0).abs() < f64::EPSILON);
            }
            other => panic!("expected WriteRegs, got {other:?}"),
        }
    }

    // topic for a different gateway, should be silently ignored
    #[test]
    fn ignores_other_gateways() {
        let (tx, _rx) = mpsc::channel();
        assert!(handle_incoming_publish("commands/other-gw/write", b"{}", "gw-test-01", &tx).is_none());
    }

    // completely different topic (e.g. telemetry)
    #[test]
    fn ignores_unrelated_topics() {
        let (tx, _) = mpsc::channel();
        let r = handle_incoming_publish("telemetry/gw-test-01/data", b"{}", "gw-test-01", &tx);
        assert!(r.is_none());
    }

    // garbage payload, should return error ack, not panic
    #[test]
    fn bad_json_returns_error_ack() {
        let (tx, _) = mpsc::channel();
        let ack = handle_incoming_publish(
            "commands/gw-test-01/write", b"not json", "gw-test-01", &tx,
        ).unwrap();
        assert!(!ack.success);
        assert!(ack.error.is_some());
        // request_id is empty because we couldn't even parse it
        assert!(ack.request_id.is_empty());
    }
}
