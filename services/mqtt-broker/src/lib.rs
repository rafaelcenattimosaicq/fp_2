use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct TlsPaths {
    pub ca_cert_path: String,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct BrokerConfigOptions {
    pub tcp_listen: Option<SocketAddr>,
    pub tls_listen: Option<SocketAddr>,
    pub tls_paths: Option<TlsPaths>,
    pub ws_listen: Option<SocketAddr>,
}

#[allow(dead_code)]
const DEFAULT_MAX_PAYLOAD: usize = 1024 * 1024; // 1MB

pub fn broker_config(data: BrokerConfigOptions) -> Result<rumqttd::Config, String> {
    let mut stuff = std::collections::HashMap::new();

    if let Some(addr) = data.tcp_listen {
        stuff.insert(
            "tcp".to_string(),
            rumqttd::ServerSettings {
                name: "tcp".to_string(),
                listen: addr,
                tls: None,
                next_connection_delay_ms: 0,
                connections: rumqttd::ConnectionSettings {
                    connection_timeout_ms: 5_000,
                    max_payload_size: DEFAULT_MAX_PAYLOAD,
                    max_inflight_count: 100,
                    auth: None,
                    external_auth: None,
                    dynamic_filters: false,
                },
            },
        );
    }

    if let Some(addr) = data.tls_listen {
        let tp = data.tls_paths
            .ok_or_else(|| "tls_listen set but tls_paths missing".to_string())?;

        stuff.insert(
            "tls".to_string(),
            rumqttd::ServerSettings {
                name: "tls".to_string(),
                listen: addr,
                tls: Some(rumqttd::TlsConfig::Rustls {
                    capath: Some(tp.ca_cert_path),
                    certpath: tp.cert_path,
                    keypath: tp.key_path,
                }),
                next_connection_delay_ms: 0,
                connections: rumqttd::ConnectionSettings {
                    connection_timeout_ms: 5_000,
                    max_payload_size: DEFAULT_MAX_PAYLOAD,
                    max_inflight_count: 100,
                    auth: None,
                    external_auth: None,
                    dynamic_filters: false,
                },
            },
        );
    }

    if stuff.is_empty() {
        return Err("no listeners configured (enable TCP and/or TLS)".to_string());
    }

    // websocket listener
    let ret = data.ws_listen.map(|a| {
        let mut tmp = std::collections::HashMap::new();
        tmp.insert(
            "ws".to_string(),
            rumqttd::ServerSettings {
                name: "ws".to_string(),
                listen: a,
                tls: None,
                next_connection_delay_ms: 0,
                connections: rumqttd::ConnectionSettings {
                    connection_timeout_ms: 5_000,
                    max_payload_size: DEFAULT_MAX_PAYLOAD,
                    max_inflight_count: 100,
                    auth: None,
                    external_auth: None,
                    dynamic_filters: false,
                },
            },
        );
        tmp
    });

    // rumqttd needs v5 listeners on port+1, not sure why they did it this way
    let mut v5_map = std::collections::HashMap::new();
    for (k, item) in &stuff {
        let x = SocketAddr::new(item.listen.ip(), item.listen.port() + 1);
        v5_map.insert(
            format!("{k}_v5"),
            rumqttd::ServerSettings {
                name: format!("{}_v5", item.name),
                listen: x,
                tls: item.tls.clone(),
                next_connection_delay_ms: item.next_connection_delay_ms,
                connections: item.connections.clone(),
            },
        );
    }

    Ok(rumqttd::Config {
        id: 0,
        router: rumqttd::RouterConfig {
            max_connections: 10_000,
            max_outgoing_packet_count: 10_000,
            max_segment_size: 1024 * 1024,
            max_segment_count: 10,
            custom_segment: None,
            initialized_filters: None,
            shared_subscriptions_strategy: Default::default(),
        },
        v4: Some(stuff),
        v5: if v5_map.is_empty() { None } else { Some(v5_map) },
        ws: ret,
        cluster: None,
        console: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    })
}
