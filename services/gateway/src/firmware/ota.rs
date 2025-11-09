use crate::state::SharedState;

pub enum OtaResult {
    Success { new_version: u16 },
    NotSupported,
    Failed(String),
}

pub async fn probe_ota_support(ctx: &mut tokio_modbus::client::Context) -> bool {
    false // TODO: read OTA control register
}

pub async fn read_firmware_version(ctx: &mut tokio_modbus::client::Context) -> Option<u16> {
    None // TODO: read firmware version register
}

pub async fn flash_firmware(
    ctx: &mut tokio_modbus::client::Context,
    blob: &[u8],
    st: &SharedState,
) -> OtaResult {
    OtaResult::NotSupported
}
