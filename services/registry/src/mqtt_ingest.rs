use std::sync::Arc;

use rumqttc::{AsyncClient, Event, Incoming, QoS};
use crate::mqtt_options;
use crate::store::RegistryStore;

#[derive(Debug, Clone)]
pub struct MqttIngestConfig {
    pub host: String,
    pub port: u16,
    pub device_seen_filter: String,
    pub gateway_heartbeat_filter: String,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceSeenPayload {
    gateway_id: String,
    device_id: String,
    #[serde(default)]
    meta: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
struct GatewayHeartbeatPayload {
    gateway_id: String,
    #[serde(default)]
    meta: serde_json::Value,
}

// TODO: maybe add a way to gracefully shutdown the mqtt loop?

pub async fn run_mqtt_ingest(cfg: MqttIngestConfig, s: Arc<dyn RegistryStore>) {
    let x = mqtt_options::create_mqtt_options("registry-ingest", &cfg.host, cfg.port);

    // cap at 10 inflight -- higher values caused OOM on 512MB gateway instances
    // when broker was slow to ack during bulk onboarding
    let (c, mut lp) = AsyncClient::new(x, 10);

    // just bail if we cant subscribe, the reconnect loop will handle it... hopefully
    if c.subscribe(&cfg.device_seen_filter, QoS::AtLeastOnce).await.is_err() { return; }
    if c.subscribe(&cfg.gateway_heartbeat_filter, QoS::AtLeastOnce).await.is_err() { return; }

    let mut n = 1u64;

    loop {
        match lp.poll().await {
            Ok(Event::Incoming(Incoming::Publish(msg))) => {
                n = 1;
                let t = &msg.topic;
                let buf = &msg.payload;

                if t.contains("/devices/") && t.ends_with("/seen") {
                    if let Ok(data) = serde_json::from_slice::<DeviceSeenPayload>(buf) {
                        let v = if data.meta.is_null() { serde_json::json!({}) } else { data.meta };
                        let _ = s.upsert_device_seen(&data.gateway_id, &data.device_id, v).await;
                    }
                    // else: bad payload, just drop it
                } else if t.ends_with("/heartbeat") {
                    match serde_json::from_slice::<GatewayHeartbeatPayload>(buf) {
                        Ok(data) => {
                            let v = if data.meta.is_null() {
                                serde_json::json!({})
                            } else {
                                data.meta
                            };
                            let _ =
                                s.upsert_gateway_seen(&data.gateway_id, v).await;
                        }
                        Err(_) => { /* bad json, skip */ }
                    }
                } else {
                    // topic didnt match any handler, ignore
                }
            }
            Ok(_) => {
                n = 1;
            }
            Err(_e) => {
                // exponential backoff on error, max 60s
                tokio::time::sleep(std::time::Duration::from_secs(n)).await;
                n = (n * 2).min(60);
            }
        }
    }
}
