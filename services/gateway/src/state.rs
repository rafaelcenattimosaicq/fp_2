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
