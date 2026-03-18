#[allow(dead_code)]
pub mod auth;
pub mod command_subscriber;
pub mod publisher;

use crate::config::MqttConfig;
use crate::device_descriptor::RegisterValue;
use crate::modbus::writer::BackgroundCommand;
use crate::state::{ConnectionStatus, LogLevel, SharedState};
use crate::telemetry::build_telemetry_json;
#[allow(unused_imports)]
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::time::Duration;

const MQTT_PORT_DEFAULT: u16 = 1883;
const CHAN_CAP: usize = 10;

pub fn parse_broker_url(raw: &str) -> (String, u16) {
    let s = raw.strip_prefix("mqtt://")
        .or_else(|| raw.strip_prefix("tcp://"))
        .unwrap_or(raw);

    if s.is_empty() { return ("localhost".into(), MQTT_PORT_DEFAULT); }

    match s.rsplit_once(':') {
        Some((h, p)) => {
            let port = p.parse::<u16>().unwrap_or(MQTT_PORT_DEFAULT);
            (h.to_string(), port)
        }
        None => (s.to_string(), MQTT_PORT_DEFAULT),
    }
}

pub fn run_mqtt_loop(
    cfg: &MqttConfig,
    state: SharedState,
    cmd_tx: Sender<BackgroundCommand>,
    gw_id: String,
) -> (AsyncClient, command_subscriber::RuleStore) {
    let rules = command_subscriber::new_rule_store();
    let rules_clone = rules.clone();
    run_inner(cfg, state, cmd_tx, gw_id, rules_clone)
}

fn run_inner(
    cfg: &MqttConfig,
    state: SharedState,
    cmd_tx: Sender<BackgroundCommand>,
    gw_id: String,
    rules: command_subscriber::RuleStore,
) -> (AsyncClient, command_subscriber::RuleStore) {
    let (h, p) = parse_broker_url(&cfg.broker_url);

    let id = state.read()
        .expect("state lock poisoned - modbus poller probably panicked")
        .gateway_id.clone();

    let mut o = MqttOptions::new(&id, &h, p);
    o.set_keep_alive(Duration::from_secs(30));

    if let (Some(u), Some(pw)) = (&cfg.username, &cfg.password) {
        o.set_credentials(u, pw);
    }

    {
        let mut s = state.write().expect("state lock poisoned");
        s.mqtt_status = ConnectionStatus::Connecting;
        s.push_log(LogLevel::Info, format!("MQTT: connecting to {h}:{p} as {id}"));
        drop(s);
    }

    let (cl, mut ev) = AsyncClient::new(o, CHAN_CAP);
    let c2 = cl.clone();
    let r2 = rules.clone();

    tokio::spawn(async move {
        loop {
            match ev.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    {
                        let mut s = state.write().unwrap();
                        s.mqtt_status = ConnectionStatus::Connected;
                        s.push_log(LogLevel::Info, "MQTT connected");
                    }
                    let _ = command_subscriber::subscribe_commands(&c2, &gw_id).await;
                }

                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    let x = command_subscriber::handle_incoming_publish(
                        &msg.topic, &msg.payload, &gw_id, &cmd_tx, &rules,
                    );
                    if let Some(a) = x {
                        let t = format!("commands/{gw_id}/write/ack");
                        if let Ok(b) = serde_json::to_vec(&a) {
                            let _ = c2.publish(&t, QoS::AtLeastOnce, false, b).await;
                        }
                    }
                }

                Ok(_) => {} // pingResp, SubAck, etc

                Err(e) => {
                    let tmp = format!("{e}");
                    {
                        let mut s = state.write().unwrap();
                        s.mqtt_status = ConnectionStatus::Error(tmp.clone());
                        s.push_log(LogLevel::Error, format!("MQTT error: {tmp}"));
                        drop(s);
                    }
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    });

    (cl, r2)
}

pub const fn qos_from_u8(lvl: u8) -> QoS {
    match lvl {
        0 => QoS::AtMostOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtLeastOnce,
    }
}

pub async fn publish_telemetry(
    client: &AsyncClient,
    topic: &str,
    qos: u8,
    gw_id: &str,
    dev_id: &str,
    vals: &HashMap<String, RegisterValue>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let x = build_telemetry_json(gw_id, dev_id, vals);
    let buf = serde_json::to_vec(&x)?;
    client.publish(topic, qos_from_u8(qos), false, buf).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: a user's YAML had tcp:// prefix instead of mqtt:// and we panicked.
    #[test]
    fn parse_various_broker_urls() {
        let (h, p) = parse_broker_url("mqtt://broker.local:1883");
        assert_eq!(h, "broker.local");
        assert_eq!(p, 1883);

        let (h, p) = parse_broker_url("tcp://10.0.0.1:8883");
        assert_eq!(h, "10.0.0.1");
        assert_eq!(p, 8883);

        // bare host:port, no scheme
        let (h, p) = parse_broker_url("mybroker:9999");
        assert_eq!(h, "mybroker");
        assert_eq!(p, 9999);

        // no port at all
        let (h, p) = parse_broker_url("standalone-broker");
        assert_eq!(h, "standalone-broker");
        assert_eq!(p, MQTT_PORT_DEFAULT);
    }

    #[test]
    fn empty_url_gives_localhost() {
        let (h, p) = parse_broker_url("");
        assert_eq!(h, "localhost");
        assert_eq!(p, MQTT_PORT_DEFAULT);
    }

    #[test]
    fn qos_mapping() {
        assert_eq!(qos_from_u8(0), QoS::AtMostOnce);
        assert_eq!(qos_from_u8(1), QoS::AtLeastOnce);
        assert_eq!(qos_from_u8(2), QoS::ExactlyOnce);
        // garbage values should fallback to QoS 1
        assert_eq!(qos_from_u8(3), QoS::AtLeastOnce);
        assert_eq!(qos_from_u8(255), QoS::AtLeastOnce);
    }
}
