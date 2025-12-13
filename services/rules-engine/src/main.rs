use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use rules_engine::{evaluate, extract_device_id, Policy};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

fn env_string(s: &str, d: &str) -> String {
    std::env::var(s)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| d.to_string())
}

fn env_u16(s: &str, d: u16) -> u16 {
    std::env::var(s).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(d)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "rules_engine=info".to_string()),
        )
        .init();

    let host = env_string("MQTT_HOST", "localhost");
    let port = env_u16("MQTT_PORT", 1883);
    let pf = env_string("POLICY_TOPIC_FILTER", "policies/#");
    let tf = env_string("TELEMETRY_TOPIC_FILTER", "telemetry/#");

    let cid = format!("rules-engine-{}", std::process::id());
    let mut opts = MqttOptions::new(cid, &host, port);
    opts.set_keep_alive(Duration::from_secs(30));

    let (client, mut evloop) = AsyncClient::new(opts, 256);

    // mqtt spec says QoS 1 is enough for our use case
    client.subscribe(&pf, QoS::AtLeastOnce).await?;
    client.subscribe(&tf, QoS::AtLeastOnce).await?;

    let stuff: Arc<RwLock<HashMap<String, Policy>>> =
        Arc::new(RwLock::new(HashMap::new()));
    // generation counter so telemetry handlers pick up hot-reloaded policies
    let gen = Arc::new(AtomicU64::new(0));

    loop {
        match evloop.poll().await {
            Ok(Event::Incoming(Incoming::Publish(msg))) => {
                let t = &msg.topic;
                let data = &msg.payload;

                if t.starts_with(pf.trim_end_matches('#').trim_end_matches('/')) {
                    if let Some(did) =
                        extract_device_id(pf.trim_end_matches('#'), t)
                    {
                        if let Ok(val) = serde_json::from_slice::<Policy>(data) {
                            let _tmp = gen.fetch_add(1, Ordering::Relaxed) + 1;
                            // println!("debug: policy gen = {}", _tmp);
                            if let Ok(mut x) = stuff.write() {
                                x.insert(did.to_string(), val);
                            }
                        }
                    }
                    continue;
                }

                if let Some(did) =
                    extract_device_id(tf.trim_end_matches('#'), t)
                {
                    let tel: serde_json::Value = match serde_json::from_slice(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let res = if let Ok(r) = stuff.read() {
                        if let Some(item) = r.get(did) {
                            evaluate(item, did, &tel)
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    for thing in &res {
                        let tmp = serde_json::json!({
                            "device_id": thing.device_id,
                            "rule_id": thing.rule_id,
                        });
                        if let Ok(buf) = serde_json::to_vec(&tmp) {
                            let _ = client
                                .publish(&thing.publish_topic, QoS::AtLeastOnce, false, buf)
                                .await;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_e) => {
                // TODO: exponential backoff maybe?
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}
