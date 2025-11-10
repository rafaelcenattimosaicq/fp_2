use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub gateway_id: String,
    pub serial: SerialConfig,
    pub mqtt: MqttConfig,
    pub registry: RegistryConfig,
    pub devices_api_url: String,
    #[serde(default)]
    pub devices_dir: Option<PathBuf>,
    pub poll_interval_ms: u64,
    pub nes: Option<NesConfig>,
    pub worker: Option<WorkerConfig>,
    pub vpn: Option<VpnConfig>,
    #[serde(default)]
    pub docker: DockerConfig,
    #[serde(default)]
    pub emulated_device_id: Option<u16>,
    #[serde(default)]
    pub firmware_api_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DockerConfig {
    #[serde(default = "default_mqtt_image")]
    pub mqtt_image: String,
    #[serde(default = "default_registry_image")]
    pub registry_image: String,
    #[serde(default = "default_rules_engine_image")]
    pub rules_engine_image: String,
    #[serde(default = "default_nes_image")]
    pub nes_image: String,
    #[serde(default = "default_services_dir")]
    pub services_dir: String,
}

fn default_mqtt_image() -> String {
    "eclipse-mosquitto:2".to_string()
}

fn default_registry_image() -> String {
    "ghcr.io/rafaelcenattimosaicq/gateway-registry:latest".to_string()
}

fn default_rules_engine_image() -> String {
    "ghcr.io/rafaelcenattimosaicq/gateway-rules-engine:latest".to_string()
}

fn default_nes_image() -> String {
    "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest".to_string()
}

fn default_services_dir() -> String {
    "..".to_string()
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            mqtt_image: default_mqtt_image(),
            registry_image: default_registry_image(),
            rules_engine_image: default_rules_engine_image(),
            nes_image: default_nes_image(),
            services_dir: default_services_dir(),
        }
    }
}

fn default_worker_image() -> String {
    "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct SerialConfig {
    pub port: String,
    pub baud_rate: u32,
    pub slave_id: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttConfig {
    pub broker_url: String,
    pub topic: String,
    pub qos: u8,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryConfig {
    pub url: String,
    pub enrollment_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NesConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerConfig {
    pub binary_path: String,
    pub coordinator_host: String,
    pub coordinator_port: u16,
    pub local_worker_host: String,
    pub logical_source_name: String,
    pub physical_source_name: String,
    pub mqtt_broker_url: String,
    pub mqtt_topic: String,
    #[serde(default)]
    pub coordinator_rest_url: Option<String>,
    #[serde(default = "default_worker_image")]
    pub image: String,
    #[serde(default = "default_max_schema_fields")]
    pub max_schema_fields: usize,
    #[serde(default)]
    pub force_host_network: bool,
    #[serde(default = "default_rpc_port")]
    pub rpc_port: u16,
    #[serde(default = "default_data_port")]
    pub data_port: u16,
}

const fn default_rpc_port() -> u16 {
    40000
}

const fn default_data_port() -> u16 {
    40001
}

const fn default_max_schema_fields() -> usize {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub struct VpnConfig {
    pub provisioner_url: String,
    pub pre_shared_secret: Option<String>,
}

/// minimum poll interval we'll actually honour. Anything below this and the
/// modbus RTU bus can't turnaround in time on the RS-485 transceiver, plus
/// the Pi's CPU pegs at 100% trying to keep up.
const MIN_POLL_INTERVAL_MS: u64 = 50;

// TODO(rc): should we cap poll_interval at 10s?
// talked to supervisor, he said maybe
// if config.poll_interval_ms > 10000 {
//     config.poll_interval_ms = 10000;
// }

pub fn load_config(path: &Path) -> Result<GatewayConfig, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(path)?;
    let mut config: GatewayConfig = serde_yaml::from_str(&contents)?;

    if config.poll_interval_ms < MIN_POLL_INTERVAL_MS {
        config.poll_interval_ms = MIN_POLL_INTERVAL_MS;
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    const VALID_YAML: &str = r#"
gateway_id: "gw-001"
serial:
  port: "/dev/ttyUSB0"
  baud_rate: 9600
  slave_id: 1
mqtt:
  broker_url: "mqtt://broker.local:1883"
  topic: "telemetry/gw-001"
  qos: 1
registry:
  url: "http://registry:8080"
  enrollment_token: "tok-abc-123"
devices_api_url: "https://example.com"
devices_dir: "./devices"
poll_interval_ms: 5000
"#;

    const VALID_YAML_WITH_NES: &str = r#"
gateway_id: "gw-002"
serial:
  port: "/dev/ttyUSB0"
  baud_rate: 9600
  slave_id: 1
mqtt:
  broker_url: "mqtt://broker.local:1883"
  topic: "telemetry/gw-002"
  qos: 1
registry:
  url: "http://registry:8080"
  enrollment_token: "tok-abc-123"
devices_api_url: "https://example.com"
devices_dir: "./devices"
poll_interval_ms: 5000
nes:
  host: "127.0.0.1"
  port: 50501
"#;

    #[test]
    fn parses_valid_gateway_yaml() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(VALID_YAML.as_bytes())
            .expect("failed to write YAML to temp file");

        let config = load_config(file.path()).expect("load_config should succeed");

        assert_eq!(config.gateway_id, "gw-001");

        assert_eq!(config.serial.port, "/dev/ttyUSB0");
        assert_eq!(config.serial.baud_rate, 9600);
        assert_eq!(config.serial.slave_id, 1);

        assert_eq!(config.mqtt.broker_url, "mqtt://broker.local:1883");
        assert_eq!(config.mqtt.topic, "telemetry/gw-001");
        assert_eq!(config.mqtt.qos, 1);

        assert_eq!(config.registry.url, "http://registry:8080");
        assert_eq!(config.registry.enrollment_token, "tok-abc-123");

        assert_eq!(config.devices_api_url, "https://example.com");
        assert_eq!(config.devices_dir, Some(PathBuf::from("./devices")));
        assert_eq!(config.poll_interval_ms, 5000);

        assert!(config.nes.is_none(), "nes should be None when not in YAML");
    }

    #[test]
    fn parses_optional_nes_config() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(VALID_YAML_WITH_NES.as_bytes())
            .expect("failed to write YAML to temp file");

        let config = load_config(file.path()).expect("load_config should succeed");

        let nes = config.nes.expect("nes section should be Some");
        assert_eq!(nes.host, "127.0.0.1");
        assert_eq!(nes.port, 50501);
    }

    const VALID_YAML_WITH_EDGE: &str = r#"
gateway_id: "gw-edge-001"
serial:
  port: "/dev/ttyUSB0"
  baud_rate: 9600
  slave_id: 254
mqtt:
  broker_url: "mqtt://localhost:1883"
  topic: "controller_app/events"
  qos: 1
registry:
  url: "http://registry:8080"
  enrollment_token: "tok-abc-123"
devices_api_url: "https://example.com"
devices_dir: "./devices"
poll_interval_ms: 1000
worker:
  binary_path: "./nesWorker"
  coordinator_host: "nebulastream.iot.local"
  coordinator_port: 8080
  local_worker_host: "100.100.100.2"
  logical_source_name: "telemetry"
  physical_source_name: "edge-mqtt"
  mqtt_broker_url: "tcp://localhost:1883"
  mqtt_topic: "telemetry"
"#;

    #[test]
    fn parses_optional() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(VALID_YAML_WITH_EDGE.as_bytes())
            .expect("failed to write YAML to temp file");

        let config = load_config(file.path()).expect("load_config should succeed");

        let worker = config.worker.expect("worker section should be Some");
        assert_eq!(worker.binary_path, "./nesWorker");
        assert_eq!(worker.coordinator_host, "nebulastream.iot.local");
        assert_eq!(worker.coordinator_port, 8080);
        assert_eq!(worker.local_worker_host, "100.100.100.2");
        assert_eq!(worker.logical_source_name, "telemetry");
        assert_eq!(worker.physical_source_name, "edge-mqtt");
        assert_eq!(worker.mqtt_broker_url, "tcp://localhost:1883");
        assert_eq!(worker.mqtt_topic, "telemetry");
    }

    #[test]
    fn edge_config_is_none_when_absent() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(VALID_YAML.as_bytes())
            .expect("failed to write YAML to temp file");

        let config = load_config(file.path()).expect("load_config should succeed");
        assert!(config.worker.is_none(), "worker should be None");
    }

    const VALID_YAML_WITH_VPN: &str = r#"
gateway_id: "gw-vpn-001"
serial:
  port: "/dev/ttyUSB0"
  baud_rate: 9600
  slave_id: 1
mqtt:
  broker_url: "mqtt://broker.local:1883"
  topic: "telemetry/gw-vpn-001"
  qos: 1
registry:
  url: "http://registry:8080"
  enrollment_token: "tok-abc-123"
devices_api_url: "https://example.com"
devices_dir: "./devices"
poll_interval_ms: 5000
vpn:
  provisioner_url: "https://vpn-api.example.com"
"#;

    #[test]
    fn parses_optional_vpn_config() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(VALID_YAML_WITH_VPN.as_bytes())
            .expect("failed to write YAML to temp file");

        let config = load_config(file.path()).expect("load_config should succeed");

        let vpn = config.vpn.expect("vpn section should be Some");
        assert_eq!(vpn.provisioner_url, "https://vpn-api.example.com");
    }

    #[test]
    fn vpn_config_is_none_when_absent() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(VALID_YAML.as_bytes())
            .expect("failed to write YAML to temp file");

        let config = load_config(file.path()).expect("load_config should succeed");
        assert!(config.vpn.is_none(), "vpn should be None");
    }

    #[test]
    fn docker_config_defaults_when_absent() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(VALID_YAML.as_bytes()).expect("write");

        let config = load_config(file.path()).expect("parse");

        assert_eq!(config.docker.mqtt_image, "eclipse-mosquitto:2");
        assert_eq!(config.docker.registry_image, "ghcr.io/rafaelcenattimosaicq/gateway-registry:latest");
        assert_eq!(config.docker.rules_engine_image, "ghcr.io/rafaelcenattimosaicq/gateway-rules-engine:latest");
        assert_eq!(config.docker.nes_image, "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest");
        assert_eq!(config.docker.services_dir, "..");
    }

    #[test]
    fn worker_image_defaults_when_absent() {
        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(VALID_YAML_WITH_EDGE.as_bytes()).expect("write");

        let config = load_config(file.path()).expect("parse");

        let worker = config.worker.expect("worker present");
        assert_eq!(worker.image, "ghcr.io/rafaelcenattimosaicq/nes-executable-image:latest");
    }

    #[test]
    fn rejects_invalid_yaml() {
        let mut file = NamedTempFile::new().expect("failed to create temp file");
        file.write_all(b":::not valid yaml at all:::")
            .expect("failed to write garbage to temp file");

        let result = load_config(file.path());
        assert!(result.is_err(), "load_config should return Err for invalid YAML");
    }

    // bug: on first boot the config file doesn't exist yet because the
    // provisioning step hasn't run. load_config was unwrap'd at the call site,
    // which brought the whole gateway down instead of entering provisioning mode.
    #[test]
    fn missing_file_returns_err() {
        let result = load_config(Path::new("/nonexistent/gateway.yaml"));
        assert!(result.is_err(), "non-existent path must be an Err, not a panic");
    }

    // reported by a custousereer: YAML had poll_interval_ms as a string ("5000")
    // instead of a bare integer. serde_yaml should reject the type mismatch.
    #[test]
    fn rejects_wrong_field_type() {
        let bad_yaml = r#"
gateway_id: "gw-bad"
serial:
  port: "/dev/ttyUSB0"
  baud_rate: 9600
  slave_id: 1
mqtt:
  broker_url: "mqtt://broker.local:1883"
  topic: "telemetry/gw-bad"
  qos: 1
registry:
  url: "http://registry:8080"
  enrollment_token: "tok-abc-123"
devices_api_url: "https://example.com"
poll_interval_ms: "not_a_number"
"#;

        let mut file = NamedTempFile::new().expect("temp file");
        file.write_all(bad_yaml.as_bytes()).expect("write");

        let result = load_config(file.path());
        assert!(result.is_err(), "string in a u64 field should fail deserialization");
    }
}
