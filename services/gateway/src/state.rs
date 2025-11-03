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
