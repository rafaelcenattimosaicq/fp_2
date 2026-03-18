// handles remote register-write commands + auto-action rules from NES alerts

use crate::modbus::writer::BackgroundCommand;
use rumqttc::{AsyncClient, QoS};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Deserialize)]
pub struct WriteCommand {
    pub request_id: String,
    #[allow(dead_code)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionRule {
    pub rule_id: String,
    pub register_id: String,
    pub value: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    pub action: String,
    pub rule: ActionRule,
}

pub type RuleStore = Arc<RwLock<Vec<ActionRule>>>;

pub fn new_rule_store() -> RuleStore { Arc::new(RwLock::new(Vec::new())) }

pub async fn subscribe_commands(
    client: &AsyncClient, gw_id: &str,
) -> Result<(), rumqttc::ClientError> {
    let write_topic = format!("commands/{gw_id}/write");
    let rule_topic = format!("commands/{gw_id}/rule");
    client.subscribe(&write_topic, QoS::AtLeastOnce).await?;
    client.subscribe(&rule_topic, QoS::AtLeastOnce).await?;
    client.subscribe("nebulastream/alerts/#", QoS::AtLeastOnce).await?;
    Ok(())
}

pub fn handle_incoming_publish(
    topic: &str,
    payload: &[u8],
    gw_id: &str,
    cmd_tx: &Sender<BackgroundCommand>,
    rules: &RuleStore,
) -> Option<WriteAck> {
    let t1 = format!("commands/{gw_id}/write");
    if topic == t1 {
        return handle_write(payload, cmd_tx);
    }

    let t2 = format!("commands/{gw_id}/rule");
    if topic == t2 {
        handle_rule_cfg(payload, rules);
        return None;
    }

    if topic.starts_with("nebulastream/alerts/") {
        let rid = topic.rsplit('/').next().unwrap_or("");
        process_alert(rid, payload, cmd_tx, rules);
        return None;
    }

    None
}

fn handle_write(
    payload: &[u8],
    cmd_tx: &Sender<BackgroundCommand>,
) -> Option<WriteAck> {
    let x: WriteCommand = match serde_json::from_slice(payload) {
        Ok(c) => c,
        Err(e) => {
            return Some(WriteAck {
                request_id: String::new(),
                success: false,
                error: Some(format!("bad command: {e}")),
            });
        }
    };

    let r = x.request_id;
    let v = vec![(x.register_id, x.value)];

    match cmd_tx.send(BackgroundCommand::WriteRegs(v)) {
        Ok(()) => Some(WriteAck { request_id: r, success: true, error: None }),
        Err(e) => Some(WriteAck {
            request_id: r,
            success: false,
            error: Some(format!("send error: {e}")),
        }),
    }
}

fn handle_rule_cfg(payload: &[u8], rules: &RuleStore) {
    let tmp: RuleConfig = match serde_json::from_slice(payload) {
        Ok(c) => c,
        Err(_e) => { return; }
    };

    let mut s = rules.write().expect("rule store lock");
    match tmp.action.as_str() {
        "add" => {
            s.retain(|r| r.rule_id != tmp.rule.rule_id);
            s.push(tmp.rule);
        }
        "remove" => {
            s.retain(|r| r.rule_id != tmp.rule.rule_id);
        }
        _other => {}
    }
}

fn process_alert(
    rule_id: &str,
    _payload: &[u8],
    cmd_tx: &Sender<BackgroundCommand>,
    rules: &RuleStore,
) {
    let s = rules.read().expect("rule store lock");
    let Some(item) = s.iter().find(|r| r.rule_id == rule_id) else {
        return;
    };

    let v = vec![(item.register_id.clone(), item.value)];
    let _ = cmd_tx.send(BackgroundCommand::WriteRegs(v));
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

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
        assert_eq!(cmd.register_id, "PARAM_TH_SETPOINT");
    }

    #[test]
    fn dispatches_write_to_modbus_thread() {
        let (tx, rx) = mpsc::channel();
        let rules = new_rule_store();
        let payload = br#"{
            "request_id": "req-100",
            "device_id": "0x0007",
            "register_id": "PARAM_TH_SETPOINT",
            "value": 30.0
        }"#;

        let ack = handle_incoming_publish("commands/gw-test/write", payload, "gw-test", &tx, &rules)
            .expect("should produce ack");
        assert!(ack.success);

        match rx.try_recv().unwrap() {
            BackgroundCommand::WriteRegs(w) => {
                assert_eq!(w[0].0, "PARAM_TH_SETPOINT");
            }
            other => panic!("expected WriteRegs, got {other:?}"),
        }
    }

    #[test]
    fn rule_add_and_alert_triggers_write() {
        let (tx, rx) = mpsc::channel();
        let rules = new_rule_store();

        // Add a rule
        let rule_cfg = br#"{"action":"add","rule":{"rule_id":"r1","register_id":"PARAM_MOTOR_COMMAND","value":3}}"#;
        handle_incoming_publish("commands/gw-test/rule", rule_cfg, "gw-test", &tx, &rules);

        assert_eq!(rules.read().unwrap().len(), 1);

        // Simulate NES alert
        handle_incoming_publish("nebulastream/alerts/r1", b"{}", "gw-test", &tx, &rules);

        match rx.try_recv().unwrap() {
            BackgroundCommand::WriteRegs(w) => {
                assert_eq!(w[0].0, "PARAM_MOTOR_COMMAND");
                assert!((w[0].1 - 3.0).abs() < f64::EPSILON);
            }
            other => panic!("expected WriteRegs, got {other:?}"),
        }
    }

    #[test]
    fn unknown_alert_ignored() {
        let (tx, rx) = mpsc::channel();
        let rules = new_rule_store();

        handle_incoming_publish("nebulastream/alerts/unknown-rule", b"{}", "gw-test", &tx, &rules);
        assert!(rx.try_recv().is_err(), "should not send anything");
    }

    #[test]
    fn rule_remove() {
        let (tx, _rx) = mpsc::channel();
        let rules = new_rule_store();

        let add = br#"{"action":"add","rule":{"rule_id":"r1","register_id":"X","value":1}}"#;
        handle_incoming_publish("commands/gw-test/rule", add, "gw-test", &tx, &rules);
        assert_eq!(rules.read().unwrap().len(), 1);

        let remove = br#"{"action":"remove","rule":{"rule_id":"r1","register_id":"X","value":1}}"#;
        handle_incoming_publish("commands/gw-test/rule", remove, "gw-test", &tx, &rules);
        assert_eq!(rules.read().unwrap().len(), 0);
    }
}
