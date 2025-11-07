use crate::device_descriptor::{DeviceDescriptor, RegisterValue};
use crate::vpn::fingerprint::HardwareFingerprint;
use chrono::{DateTime, Local};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => f.write_str("Disconnected"),
            Self::Connecting => f.write_str("Connecting"),
            Self::Connected => f.write_str("Connected"),
            Self::Error(reason) => write!(f, "Error: {reason}"),
        }
    }
}

// NOTE: considered adding a Timeout variant here but the Modbus
// library already maps timeouts to Error("timed out"), so we'd
// just be duplicating. Leaving this as a reminder.
// Timeout(Duration),

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VpnStatus {
    NotConfigured,
    Checking,
    Installing,
    Provisioning,
    Connecting,
    Connected(String),
    Error(String),
}

impl fmt::Display for VpnStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "Not configured"),
            Self::Checking => write!(f, "Checking"),
            Self::Installing => write!(f, "Installing"),
            Self::Provisioning => write!(f, "Provisioning"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected(ip) => write!(f, "Connected ({ip})"),
            Self::Error(reason) => write!(f, "Error: {reason}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerStatus {
    NotManaged,
    Starting,
    Running,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    Pending,
    Pulling,
    Starting,
    Running,
    Error(String),
}

impl ServiceStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Pending => "Pending".into(),
            Self::Pulling => "Pulling".into(),
            Self::Starting => "Starting".into(),
            Self::Running => "Running".into(),
            Self::Error(reason) => format!("Error: {reason}"),
        }
    }
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NesStatus {
    Disabled,
    WaitingForDevice,
    RegisteringSchema,
    WorkerStarting,
    Connected { worker_id: u32 },
    Reconnecting { reason: String },
    Error(String),
}

impl fmt::Display for NesStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Disabled => "Disabled",
            Self::WaitingForDevice => "Waiting for device",
            Self::RegisteringSchema => "Registering schema",
            Self::WorkerStarting => "Worker starting",
            Self::Connected { worker_id } => {
                return write!(f, "Connected (worker {worker_id})");
            }
            Self::Reconnecting { reason } => {
                return write!(f, "Reconnecting: {reason}");
            }
            Self::Error(reason) => {
                return write!(f, "Error: {reason}");
            }
        };
        f.write_str(label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedQuery {
    pub query_id: u64,
    pub status: String,
    pub first_seen_secs: u64,
    pub auto_stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrafficDirection {
    Tx,
    Rx,
}

#[derive(Debug, Clone)]
pub struct SerialTrafficEntry {
    pub timestamp: DateTime<Local>,
    pub direction: TrafficDirection,
    pub bytes: Vec<u8>,
}

const MAX_LOG_ENTRIES: usize = 500;
// one descriptor dump message was 14KB and the egui label widget choked on it.
const MAX_LOG_MESSAGE_LEN: usize = 512;

const MAX_TRAFFIC_ENTRIES: usize = 2000;

#[derive(Debug)]
pub struct AppState {
    pub serial_status: ConnectionStatus,
    pub mqtt_status: ConnectionStatus,
    pub vpn_status: VpnStatus,
    pub docker_status: DockerStatus,
    pub docker_services: Vec<(String, ServiceStatus)>,
    pub nes_status: NesStatus,
    pub register_values: HashMap<String, RegisterValue>,
    #[allow(dead_code)]
    pub parameter_values: HashMap<String, RegisterValue>,
    pub last_poll_ms: Option<u64>,
    pub poll_error_count: u32,
    pub log: Vec<LogEntry>,
    pub descriptor: Option<DeviceDescriptor>,
    pub available_ports: Vec<(String, String)>,
    pub available_ble_devices: Vec<(String, String)>,
    pub gateway_id: String,
    pub serial_traffic: Vec<SerialTrafficEntry>,
    pub fingerprint: Option<HardwareFingerprint>,
    pub vpn_secret_configured: bool,
    pub vpn_secret: String,
    pub config_path: String,
    pub chart_register_ids: HashSet<String>,
    pub tracked_queries: Vec<TrackedQuery>,
    pub ota_status: crate::firmware::types::OtaStatus,
    pub device_supports_ota: bool,
    pub device_firmware_version: Option<u16>,
    pub mqtt_credentials: Option<(String, String)>,
}

impl AppState {
    pub fn new(gateway_id: String) -> Self {
        Self {
            serial_status: ConnectionStatus::Disconnected,
            mqtt_status: ConnectionStatus::Disconnected,
            vpn_status: VpnStatus::NotConfigured,
            docker_status: DockerStatus::NotManaged,
            docker_services: vec![
                ("MQTT broker".to_string(), ServiceStatus::Pending),
                ("Registry".to_string(), ServiceStatus::Pending),
                ("Rules engine".to_string(), ServiceStatus::Pending),
                ("MQTT-to-TCP bridge".to_string(), ServiceStatus::Pending),
                ("NES worker".to_string(), ServiceStatus::Pending),
            ],
            nes_status: NesStatus::Disabled,
            register_values: HashMap::new(),
            parameter_values: HashMap::new(),
            last_poll_ms: None,
            poll_error_count: 0,
            log: Vec::new(),
            descriptor: None,
            available_ports: Vec::new(),
            available_ble_devices: Vec::new(),
            gateway_id,
            serial_traffic: Vec::new(),
            fingerprint: None,
            vpn_secret_configured: false,
            vpn_secret: String::new(),
            config_path: String::new(),
            chart_register_ids: HashSet::new(),
            tracked_queries: Vec::new(),
            ota_status: crate::firmware::types::OtaStatus::Idle,
            device_supports_ota: false,
            device_firmware_version: None,
            mqtt_credentials: None,
        }
    }

    pub fn push_log(&mut self, level: LogLevel, message: impl Into<String>) {
        let mut msg = message.into();
        if msg.len() > MAX_LOG_MESSAGE_LEN {
            msg.truncate(MAX_LOG_MESSAGE_LEN);
            msg.push_str("...");
        }
        self.log.push(LogEntry {
            timestamp: Local::now(),
            level,
            message: msg,
        });

        let len = self.log.len();
        if len > MAX_LOG_ENTRIES {
            let excess = len - MAX_LOG_ENTRIES;
            self.log.drain(..excess);
        }
    }

    pub fn push_traffic(&mut self, direction: TrafficDirection, bytes: Vec<u8>) {
        self.serial_traffic.push(SerialTrafficEntry {
            timestamp: Local::now(),
            direction,
            bytes,
        });

        let len = self.serial_traffic.len();
        if len > MAX_TRAFFIC_ENTRIES {
            let excess = len - MAX_TRAFFIC_ENTRIES;
            self.serial_traffic.drain(..excess);
        }
    }

    pub fn clear_traffic(&mut self) {
        self.serial_traffic.clear();
    }

    pub fn set_service_status(&mut self, name: &str, status: ServiceStatus) {
        if let Some(entry) = self.docker_services.iter_mut().find(|(n, _)| n == name) {
            entry.1 = status;
        }
    }

    #[allow(dead_code)]
    pub fn all_services_running(&self) -> bool {
        !self.docker_services.is_empty()
            && self.docker_services.iter().all(|(name, s)| {
                name == "NES worker" || *s == ServiceStatus::Running
            })
    }

    #[allow(dead_code)]
    pub fn any_service_error(&self) -> bool {
        self.docker_services.iter().any(|(_, s)| matches!(s, ServiceStatus::Error(_)))
    }
}

/// shared between the egui UI thread and the tokio background runtime.
/// callers MUST handle `PoisonError` gracefully, if the Modbus poller panics
/// (which it does when the USB-serial adapter is yanked mid-transaction) the
/// `RwLock` stays poisoned and every `.unwrap()` cascades into a full crash.
pub type SharedState = Arc<RwLock<AppState>>;

pub fn new_shared_state(gateway_id: String) -> SharedState {
    Arc::new(RwLock::new(AppState::new(gateway_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_entries_capped() {
        let mut state = AppState::new("test-gw".to_string());

        for i in 0..600 {
            state.push_log(LogLevel::Info, format!("log {i}"));
        }

        assert_eq!(state.log.len(), 500);
        assert_eq!(state.log[0].message, "log 100");
        assert_eq!(state.log[499].message, "log 599");
    }

    #[test]
    fn connection_status_display() {
        assert_eq!(ConnectionStatus::Disconnected.to_string(), "Disconnected");
        assert_eq!(ConnectionStatus::Connecting.to_string(), "Connecting");
        assert_eq!(ConnectionStatus::Connected.to_string(), "Connected");
        assert_eq!(
            ConnectionStatus::Error("timeout".to_string()).to_string(),
            "Error: timeout"
        );
    }

    #[test]
    fn vpn_status_display() {
        assert_eq!(VpnStatus::NotConfigured.to_string(), "Not configured");
        assert_eq!(VpnStatus::Checking.to_string(), "Checking");
        assert_eq!(VpnStatus::Installing.to_string(), "Installing");
        assert_eq!(VpnStatus::Provisioning.to_string(), "Provisioning");
        assert_eq!(VpnStatus::Connecting.to_string(), "Connecting");
        assert_eq!(
            VpnStatus::Connected("100.64.0.1".to_string()).to_string(),
            "Connected (100.64.0.1)"
        );
        assert_eq!(
            VpnStatus::Error("no key".to_string()).to_string(),
            "Error: no key"
        );
    }

    #[test]
    fn nes_status_display() {
        assert_eq!(NesStatus::Disabled.to_string(), "Disabled");
        assert_eq!(NesStatus::WaitingForDevice.to_string(), "Waiting for device");
        assert_eq!(
            NesStatus::RegisteringSchema.to_string(),
            "Registering schema"
        );
        assert_eq!(NesStatus::WorkerStarting.to_string(), "Worker starting");
        assert_eq!(
            NesStatus::Connected { worker_id: 7 }.to_string(),
            "Connected (worker 7)"
        );
        assert_eq!(
            NesStatus::Reconnecting {
                reason: "evicted".to_string()
            }
            .to_string(),
            "Reconnecting: evicted"
        );
        assert_eq!(
            NesStatus::Error("spawn failed".to_string()).to_string(),
            "Error: spawn failed"
        );
    }

    #[test]
    fn new_state_has_correct_defaults() {
        let state = AppState::new("gw-42".to_string());

        assert_eq!(state.serial_status, ConnectionStatus::Disconnected);
        assert_eq!(state.mqtt_status, ConnectionStatus::Disconnected);
        assert_eq!(state.vpn_status, VpnStatus::NotConfigured);
        assert_eq!(state.nes_status, NesStatus::Disabled);
        assert!(state.register_values.is_empty());
        assert!(state.parameter_values.is_empty());
        assert_eq!(state.last_poll_ms, None);
        assert_eq!(state.poll_error_count, 0);
        assert!(state.log.is_empty());
        assert!(state.descriptor.is_none());
        assert!(state.available_ports.is_empty());
        assert!(state.available_ble_devices.is_empty());
        assert!(state.serial_traffic.is_empty());
        assert!(state.fingerprint.is_none());
        assert!(!state.vpn_secret_configured);
        assert_eq!(state.docker_status, DockerStatus::NotManaged);
        assert_eq!(state.docker_services.len(), 5);
        assert_eq!(
            state.docker_services[0],
            ("MQTT broker".to_string(), ServiceStatus::Pending)
        );
        assert_eq!(state.gateway_id, "gw-42");
        assert!(state.tracked_queries.is_empty());
        assert_eq!(state.ota_status, crate::firmware::types::OtaStatus::Idle);
        assert!(!state.device_supports_ota);
        assert_eq!(state.device_firmware_version, None);
    }

    #[test]
    fn service_status_display() {
        assert_eq!(ServiceStatus::Pending.to_string(), "Pending");
        assert_eq!(ServiceStatus::Pulling.to_string(), "Pulling");
        assert_eq!(ServiceStatus::Starting.to_string(), "Starting");
        assert_eq!(ServiceStatus::Running.to_string(), "Running");
        assert_eq!(
            ServiceStatus::Error("OOM".to_string()).to_string(),
            "Error: OOM"
        );
    }

    #[test]
    fn docker_status_debug() {
        assert_eq!(format!("{:?}", DockerStatus::NotManaged), "NotManaged");
        assert_eq!(format!("{:?}", DockerStatus::Starting), "Starting");
        assert_eq!(format!("{:?}", DockerStatus::Running), "Running");
        assert_eq!(
            format!("{:?}", DockerStatus::Error("no socket".to_string())),
            "Error(\"no socket\")"
        );
    }

    #[test]
    fn traffic_entries_capped() {
        let mut state = AppState::new("test-gw".to_string());

        for _ in 0..2500 {
            state.push_traffic(TrafficDirection::Tx, vec![0x01, 0x02]);
        }

        assert_eq!(state.serial_traffic.len(), 2000);
    }

    #[test]
    fn clear_traffic_empties_buffer() {
        let mut state = AppState::new("test-gw".to_string());
        state.push_traffic(TrafficDirection::Rx, vec![0xAA]);
        state.push_traffic(TrafficDirection::Tx, vec![0xBB]);
        assert_eq!(state.serial_traffic.len(), 2);

        state.clear_traffic();
        assert!(state.serial_traffic.is_empty());
    }

    #[test]
    fn set_service_status_updates_entry() {
        let mut state = AppState::new("test-gw".to_string());
        state.set_service_status("MQTT broker", ServiceStatus::Running);

        assert_eq!(state.docker_services[0].1, ServiceStatus::Running);
        assert_eq!(state.docker_services[1].1, ServiceStatus::Pending);
    }

    #[test]
    fn all_services_running_check() {
        let mut state = AppState::new("test-gw".to_string());
        assert!(!state.all_services_running());

        for (_, status) in &mut state.docker_services {
            *status = ServiceStatus::Running;
        }
        assert!(state.all_services_running());
    }

    #[test]
    fn any_service_error_check() {
        let mut state = AppState::new("test-gw".to_string());
        assert!(!state.any_service_error());

        state.set_service_status("Registry", ServiceStatus::Error("build failed".to_string()));
        assert!(state.any_service_error());
    }

    #[test]
    fn shared_state_is_readable_and_writable() {
        let shared = new_shared_state("gw-shared".to_string());

        {
            let mut state = shared.write().expect("write lock");
            state.push_log(LogLevel::Warn, "test warning");
        }

        let state = shared.read().expect("read lock");
        assert_eq!(state.log.len(), 1);
        assert_eq!(state.log[0].level, LogLevel::Warn);
        assert_eq!(state.log[0].message, "test warning");
    }
}
