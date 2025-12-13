use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use rules_engine::s3_sink::{S3SinkConfig, spawn_s3_sink};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

// i hate how much boilerplate env parsing is in rust
fn env_u16(n: &str, def: u16) -> u16 {
    std::env::var(n).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(def)
}

fn env_string(n: &str, def: &str) -> String {
    std::env::var(n)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| def.to_string())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "mqtt_to_tcp_bridge=info".to_string()),
        )
        .init();

    let h = env_string("MQTT_HOST", "localhost");
    let p = env_u16("MQTT_PORT", 1883);
    let topic = env_string("MQTT_TOPIC_FILTER", "controller_app/events");

    let addr_str = env_string("TCP_BIND_ADDR", "0.0.0.0");
    let tp = env_u16("TCP_PORT", 50501);
    let addr: SocketAddr = format!("{addr_str}:{tp}").parse().expect("wont fail");

    // optional s3 sink for archiving
    let s3stuff = if let Some(cfg) = S3SinkConfig::from_env() {
        let aws_cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let c = aws_sdk_s3::Client::new(&aws_cfg);
        Some(spawn_s3_sink(cfg, c))
    } else {
        None
    };

    let (tx, _) = broadcast::channel::<Vec<u8>>(1024);
    let tx2 = tx.clone();

    // spawn mqtt listener
    tokio::spawn(async move {
        let cid = std::env::var("MQTT_CLIENT_ID").unwrap_or_else(|_| {
            format!("mqtt-to-tcp-bridge-{}", std::process::id())
        });
        let mut opts = MqttOptions::new(cid, h, p);
        opts.set_keep_alive(Duration::from_secs(10));
        let (client, mut evloop) = AsyncClient::new(opts, 10);

        if let Err(_e) = client.subscribe(topic.clone(), QoS::AtLeastOnce).await {
            return;
        }

        loop {
            match evloop.poll().await {
                Ok(Event::Incoming(Incoming::Publish(msg))) => {
                    // NES TCPSource expects newline-delimited JSON
                    let mut data = msg.payload.to_vec();
                    if !data.ends_with(b"\n") {
                        data.push(b'\n');
                    }
                    // println!("debug: bridge got {} bytes", data.len());
                    let _ = tx2.send(data.clone());
                    if let Some(ref s) = s3stuff {
                        let _ = s.send(data).await;
                    }
                }
                Ok(_) => {}
                Err(_e) => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    });

    // accept tcp connections and forward mqtt messages
    let lsnr = TcpListener::bind(addr).await?;

    loop {
        let (mut sock, _) = lsnr.accept().await?;
        let mut r = tx.subscribe();
        tokio::spawn(async move {
            loop {
                match r.recv().await {
                    Ok(v) => {
                        if sock.write_all(&v).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_n)) => continue,
                }
            }
            let _ = sock.shutdown().await;
        });
    }
}
