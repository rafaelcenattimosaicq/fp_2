use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceDescriptor {
    #[serde(default)]
    pub yaml_version: Option<serde_yaml::Value>,
    #[serde(rename = "Device Description", default)]
    pub device_description: Option<DeviceDescription>,
    #[serde(default)]
    pub characteristics: Option<Characteristics>,
    #[serde(default)]
    pub services: Vec<Service>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceDescription {
    #[serde(rename = "type", default)]
    pub device_type: Option<String>,
    #[serde(rename = "Device ID", default)]
    pub device_id: Option<String>,
    #[serde(rename = "Software Version", default)]
    pub software_version: Option<serde_yaml::Value>,
    #[serde(default)]
    pub hw_sw_set: Option<HwSwSet>,
    #[serde(default)]
    pub units_handling: Vec<UnitHandling>,
    #[serde(default)]
    pub access_level: Option<AccessLevel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HwSwSet {
    #[serde(default)]
    pub address: Option<u32>,
    #[serde(default)]
    pub length: Option<u32>,
    #[serde(default)]
    pub default_value: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitHandling {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub selection_param: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccessLevel {
    #[serde(default)]
    pub password_input: Option<String>,
    #[serde(default)]
    pub read_access_level: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Characteristics {
    #[serde(default)]
    pub parameters: Vec<Register>,
    #[serde(default)]
    pub status: Vec<Register>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Register {
    pub id: String,
    #[allow(clippy::struct_field_names)] // "type" is the YAML key from the client's spec, can't rename it
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
    // NOTE: older descriptor versions store this as an integer (e.g. 10) while
    // newer ones use float (e.g. 10.0). serde_yaml handles both, but watch out
    // if you ever switch to a stricter parser.
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

impl Register {
    pub fn decode(&self, raw: u16) -> RegisterValue {
        let type_str = self.register_type.as_deref().unwrap_or("");

        match type_str {
            "integer" => self.decode_integer(raw),
            "enum" => self.decode_enum(raw),
            "bitwise" => self.decode_bitwise(raw),
            "boolean" => RegisterValue::Boolean(raw != 0),
            _ => self.decode_unsigned(raw),
        }
    }

    pub fn display_name(&self) -> &str {
        if let Some(ref n) = self.name {
            if !n.is_empty() {
                return n;
            }
        }
        if let Some(ref a) = self.acronym {
            if !a.is_empty() {
                return a;
            }
        }
        &self.id
    }

    fn decode_integer(&self, raw: u16) -> RegisterValue {
        #[allow(clippy::cast_possible_wrap)]
        let signed = raw as i16;
        let multiplier = self.effective_multiplier();

        RegisterValue::Float(f64::from(signed) / multiplier)
    }

    fn decode_unsigned(&self, raw: u16) -> RegisterValue {
        let multiplier = self.effective_multiplier();

        if (multiplier - 1.0).abs() < f64::EPSILON {
            RegisterValue::Unsigned(u64::from(raw))
        } else {
            RegisterValue::Float(f64::from(raw) / multiplier)
        }
    }

    fn decode_enum(&self, raw: u16) -> RegisterValue {
        let label = self.fields.iter()
            .find(|f| f.index == raw)
            .and_then(|f| f.name.clone())
            .unwrap_or_else(|| format!("?{raw}"));

        RegisterValue::Enum(label)
    }

    fn decode_bitwise(&self, raw: u16) -> RegisterValue {
        let mut bits = HashMap::new();

        for field in &self.fields {
            let bit_name = field
                .name
                .clone()
                .unwrap_or_else(|| format!("bit_{}", field.index));
            let is_set = (raw >> field.index) & 1 == 1;
            bits.insert(bit_name, is_set);
        }

        RegisterValue::Bitwise(bits)
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn encode(&self, value: f64) -> u16 {
        let type_str = self.register_type.as_deref().unwrap_or("");
        let multiplier = self.effective_multiplier();

        match type_str {
            "integer" => {
                let raw = (value * multiplier).round() as i16;
                raw as u16
            }
            "enum" | "bitwise" | "boolean" => {
                value.round() as u16
            }
            _ => {
                let raw = (value * multiplier).round();
                raw as u16
            }
        }
    }

    fn effective_multiplier(&self) -> f64 {
        match self.multiplier {
            Some(m) if m.abs() > f64::EPSILON => m,
            // some descriptors from hw rev 2 had multiplier: 0 for boolean-ish
            // registers. Treat it as 1.0 to avoid division by zero.
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Service {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub config_groups: Vec<ConfigGroup>,
    #[serde(default)]
    pub table_data: Vec<ParamRef>,
    #[serde(default)]
    pub graph_data: Vec<ParamRef>,
    #[serde(default)]
    pub storage_data: Vec<ParamRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigGroup {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub acronym: Option<String>,
    #[serde(default)]
    pub parameters: Vec<ParamRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParamRef {
    #[serde(default)]
    pub id: Option<String>,
}

pub fn load_descriptor(path: &Path) -> Result<DeviceDescriptor, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(path)?;
    let descriptor: DeviceDescriptor = serde_yaml::from_str(&contents)?;
    Ok(descriptor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_register(register_type: &str, multiplier: Option<f64>, fields: Vec<EnumField>) -> Register {
        Register {
            id: "TEST_REG".to_string(),
            register_type: Some(register_type.to_string()),
            name: None,
            acronym: None,
            description: None,
            address: None,
            min_value: None,
            max_value: None,
            default_value: None,
            multiplier,
            unit: None,
            is_read_only: None,
            is_write_only: None,
            is_visible: None,
            read_access_level: None,
            write_access_level: None,
            hw_sw_set_mask: None,
            is_delta: None,
            in_chart: None,
            fields,
        }
    }

    #[test]
    fn decodes_signed_integer_with_multiplier() {
        let reg = make_register("integer", Some(10.0), vec![]);

        let pos = reg.decode(152);
        assert_eq!(pos, RegisterValue::Float(15.2));

        let neg = reg.decode(0xFFF6);
        assert_eq!(neg, RegisterValue::Float(-1.0));
    }

    #[test]
    fn decodes_enum_register() {
        let fields = vec![
            EnumField { index: 0, name: Some("Disable".to_string()) },
            EnumField { index: 1, name: Some("Enable".to_string()) },
        ];
        let reg = make_register("enum", None, fields);

        assert_eq!(reg.decode(1), RegisterValue::Enum("Enable".to_string()));
        assert_eq!(reg.decode(0), RegisterValue::Enum("Disable".to_string()));
        assert_eq!(reg.decode(99), RegisterValue::Enum("Unknown(99)".to_string()));
    }

    #[test]
    fn decodes_bitwise_register() {
        let fields = vec![
            EnumField { index: 0, name: Some("Enabled".to_string()) },
            EnumField { index: 1, name: Some("Activate buzzer".to_string()) },
            EnumField { index: 2, name: Some("Activate display message".to_string()) },
        ];
        let reg = make_register("bitwise", None, fields);

        let result = reg.decode(0b101);

        if let RegisterValue::Bitwise(bits) = result {
            assert_eq!(bits.get("Enabled"), Some(&true));
            assert_eq!(bits.get("Activate buzzer"), Some(&false));
            assert_eq!(bits.get("Activate display message"), Some(&true));
        } else {
            panic!("expected RegisterValue::Bitwise, got {result:?}");
        }
    }

    #[test]
    fn decodes_unsigned_integer() {
        let reg = make_register("unsigned integer", None, vec![]);
        assert_eq!(reg.decode(3000), RegisterValue::Unsigned(3000));
    }

    #[test]
    fn parses_real_yaml_if_available() {
        let path = Path::new("/Users/rafaelrccenatti/Downloads/V0x0007_1.03V2.yaml");

        if !path.exists() {
            eprintln!("Skipping real YAML test, file not found at {}", path.display());
            return;
        }

        let descriptor = load_descriptor(path).expect("failed to parse real YAML");

        let desc = descriptor
            .device_description
            .as_ref()
            .expect("Device Description should be present");
        assert_eq!(
            desc.device_type.as_deref(),
            Some("Controller"),
            "device type should be 'Controller'"
        );

        let chars = descriptor
            .characteristics
            .as_ref()
            .expect("characteristics should be present");
        assert!(!chars.parameters.is_empty());
        assert!(!chars.status.is_empty());

        println!(
            "Parsed {} parameters, {} status registers, {} services",
            chars.parameters.len(),
            chars.status.len(),
            descriptor.services.len()
        );

        let setpoint = chars
            .parameters
            .iter()
            .find(|p| p.id == "PARAM_TH_SETPOINT")
            .expect("PARAM_TH_SETPOINT should exist");
        assert_eq!(setpoint.register_type.as_deref(), Some("integer"));
        assert_eq!(setpoint.multiplier, Some(10.0));
        assert_eq!(setpoint.address, Some(60002));
        assert_eq!(setpoint.display_name(), "Setpoint");

        assert!(!descriptor.services.is_empty());
    }

    #[test]
    fn display_name_prefers_name_then_acronym_then_id() {
        let mut reg = make_register("integer", None, vec![]);
        reg.name = Some("Setpoint".to_string());
        reg.acronym = Some("t00".to_string());
        assert_eq!(reg.display_name(), "Setpoint");

        reg.name = None;
        assert_eq!(reg.display_name(), "t00");

        reg.acronym = None;
        assert_eq!(reg.display_name(), "TEST_REG");
    }

    #[test]
    fn decodes_boolean_register() {
        let reg = make_register("boolean", None, vec![]);

        assert_eq!(reg.decode(1), RegisterValue::Boolean(true));
        assert_eq!(reg.decode(0), RegisterValue::Boolean(false));
        assert_eq!(reg.decode(42), RegisterValue::Boolean(true));
    }

    #[test]
    fn decodes_unsigned_integer_with_multiplier() {
        let reg = make_register("unsigned integer", Some(100.0), vec![]);
        assert_eq!(reg.decode(3000), RegisterValue::Float(30.0));
    }

    #[test]
    fn encodes_float_to_raw_register() {
        let reg = make_register("integer", Some(10.0), vec![]);
        let raw = reg.encode(15.2);
        assert_eq!(raw, 152);
    }

    #[test]
    fn encodes_negative_integer() {
        let reg = make_register("integer", Some(10.0), vec![]);
        let raw = reg.encode(-1.0);
        assert_eq!(raw, (-10_i16) as u16);
    }

    #[test]
    fn encodes_without_multiplier() {
        let reg = make_register("unsigned integer", None, vec![]);
        let raw = reg.encode(42.0);
        assert_eq!(raw, 42);
    }

    #[test]
    fn zero_multiplier_treated_as_one() {
        // hw rev 2 descriptors sometimes had multiplier: 0 on status registers
        let reg = make_register("integer", Some(0.0), vec![]);
        let result = reg.decode(100);
        assert_eq!(result, RegisterValue::Float(100.0));
    }

    #[test]
    fn encodes_enum_by_index() {
        let fields = vec![
            EnumField { index: 0, name: Some("Off".into()) },
            EnumField { index: 1, name: Some("On".into()) },
        ];
        let reg = make_register("enum", None, fields);
        let raw = reg.encode(1.0);
        assert_eq!(raw, 1);
    }
}
