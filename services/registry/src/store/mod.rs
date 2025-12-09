pub mod sqlite;

#[cfg(feature = "dynamodb")]
pub mod dynamodb;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct Gateway {
    pub gateway_id: String,
    pub status: String,
    pub meta: serde_json::Value,
    pub created_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub gateway_id: String,
    pub device_id: String,
    pub status: String,
    pub meta: serde_json::Value,
    pub created_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
}

// sqlite for single-node, dynamodb for multi-region (eventually)
#[async_trait]
pub trait RegistryStore: Send + Sync + 'static {
    async fn list_gateways(&self) -> Result<Vec<Gateway>, StoreError>;
    async fn get_gateway(&self, gateway_id: &str) -> Result<Option<Gateway>, StoreError>;
    async fn list_devices(&self, gateway_id: &str) -> Result<Vec<Device>, StoreError>;
    async fn get_device(&self, gateway_id: &str, device_id: &str) -> Result<Option<Device>, StoreError>;

    async fn upsert_device_seen(
        &self,
        gateway_id: &str,
        device_id: &str,
        meta: serde_json::Value,
    ) -> Result<Device, StoreError>;

    async fn upsert_gateway_seen(
        &self,
        gateway_id: &str,
        meta: serde_json::Value,
    ) -> Result<Gateway, StoreError>;

    async fn request_gateway_onboarding(
        &self,
        gateway_id: &str,
        meta: serde_json::Value,
    ) -> Result<(Gateway, String), StoreError>;

    async fn approve_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError>;

    async fn claim_gateway_access_token(
        &self,
        gateway_id: &str,
        enrollment_token: &str,
    ) -> Result<String, StoreError>;

    async fn verify_gateway_access_token(
        &self, gateway_id: &str, token: &str,
    ) -> Result<bool, StoreError>;

    async fn rotate_gateway_access_token(
        &self,
        gateway_id: &str,
        old_token: &str,
    ) -> Result<String, StoreError>;

    async fn revoke_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError>;
    async fn decommission_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError>;

    async fn register_device(
        &self,
        gateway_id: &str,
        device_id: &str,
        meta: serde_json::Value,
    ) -> Result<Device, StoreError>;

    async fn revoke_device(
        &self,
        gateway_id: &str,
        device_id: &str,
    ) -> Result<Device, StoreError>;

    async fn decommission_device(
        &self,
        gateway_id: &str,
        device_id: &str,
    ) -> Result<Device, StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_error_bad_request_display() {
        let msg = "missing field";
        let err = StoreError::BadRequest(msg.to_string());
        let display = format!("{err}");
        assert_eq!(display, "bad request: missing field");
    }

    #[test]
    fn store_error_unavailable_display() {
        let err = StoreError::Unavailable("connection refused".to_string());
        let display = format!("{err}");
        assert_eq!(display, "unavailable: connection refused");
    }

    #[test]
    fn store_error_internal_display() {
        let err = StoreError::Internal("disk full".to_string());
        let display = format!("{err}");
        assert_eq!(display, "internal error: disk full");
    }

    #[test]
    fn gateway_serializes_to_json_with_all_fields() {
        let gw = Gateway {
            gateway_id: "gw-001".to_string(),
            status: "approved".to_string(),
            meta: serde_json::json!({"firmware": "1.2.3"}),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            last_seen_at: "2025-06-15T12:00:00Z".to_string(),
        };
        let json = serde_json::to_value(&gw).expect("serialization should succeed");
        assert_eq!(json["gateway_id"], "gw-001");
        assert_eq!(json["status"], "approved");
        assert_eq!(json["meta"]["firmware"], "1.2.3");
        assert_eq!(json["created_at"], "2025-01-01T00:00:00Z");
        assert_eq!(json["last_seen_at"], "2025-06-15T12:00:00Z");
    }

    #[test]
    fn device_serializes_to_json_with_all_fields() {
        let dev = Device {
            gateway_id: "gw-001".to_string(),
            device_id: "dev-abc".to_string(),
            status: "active".to_string(),
            meta: serde_json::json!({}),
            created_at: "2025-03-01T00:00:00Z".to_string(),
            last_seen_at: "2025-03-10T08:30:00Z".to_string(),
        };
        let json = serde_json::to_value(&dev).expect("serialization should succeed");
        assert_eq!(json["gateway_id"], "gw-001");
        assert_eq!(json["device_id"], "dev-abc");
        assert_eq!(json["status"], "active");
        assert_eq!(json["meta"], serde_json::json!({}));
        assert_eq!(json["created_at"], "2025-03-01T00:00:00Z");
        assert_eq!(json["last_seen_at"], "2025-03-10T08:30:00Z");
    }

    #[test]
    fn gateway_clone_produces_independent_copy() {
        let gw = Gateway {
            gateway_id: "gw-clone".to_string(),
            status: "pending_approval".to_string(),
            meta: serde_json::json!({"key": "value"}),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            last_seen_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let cloned = gw.clone();
        assert_eq!(cloned.gateway_id, gw.gateway_id);
        assert_eq!(cloned.status, gw.status);
        assert_eq!(cloned.meta, gw.meta);
    }

    #[test]
    fn device_clone_produces_independent_copy() {
        let dev = Device {
            gateway_id: "gw-001".to_string(),
            device_id: "dev-001".to_string(),
            status: "active".to_string(),
            meta: serde_json::json!(null),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            last_seen_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let cloned = dev.clone();
        assert_eq!(cloned.device_id, dev.device_id);
        assert_eq!(cloned.gateway_id, dev.gateway_id);
    }

    #[test]
    fn store_error_debug_format_is_not_empty() {
        let err = StoreError::Internal("oops".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("Internal"));
        assert!(debug.contains("oops"));
    }
}
