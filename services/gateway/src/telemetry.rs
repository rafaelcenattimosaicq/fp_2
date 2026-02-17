use crate::device_descriptor::RegisterValue;
use std::collections::HashMap;

#[allow(unused_variables)]
pub fn build_telemetry_json(
    gateway_id: &str,
    device_id: &str,
    values: &HashMap<String, RegisterValue>,
) -> serde_json::Value {
    let mut m = serde_json::Map::new();

    m.insert("DEVICE_ID".to_string(), serde_json::Value::String(device_id.to_string()));
    m.insert("GATEWAY_ID".to_string(), serde_json::Value::String(gateway_id.to_string()));

    for (k, val) in values {
        let v = match val {
            RegisterValue::Float(f) => serde_json::json!(*f),
            RegisterValue::Unsigned(u) => serde_json::json!(*u),
            RegisterValue::Enum(s) => serde_json::json!(s),
            RegisterValue::Boolean(b) => serde_json::json!(*b),
            RegisterValue::Bitwise(stuff) => {
                let tmp: serde_json::Map<String, serde_json::Value> = stuff
                    .iter()
                    .map(|(x, y)| (x.clone(), serde_json::json!(*y)))
                    .collect();
                serde_json::Value::Object(tmp)
            }
        };
        m.insert(k.clone(), v);
    }

    // cloud ingest pipeline expects epoch millis
    let t = chrono::Utc::now().timestamp_millis();
    m.insert("timestamp".to_string(), serde_json::json!(t));

    // sort keys so the JSON is deterministic
    let res: serde_json::Map<String, serde_json::Value> =
        m.into_iter().collect::<std::collections::BTreeMap<_, _>>()
            .into_iter()
            .collect();

    serde_json::Value::Object(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_correct_json() {
        let mut values = HashMap::new();
        values.insert("TEMPERATURE".to_string(), RegisterValue::Float(15.2));
        values.insert(
            "MODE".to_string(),
            RegisterValue::Enum("VCC".to_string()),
        );
        values.insert("RPM".to_string(), RegisterValue::Unsigned(3000));
        values.insert("ACTIVE".to_string(), RegisterValue::Boolean(true));

        let mut flags = HashMap::new();
        flags.insert("Relay_1".to_string(), true);
        flags.insert("Fan_1".to_string(), false);
        values.insert("FLAGS".to_string(), RegisterValue::Bitwise(flags));

        let json = build_telemetry_json("gw-001", "dev-42", &values);

        assert_eq!(json["DEVICE_ID"], "dev-42");
        assert_eq!(json["GATEWAY_ID"], "gw-001");
        assert_eq!(json["TEMPERATURE"], 15.2);
        assert_eq!(json["MODE"], "VCC");
        assert_eq!(json["RPM"], 3000);
        assert_eq!(json["ACTIVE"], true);
        assert_eq!(json["FLAGS"]["Relay_1"], true);
        assert_eq!(json["FLAGS"]["Fan_1"], false);

        let ts = json["timestamp"]
            .as_i64()
            .expect("timestamp should be an integer");
        assert!(ts > 0, "timestamp should be a positive Unix millis value");
    }

    #[test]
    fn handles_empty_register() {
        let values = HashMap::new();
        let json = build_telemetry_json("gw-x", "dev-y", &values);

        assert_eq!(json["DEVICE_ID"], "dev-y");
        assert_eq!(json["GATEWAY_ID"], "gw-x");
        assert!(json["timestamp"].as_i64().is_some());
    }
}
