use crate::device_descriptor::RegisterValue;
use crate::nes::schema::NesSchema;
use std::collections::HashMap;


fn fnv1a_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(test)]
fn build_csv_line(
    schema: &NesSchema,
    gw_id: &str,
    dev_id: &str,
    ts_ms: i64,
    vals: &HashMap<String, RegisterValue>,
) -> String {
    let parts: Vec<String> = schema.fields.iter().map(|f| {
        match f.name.as_str() {
            "DEVICE_ID" => fnv1a_hash(dev_id).to_string(),
            "GATEWAY_ID" => fnv1a_hash(gw_id).to_string(),
            "timestamp" => ts_ms.to_string(),
            _ => match vals.get(&f.name) {
                Some(RegisterValue::Float(v)) => v.to_string(),
                Some(RegisterValue::Unsigned(v)) => v.to_string(),
                Some(RegisterValue::Boolean(b)) => if *b { "1" } else { "0" }.to_string(),
                Some(RegisterValue::Enum(s)) => s.clone(),
                Some(RegisterValue::Bitwise(_)) => "0".to_string(), // bitwise not supported in NES
                None => default_for_type(&f.nes_type),
            },
        }
    }).collect();

    parts.join(",")
}


pub fn build_json_line(
    schema: &NesSchema,
    gw_id: &str,
    dev_id: &str,
    ts_ms: i64,
    vals: &HashMap<String, RegisterValue>,
) -> String {
    let pairs: Vec<String> = schema.fields.iter().map(|f| {
        let v = match f.name.as_str() {
            "DEVICE_ID" => fnv1a_hash(dev_id).to_string(),
            "GATEWAY_ID" => fnv1a_hash(gw_id).to_string(),
            "timestamp" => ts_ms.to_string(),
            _ => match vals.get(&f.name) {
                Some(RegisterValue::Float(v)) => v.to_string(),
                Some(RegisterValue::Unsigned(v)) => v.to_string(),
                Some(RegisterValue::Boolean(b)) => if *b { "1" } else { "0" }.to_string(),
                Some(RegisterValue::Enum(s)) => s.clone(),
                Some(RegisterValue::Bitwise(_)) => "0".to_string(),
                None => default_for_type(&f.nes_type),
            },
        };
        format!("\"{}\":{}", f.name, v)
    }).collect();

    format!("{{{}}}", pairs.join(","))
}

fn default_for_type(nes_type: &str) -> String {
    match nes_type {
        "UINT64" | "FLOAT64" => "0".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::schema::{NesField, NesSchema};

    fn f(name: &str, ty: &str) -> NesField {
        NesField { name: name.to_string(), nes_type: ty.to_string() }
    }

    // verify CSV output matches schema field order exactly. If fields are
    // out of order the NES worker maps values to wrong columns and you get
    // garbage in the query results (ask me how I know).
    #[test]
    fn builds_csv_with_all_fields() {
        let schema = NesSchema {
            logical_source_name: "telemetry_test".to_string(),
            fields: vec![
                f("DEVICE_ID", "UINT64"),
                f("GATEWAY_ID", "UINT64"),
                f("timestamp", "UINT64"),
                f("STATUS_ID_TEMP", "FLOAT64"),
                f("STATUS_ID_RPM", "UINT64"),
            ],
        };
        let mut vals = HashMap::new();
        vals.insert("STATUS_ID_TEMP".to_string(), RegisterValue::Float(25.5));
        vals.insert("STATUS_ID_RPM".to_string(), RegisterValue::Unsigned(3000));

        let csv = build_csv_line(&schema, "gw-001", "dev-42", 1_700_000_000_000, &vals);

        let dh = fnv1a_hash("dev-42");
        let gh = fnv1a_hash("gw-001");
        assert_eq!(csv, format!("{dh},{gh},1700000000000,25.5,3000"));
    }

    // missing register values should produce "0" not empty strings, because
    // nES CSV parser treats empty fields as parse errors and drops the whole row
    #[test]
    fn handles_missing_values() {
        let schema = NesSchema {
            logical_source_name: "telemetry_test".to_string(),
            fields: vec![
                f("DEVICE_ID", "UINT64"), f("GATEWAY_ID", "UINT64"),
                f("timestamp", "UINT64"),
                f("STATUS_ID_TEMP", "FLOAT64"), f("STATUS_ID_RPM", "UINT64"),
            ],
        };

        let csv = build_csv_line(&schema, "gw-001", "dev-42", 1_700_000_000_000, &HashMap::new());

        let dh = fnv1a_hash("dev-42");
        let gh = fnv1a_hash("gw-001");
        assert_eq!(csv, format!("{dh},{gh},1700000000000,0,0"));
    }

    #[test]
    fn encodes_boolean_values() {
        let schema = NesSchema {
            logical_source_name: "telemetry_test".to_string(),
            fields: vec![
                f("DEVICE_ID", "UINT64"), f("GATEWAY_ID", "UINT64"),
                f("timestamp", "UINT64"), f("STATUS_ACTIVE", "UINT64"),
            ],
        };
        let mut vals = HashMap::new();
        vals.insert("STATUS_ACTIVE".to_string(), RegisterValue::Boolean(true));

        let csv = build_csv_line(&schema, "gw", "dev", 123, &vals);
        let dh = fnv1a_hash("dev");
        let gh = fnv1a_hash("gw");
        assert_eq!(csv, format!("{dh},{gh},123,1"));
    }

    #[test]
    fn builds_json_with_all_fields() {
        let schema = NesSchema {
            logical_source_name: "telemetry_test".to_string(),
            fields: vec![
                f("DEVICE_ID", "UINT64"), f("GATEWAY_ID", "UINT64"),
                f("timestamp", "UINT64"),
                f("STATUS_ID_TEMP", "FLOAT64"), f("STATUS_ID_RPM", "UINT64"),
            ],
        };
        let mut vals = HashMap::new();
        vals.insert("STATUS_ID_TEMP".to_string(), RegisterValue::Float(25.5));
        vals.insert("STATUS_ID_RPM".to_string(), RegisterValue::Unsigned(3000));

        let json = build_json_line(&schema, "gw-001", "dev-42", 1_700_000_000_000, &vals);

        let dh = fnv1a_hash("dev-42");
        let gh = fnv1a_hash("gw-001");
        let expected = format!(
            "{{\"DEVICE_ID\":{dh},\"GATEWAY_ID\":{gh},\"timestamp\":1700000000000,\"STATUS_ID_TEMP\":25.5,\"STATUS_ID_RPM\":3000}}"
        );
        assert_eq!(json, expected);
    }
}
