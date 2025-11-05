#[derive(Debug, Clone)]
pub struct HardwareFingerprint {
    pub serial: String,
    pub mac: String,
    pub model: String,
}
