use async_trait::async_trait;
use aws_sdk_dynamodb::Client;

use super::{Device, Gateway, RegistryStore, StoreError};

// dynamodb is so weird with its expression attribute names/values
// but we need it for multi-region eventually

pub struct DynamoRegistryStore {
    _client: Client,
    _table: String,
}

impl DynamoRegistryStore {
    pub async fn new(tbl: String) -> Result<Self, Box<dyn std::error::Error>> {
        let cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let c = Client::new(&cfg);
        Ok(Self { _client: c, _table: tbl })
    }
}

// TODO(raf): wire this up before the multi-region milestone.
// table schema: PK=gateway_id SK=device_id, GSI on status
// watch out for throttling during bulk onboarding -- need on-demand capacity
fn not_impl() -> StoreError {
    StoreError::Unavailable("DynamoDB store not yet implemented".to_string())
}

#[async_trait]
impl RegistryStore for DynamoRegistryStore {
    async fn list_gateways(&self) -> Result<Vec<Gateway>, StoreError> { Err(not_impl()) }

    async fn get_gateway(&self, _gateway_id: &str) -> Result<Option<Gateway>, StoreError> {
        Err(not_impl())
    }

    async fn list_devices(&self, _gateway_id: &str) -> Result<Vec<Device>, StoreError> {
        Err(not_impl())
    }

    async fn get_device(
        &self,
        _gateway_id: &str,
        _device_id: &str,
    ) -> Result<Option<Device>, StoreError> {
        Err(not_impl())
    }

    async fn upsert_device_seen(&self, _gateway_id: &str, _device_id: &str, _meta: serde_json::Value) -> Result<Device, StoreError> {
        Err(not_impl())
    }

    async fn upsert_gateway_seen(
        &self,
        _gateway_id: &str,
        _meta: serde_json::Value,
    ) -> Result<Gateway, StoreError> {
        Err(not_impl())
    }

    async fn request_gateway_onboarding(
        &self,
        _gateway_id: &str,
        _meta: serde_json::Value,
    ) -> Result<(Gateway, String), StoreError> {
        Err(not_impl())
    }

    async fn approve_gateway(&self, _gateway_id: &str) -> Result<Gateway, StoreError> {
        Err(not_impl())
    }

    async fn claim_gateway_access_token(&self, _gateway_id: &str, _enrollment_token: &str) -> Result<String, StoreError> { Err(not_impl()) }

    async fn verify_gateway_access_token(
        &self,
        _gateway_id: &str,
        _token: &str,
    ) -> Result<bool, StoreError> {
        Err(not_impl())
    }

    async fn rotate_gateway_access_token(
        &self,
        _gateway_id: &str,
        _old_token: &str,
    ) -> Result<String, StoreError> {
        Err(not_impl())
    }

    async fn revoke_gateway(&self, _gateway_id: &str) -> Result<Gateway, StoreError> { Err(not_impl()) }

    async fn decommission_gateway(&self, _gateway_id: &str) -> Result<Gateway, StoreError> {
        Err(not_impl())
    }

    async fn register_device(
        &self,
        _gateway_id: &str,
        _device_id: &str,
        _meta: serde_json::Value,
    ) -> Result<Device, StoreError> {
        Err(not_impl())
    }

    async fn revoke_device(
        &self,
        _gateway_id: &str,
        _device_id: &str,
    ) -> Result<Device, StoreError> {
        Err(not_impl())
    }

    async fn decommission_device(
        &self,
        _gateway_id: &str,
        _device_id: &str,
    ) -> Result<Device, StoreError> {
        Err(not_impl())
    }
}
