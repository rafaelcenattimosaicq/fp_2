use std::collections::HashSet;
use std::fmt::Write as _;

use crate::device_descriptor::DeviceDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NesField {
    pub name: String,
    pub nes_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NesSchema {
    pub logical_source_name: String,
    pub fields: Vec<NesField>,
}

#[allow(clippy::match_same_arms)]
fn map_reg_type(rt: Option<&str>) -> Option<&'static str> {
    match rt {
        Some("unsigned integer") => Some("UINT64"),
        Some("integer") => Some("FLOAT64"),
        Some("enum") => None,    // can't represent string enums in NES
        Some("boolean") => Some("UINT64"), // 0/1
        Some("bitwise") => None, // bitmask, not useful for analytics
        _ => Some("FLOAT64"),    // safe default
    }
}

fn extract_device_type_id(desc: &DeviceDescriptor) -> String {
    desc.device_description.as_ref()
        .and_then(|dd| dd.device_id.as_deref())
        .map_or_else(
            || "unknown".to_string(),
            |id| id.to_lowercase().replace('\'', ""),
        )
}

fn graph_data_ids(desc: &DeviceDescriptor) -> Vec<String> {
    for svc in &desc.services {
        let is_da = svc.id.as_deref()
            .is_some_and(|id| id.contains("DATA_ACQUISITION"));
        if is_da {
            return svc.graph_data.iter()
                .filter_map(|pr| pr.id.clone())
                .collect();
        }
    }
    Vec::new()
}

/// build a `NesSchema` 
pub fn build_schema(desc: &DeviceDescriptor, max_reg_fields: usize) -> NesSchema {
    let dev_id = extract_device_type_id(desc);
    let ls_name = format!("telemetry_{dev_id}");

    let mut fields = vec![
        NesField { name: "DEVICE_ID".to_string(), nes_type: "UINT64".to_string() },
        NesField { name: "GATEWAY_ID".to_string(), nes_type: "UINT64".to_string() },
        NesField { name: "timestamp".to_string(), nes_type: "UINT64".to_string() },
    ];

    let mut status_by_id: std::collections::HashMap<&str, &crate::device_descriptor::Register> =
        std::collections::HashMap::new();
    if let Some(chars) = &desc.characteristics {
        for reg in &chars.status {
            status_by_id.entry(reg.id.as_str()).or_insert(reg);
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut reg_count = 0;

    let mut try_add = |rid: &str| {
        if reg_count >= max_reg_fields { return; }
        if !seen.insert(rid.to_string()) { return; } // already added
        if let Some(reg) = status_by_id.get(rid) {
            if let Some(nes_t) = map_reg_type(reg.register_type.as_deref()) {
                fields.push(NesField { name: reg.id.clone(), nes_type: nes_t.to_string() });
                reg_count += 1;
            }
        }
    };

    let prio = graph_data_ids(desc);
    for id in &prio { try_add(id); }

    if let Some(chars) = &desc.characteristics {
        for reg in &chars.status { try_add(&reg.id); }
    }

    NesSchema { logical_source_name: ls_name, fields }
}

#[allow(clippy::match_same_arms)]
fn nes_type_to_dsl(t: &str) -> &'static str {
    match t {
        "TEXT" => "BasicType::CHAR",
        "UINT64" => "BasicType::UINT64",
        "INT64" => "BasicType::INT64",
        "FLOAT64" => "BasicType::FLOAT64",
        _ => "BasicType::FLOAT64", // safe fallback
    }
}


pub fn generate_schema_dsl(schema: &NesSchema) -> String {
    let mut dsl = String::from("Schema::create()");
    for f in &schema.fields {
        let _ = write!(dsl, "->addField(createField(\"{}\", {}))", f.name, nes_type_to_dsl(&f.nes_type));
    }
    dsl.push(';');
    dsl
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_descriptor::{Characteristics, DeviceDescription, ParamRef, Register, Service};

    fn mk_reg(id: &str, rt: &str) -> Register {
        Register {
            id: id.to_string(),
            register_type: Some(rt.to_string()),
            name: None, acronym: None, description: None,
            address: None, min_value: None, max_value: None,
            default_value: None, multiplier: None, unit: None,
            is_read_only: None, is_write_only: None, is_visible: None,
            read_access_level: None, write_access_level: None,
            hw_sw_set_mask: None, is_delta: None, in_chart: None,
            fields: vec![],
        }
    }

    fn mk_descriptor(dev_id: &str, status: Vec<Register>) -> DeviceDescriptor {
        DeviceDescriptor {
            yaml_version: None,
            device_description: Some(DeviceDescription {
                device_type: Some("Controller".to_string()),
                device_id: Some(dev_id.to_string()),
                software_version: None, hw_sw_set: None,
                units_handling: vec![], access_level: None,
            }),
            characteristics: Some(Characteristics { parameters: vec![], status }),
            services: vec![],
        }
    }

  
    #[test]
    fn builds_schema_from() {
        let status = vec![
            mk_reg("STATUS_TEMP", "unsigned integer"),
            mk_reg("STATUS_PRESSURE", "integer"),
            mk_reg("STATUS_MODE", "enum"),       
            mk_reg("STATUS_RUNNING", "boolean"),
            mk_reg("STATUS_FLAGS", "bitwise"),
        ];
        let desc = mk_descriptor("0x0007", status);
        let schema = build_schema(&desc, usize::MAX);

        assert_eq!(schema.fields.len(), 6, "3 fixed + 3 registers (enum+bitwise skipped)");
        assert_eq!(schema.logical_source_name, "telemetry_0x0007");

        assert_eq!(schema.fields[0].name, "DEVICE_ID");
        assert_eq!(schema.fields[0].nes_type, "UINT64");
        assert_eq!(schema.fields[1].name, "GATEWAY_ID");
        assert_eq!(schema.fields[1].nes_type, "UINT64");

        assert_eq!(schema.fields[3].name, "STATUS_TEMP");
        assert_eq!(schema.fields[3].nes_type, "UINT64");
        assert_eq!(schema.fields[4].name, "STATUS_PRESSURE");
        assert_eq!(schema.fields[4].nes_type, "FLOAT64");
        assert_eq!(schema.fields[5].name, "STATUS_RUNNING");
        assert_eq!(schema.fields[5].nes_type, "UINT64");
    }


    #[test]
    fn deduplicates_register_ids() {
        let status = vec![
            mk_reg("STATUS_TEMP", "unsigned integer"),
            mk_reg("STATUS_TEMP", "integer"), 
        ];
        let schema = build_schema(&mk_descriptor("0x0007", status), usize::MAX);
        assert_eq!(schema.fields.len(), 4, "duplicate register should be skipped");
        assert_eq!(schema.fields[3].nes_type, "UINT64", "first occurrence type wins");
    }

    #[test]
    fn handles_empty_characteristics() {
        let desc = DeviceDescriptor {
            yaml_version: None,
            device_description: Some(DeviceDescription {
                device_type: Some("Controller".to_string()),
                device_id: Some("0x0007".to_string()),
                software_version: None, hw_sw_set: None,
                units_handling: vec![], access_level: None,
            }),
            characteristics: None,
            services: vec![],
        };

        let schema = build_schema(&desc, usize::MAX);
        assert_eq!(schema.fields.len(), 3); // just the fixed fields
        assert_eq!(schema.fields[0].name, "DEVICE_ID");
        assert_eq!(schema.fields[1].name, "GATEWAY_ID");
        assert_eq!(schema.fields[2].name, "timestamp");
    }

    #[test]
    fn handles_missing_device_id() {
        let desc = DeviceDescriptor {
            yaml_version: None,
            device_description: None,
            characteristics: Some(Characteristics { parameters: vec![], status: vec![] }),
            services: vec![],
        };
        let schema = build_schema(&desc, usize::MAX);
        assert_eq!(schema.logical_source_name, "telemetry_unknown");
    }

    #[test]
    fn generates_schema_dsl_string() {
        let status = vec![
            mk_reg("STATUS_TEMP", "integer"),
            mk_reg("STATUS_RPM", "unsigned integer"),
            mk_reg("STATUS_MODE", "enum"), // skipped
        ];
        let schema = build_schema(&mk_descriptor("0x0007", status), usize::MAX);
        let dsl = generate_schema_dsl(&schema);

        assert!(dsl.starts_with("Schema::create()"));
        assert!(dsl.ends_with(';'));
        assert!(dsl.contains("createField(\"DEVICE_ID\", BasicType::UINT64)"));
        assert!(dsl.contains("createField(\"timestamp\", BasicType::UINT64)"));
        assert!(dsl.contains("createField(\"STATUS_TEMP\", BasicType::FLOAT64)"));
        assert!(dsl.contains("createField(\"STATUS_RPM\", BasicType::UINT64)"));
        assert!(!dsl.contains("STATUS_MODE")); // enum skipped
    }


    #[test]
    fn schema_from_real_descriptor_if_available() {
        let path = std::path::Path::new("devices/V0x0007_1.03V2.yaml");
        if !path.exists() {
            eprintln!("Skipping, descriptor not found at {}", path.display());
            return;
        }
        let desc = crate::device_descriptor::load_descriptor(path)
            .expect("should parse descriptor");
        let schema = build_schema(&desc, usize::MAX);

        assert_eq!(schema.logical_source_name, "telemetry_0x0007");
        assert!(schema.fields.len() > 50, "expected 50+ fields, got {}", schema.fields.len());

        println!("Schema: {} fields", schema.fields.len());
        for f in &schema.fields { println!("  {}: {}", f.name, f.nes_type); }
    }


    #[test]
    fn max_register_fields_caps_schema_size() {
        let desc = DeviceDescriptor {
            yaml_version: None,
            device_description: Some(DeviceDescription {
                device_type: Some("Controller".to_string()),
                device_id: Some("0x0007".to_string()),
                software_version: None, hw_sw_set: None,
                units_handling: vec![], access_level: None,
            }),
            characteristics: Some(Characteristics {
                status: vec![
                    mk_reg("REG_A", "integer"), mk_reg("REG_B", "integer"),
                    mk_reg("REG_C", "integer"), mk_reg("REG_D", "integer"),
                    mk_reg("REG_E", "integer"),
                ],
                parameters: vec![],
            }),
            services: vec![],
        };

        let schema = build_schema(&desc, 3);
        assert_eq!(schema.fields.len(), 6, "3 fixed + 3 register = 6");
        assert_eq!(schema.fields[3].name, "REG_A");
        assert_eq!(schema.fields[4].name, "REG_B");
        assert_eq!(schema.fields[5].name, "REG_C");

        // zero cap means only fixed fields
        let schema = build_schema(&desc, 0);
        assert_eq!(schema.fields.len(), 3, "only 3 fixed fields");
    }

    #[test]
    fn prioritises_graph() {
        let status = vec![
            mk_reg("REG_A", "integer"), mk_reg("REG_B", "integer"),
            mk_reg("REG_C", "integer"), mk_reg("REG_D", "integer"),
            mk_reg("REG_E", "integer"),
        ];
        let services = vec![Service {
            id: Some("SERVICE_DATA_ACQUISITION".to_string()),
            config_groups: vec![], table_data: vec![],
            graph_data: vec![
                ParamRef { id: Some("REG_D".to_string()) },
                ParamRef { id: Some("REG_E".to_string()) },
            ],
            storage_data: vec![],
        }];
        let desc = DeviceDescriptor {
            yaml_version: None,
            device_description: Some(DeviceDescription {
                device_type: Some("Controller".to_string()),
                device_id: Some("0x0007".to_string()),
                software_version: None, hw_sw_set: None,
                units_handling: vec![], access_level: None,
            }),
            characteristics: Some(Characteristics { status, parameters: vec![] }),
            services,
        };

        let schema = build_schema(&desc, 3);
        let names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(schema.fields.len(), 6, "3 fixed + 3 register = 6");
        assert!(names.contains(&"REG_D"));
        assert!(names.contains(&"REG_E"));
        assert_eq!(schema.fields[3].name, "REG_D", "graph_data first");
        assert_eq!(schema.fields[4].name, "REG_E", "graph_data second");
        assert_eq!(schema.fields[5].name, "REG_A", "fill from status list");
    }

    #[test]
    fn graph_data_truncated_by_cap() {
        let status = vec![
            mk_reg("REG_A", "integer"), mk_reg("REG_B", "integer"),
            mk_reg("REG_C", "integer"),
        ];
        let services = vec![Service {
            id: Some("SERVICE_DATA_ACQUISITION".to_string()),
            config_groups: vec![], table_data: vec![],
            graph_data: vec![
                ParamRef { id: Some("REG_C".to_string()) },
                ParamRef { id: Some("REG_B".to_string()) },
                ParamRef { id: Some("REG_A".to_string()) },
            ],
            storage_data: vec![],
        }];
        let desc = DeviceDescriptor {
            yaml_version: None,
            device_description: Some(DeviceDescription {
                device_type: Some("Controller".to_string()),
                device_id: Some("0x0007".to_string()),
                software_version: None, hw_sw_set: None,
                units_handling: vec![], access_level: None,
            }),
            characteristics: Some(Characteristics { status, parameters: vec![] }),
            services,
        };

        let schema = build_schema(&desc, 2);
        assert_eq!(schema.fields.len(), 5, "3 fixed + 2 register = 5");
        assert_eq!(schema.fields[3].name, "REG_C");
        assert_eq!(schema.fields[4].name, "REG_B");
    }
}
