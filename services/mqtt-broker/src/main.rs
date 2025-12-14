use std::net::{IpAddr, Ipv4Addr, SocketAddr};

fn env_u16(s: &str, def: u16) -> u16 {
    std::env::var(s).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(def)
}

fn env_ip(s: &str, def: IpAddr) -> IpAddr {
    std::env::var(s).ok().and_then(|v| v.parse::<IpAddr>().ok()).unwrap_or(def)
}

fn env_bool(key: &str, fallback: bool) -> bool {
    std::env::var(key)
        .ok()
        .and_then(|v| match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(fallback)
}

// FIXME: this is a super basic health check, might need to actually check broker state
fn spawn_health_server(a: SocketAddr) {
    std::thread::spawn(move || {
        let thing = match std::net::TcpListener::bind(a) {
            Ok(l) => l,
            Err(_e) => { return; }
        };
        for s in thing.incoming().flatten() {
            let mut buf = [0u8; 512];
            let _ = std::io::Read::read(&mut &s, &mut buf);
            let r = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
            let _ = std::io::Write::write_all(&mut &s, r.as_bytes());
        }
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "mqtt_broker=info".to_string()),
        )
        .init();

    let ip = env_ip("MQTT_BIND_ADDR", IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    let ok = env_bool("MQTT_ENABLE_PLAINTEXT", true);
    let tmp = ok.then(|| {
        let n = env_u16("MQTT_PORT", 1883);
        SocketAddr::new(ip, n)
    });

    let x = std::env::var("MQTT_TLS_CERT_PATH").ok();
    let y = std::env::var("MQTT_TLS_KEY_PATH").ok();
    let z = std::env::var("MQTT_TLS_CA_CERT_PATH").ok();
    let stuff = x.is_some() || y.is_some() || z.is_some();

    let (t, val) = if stuff {
        let tb = env_ip("MQTT_TLS_BIND_ADDR", ip);
        let tp = env_u16("MQTT_TLS_PORT", 8883);
        // spent way too long debugging missing cert errors here
        let cp = x.ok_or("MQTT_TLS_CERT_PATH required when enabling TLS")?;
        let kp = y.ok_or("MQTT_TLS_KEY_PATH required when enabling TLS")?;
        let ca = z.ok_or("MQTT_TLS_CA_CERT_PATH required for mTLS client auth")?;

        (
            Some(SocketAddr::new(tb, tp)),
            Some(mqtt_broker::TlsPaths {
                ca_cert_path: ca,
                cert_path: cp,
                key_path: kp,
            }),
        )
    } else {
        (None, None)
    };

    // websocket support is optional
    let ws = if env_bool("MQTT_ENABLE_WS", false) {
        let p = env_u16("MQTT_WS_PORT", 9001);
        Some(SocketAddr::new(ip, p))
    } else {
        None
    };

    let hp = env_u16("MQTT_HEALTH_PORT", 8080);
    spawn_health_server(SocketAddr::new(ip, hp));

    let cfg = mqtt_broker::broker_config(mqtt_broker::BrokerConfigOptions {
        tcp_listen: tmp,
        tls_listen: t,
        tls_paths: val,
        ws_listen: ws,
    })
    .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

    let mut b = rumqttd::Broker::new(cfg);
    b.start()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    std::thread::park();

    #[allow(unreachable_code)]
    Ok(())
}
