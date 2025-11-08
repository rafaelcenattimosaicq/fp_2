use crate::device_descriptor::DeviceDescriptor;
use crate::state::SharedState;
use std::path::Path;

pub async fn discover_and_load(
    ctx: &mut tokio_modbus::client::Context,
    api_base: &str,
    local_dir: Option<&Path>,
    st: &SharedState,
) -> Option<DeviceDescriptor> {
    // TODO: read device ID register, fetch from cloud, fallback to local
    None
}
