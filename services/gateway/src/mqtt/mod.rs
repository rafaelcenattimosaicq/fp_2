// mQTT plumbing for Mosquitto broker running in local Docker container.
// the broker is started by docker.rs with a random hex password, credentials
// get injected into MqttConfig before we reach this code.
//
// nES worker also connects to the same broker via MQTT_SOURCE, so we publish
// telemetry to controller_app/events which it subscribes to.

#[allow(dead_code)] // auth module used during provisioning flow only
pub mod auth;
pub mod command_subscriber;
pub mod publisher;

use crate::config::MqttConfig;
use crate::device_descriptor::RegisterValue;
use crate::modbus::writer::BackgroundCommand;
use crate::state::{ConnectionStatus, LogLevel, SharedState};
use crate::telemetry::build_telemetry_json;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::time::Duration;

const MQTT_PORT_DEFAULT: u16 = 1883;
// rumqttc channel capacity, 10 is fine, we publish at most once per poll cycle
// (~1-5 seconds). If the channel fills up the broker is probably dead anyway.
const CHAN_CAP: usize = 10;

/// Parse broker URL into (host, port). Strips mqtt:// or tcp:// scheme.
///
/// FIXME: Docker on Linux sometimes gives us IPv6-mapped IPv4 like
/// `::ffff:172.17.0.2:1883`. The naive split-on-colon breaks because of the
/// colons in the IPv6 part. `rsplit_once` handles it because the port is always
/// the LAST colon-separated segment.
pub fn parse_broker_url(raw: &str) -> (String, u16) {
    let s = raw.strip_prefix("mqtt://")
        .or_else(|| raw.strip_prefix("tcp://"))
        .unwrap_or(raw);

    if s.is_empty() {
        tracing::warn!("empty broker URL, falling back to localhost");
        return ("localhost".into(), MQTT_PORT_DEFAULT);
    }

    // rsplit_once so IPv6 addresses don't confuse us
    match s.rsplit_once(':') {
        Some((h, p)) => {
            let port = p.parse::<u16>().unwrap_or(MQTT_PORT_DEFAULT);
            (h.to_string(), port)
        }
        None => (s.to_string(), MQTT_PORT_DEFAULT),
    }
}

/// fire up the rumqttc event loop on a background tokio task.
/// returns the client handle for publishing.
///
/// the 3-second reconnect delay is intentional, shorter values cause
/// duplicate client-id kicks because Mosquitto hasn't cleaned up the old
/// session yet. We hit this in production on the the client racks when the
/// pi was on flaky `WiFi`.
pub fn run_mqtt_loop(
    cfg: &MqttConfig,
    state: SharedState,
    cmd_tx: Sender<BackgroundCommand>,
    gw_id: String,
) -> AsyncClient {
    let (host, port) = parse_broker_url(&cfg.broker_url);

    let cid = state.read()
        .expect("state lock poisoned - modbus poller probably panicked")
        .gateway_id.clone();

    let mut opts = MqttOptions::new(&cid, &host, port);
    opts.set_keep_alive(Duration::from_secs(30));

    // credentials are optional, local dev mode runs without auth
    if let (Some(u), Some(p)) = (&cfg.username, &cfg.password) {
        opts.set_credentials(u, p);
    }

    {
        let mut st = state.write()
            .expect("state lock poisoned");
        st.mqtt_status = ConnectionStatus::Connecting;
        st.push_log(LogLevel::Info, format!("MQTT: connecting to {host}:{port} as {cid}"));
        drop(st);
    }

    let (client, mut evloop) = AsyncClient::new(opts, CHAN_CAP);
    let cl = client.clone();

    tokio::spawn(async move {
        loop {
            match evloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    tracing::info!("MQTT connected to broker");
                    {
                        let mut st = state.write().unwrap();
                        st.mqtt_status = ConnectionStatus::Connected;
                        st.push_log(LogLevel::Info, "MQTT connected");
                    }

                    // subscribe to remote write commands from Cloud Desktop
                    if let Err(e) = command_subscriber::subscribe_commands(&cl, &gw_id).await {
                        tracing::warn!("failed subscribing to command topic: {e}");
                    }
                }

                Ok(Event::Incoming(Packet::Publish(pub_msg))) => {
                    // TODO: maybe batch acks if we ever get >1 command per poll cycle
                    let ack = command_subscriber::handle_incoming_publish(
                        &pub_msg.topic, &pub_msg.payload, &gw_id, &cmd_tx,
                    );
                    if let Some(a) = ack {
                        let ack_topic = format!("commands/{gw_id}/write/ack");
                        match serde_json::to_vec(&a) {
                            Ok(bytes) => {
                                if let Err(e) = cl.publish(&ack_topic, QoS::AtLeastOnce, false, bytes).await {
                                    tracing::warn!("ack publish failed: {e}");
                                }
                            }
                            Err(e) => tracing::error!("BUG: couldn't serialize WriteAck: {e}"),
                        }
                    }
                }

                Ok(_) => {} // pingResp, SubAck, etc, don't care

                Err(e) => {
                    let msg = format!("{e}");
                    tracing::warn!("MQTT error: {msg}");
                    {
                        let mut st = state.write().unwrap();
                        st.mqtt_status = ConnectionStatus::Error(msg.clone());
                        st.push_log(LogLevel::Error, format!("MQTT error: {msg}"));
                        drop(st);
                    }
                    // 3s, see doc comment on this fn about duplicate client-id
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
    });

    client
}

/// map u8 qos level from config YAML to rumqttc enum.
/// anything other than 0 or 2 defaults to `QoS` 1 (`AtLeastOnce`) which is what
/// we want for telemetry, at-most-once drops data, exactly-once is overkill
/// for sensor readings.
pub const fn qos_from_u8(lvl: u8) -> QoS {
    match lvl {
        0 => QoS::AtMostOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtLeastOnce, // sane default for IoT telemetry
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
    let payload = build_telemetry_json(gw_id, dev_id, vals);
    let bytes = serde_json::to_vec(&payload)?;
    client.publish(topic, qos_from_u8(qos), false, bytes).await?;
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
