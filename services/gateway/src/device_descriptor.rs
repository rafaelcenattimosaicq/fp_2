use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Register {
    pub id: String,
    #[allow(clippy::struct_field_names)]
    #[serde(rename = "type", default)]
    pub register_type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub acronym: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub address: Option<u16>,
    #[serde(default)]
    pub min_value: Option<f64>,
    #[serde(default)]
    pub max_value: Option<f64>,
    #[serde(default)]
    pub default_value: Option<f64>,
    #[serde(default)]
    pub multiplier: Option<f64>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub is_read_only: Option<bool>,
    #[serde(default)]
    pub is_write_only: Option<bool>,
    #[serde(default)]
    pub is_visible: Option<serde_yaml::Value>,
    #[serde(default)]
    pub read_access_level: Option<u8>,
    #[serde(default)]
    pub write_access_level: Option<u8>,
    #[serde(default)]
    pub hw_sw_set_mask: Option<u64>,
    #[serde(default)]
    pub is_delta: Option<bool>,
    #[serde(default)]
    pub in_chart: Option<bool>,
    #[serde(default)]
    pub fields: Vec<EnumField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnumField {
    pub index: u16,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RegisterValue {
    Float(f64),
    Unsigned(u64),
    Enum(String),
    Bitwise(HashMap<String, bool>),
    Boolean(bool),
}
