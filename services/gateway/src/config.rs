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
