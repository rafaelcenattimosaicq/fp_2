#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtaStatus {
    Idle,
    Downloading,
    Flashing,
    Error(String),
}
