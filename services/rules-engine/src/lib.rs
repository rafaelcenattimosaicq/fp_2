use serde::{Deserialize, Serialize};

pub mod mqtt_options;
pub mod s3_sink;

// f32 -> f64 cast can produce tiny rounding errors
const F64_EQ_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub field: String,
    pub op: Op,
    pub value: f64,
    #[serde(default)]
    pub publish_topic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Gt, Gte,
    Lt, Lte,
    Eq, Neq,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub device_id: String,
    pub rule_id: String,
    pub publish_topic: String,
}

// gets device id from mqtt topic like "telemetry/dev-1/sensors"
#[allow(unused)]
pub fn extract_device_id<'a>(prefix: &str, topic: &'a str) -> Option<&'a str> {
    let p = prefix.trim_end_matches('#');
    let p = p.trim_end_matches('/');
    let t = topic.trim_start_matches('/');
    let t = t.strip_prefix(p)?;
    let t = t.strip_prefix('/')?;
    let (ret, _) = t.split_once('/').unwrap_or((t, ""));
    if ret.is_empty() { None } else { Some(ret) }
}

pub fn evaluate(policy: &Policy, device_id: &str, telemetry: &serde_json::Value) -> Vec<Action> {
    let mut ret = Vec::new();

    for r in &policy.rules {
        let Some(val) = telemetry.get(&r.field).and_then(|v| v.as_f64()) else {
            continue;
        };

        let ok = match r.op {
            Op::Gt => val > r.value,
            Op::Gte => val >= r.value,
            Op::Lt => val < r.value,
            Op::Lte => val <= r.value,
            // dont use exact f64 equality, discharge_temp came back as 72.00000001 once
            Op::Eq => (val - r.value).abs() < F64_EQ_EPSILON,
            Op::Neq => (val - r.value).abs() >= F64_EQ_EPSILON,
        };

        if !ok { continue; }

        let tmp = r.publish_topic.clone()
            .unwrap_or_else(|| format!("alerts/{device_id}"));

        ret.push(Action {
            device_id: device_id.to_string(),
            rule_id: r.id.clone(),
            publish_topic: tmp,
        });
    }

    ret
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_device_id_from_simple_topic() {
        assert_eq!(
            extract_device_id("telemetry", "telemetry/dev-1"),
            Some("dev-1")
        );
    }

    #[test]
    fn extract_device_id_ignores_extra_segments() {
        assert_eq!(
            extract_device_id("telemetry", "telemetry/dev-1/sensors/temp"),
            Some("dev-1")
        );
    }

    #[test]
    fn extract_device_id_rejects_missing_device_id() {
        assert_eq!(extract_device_id("telemetry", "telemetry/"), None);
        assert_eq!(extract_device_id("telemetry", "telemetry"), None);
    }

    #[test]
    fn evaluate_triggers_numeric_gt_rule() {
        let policy = Policy {
            rules: vec![Rule {
                id: "over_temp".to_string(),
                field: "temperature".to_string(),
                op: Op::Gt,
                value: 50.0,
                publish_topic: None,
            }],
        };

        let telemetry = json!({"temperature": 55.2});
        let actions = evaluate(&policy, "dev-1", &telemetry);

        assert_eq!(
            actions,
            vec![Action {
                device_id: "dev-1".to_string(),
                rule_id: "over_temp".to_string(),
                publish_topic: "alerts/dev-1".to_string(),
            }]
        );
    }

    #[test]
    fn evaluate_does_not_trigger_when_field_missing() {
        let policy = Policy {
            rules: vec![Rule {
                id: "over_temp".to_string(),
                field: "temperature".to_string(),
                op: Op::Gt,
                value: 50.0,
                publish_topic: None,
            }],
        };

        let telemetry = json!({"humidity": 10.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty());
    }

    #[test]
    fn extract_device_id_prefix_with_trailing_slash() {
        assert_eq!(
            extract_device_id("telemetry/", "telemetry/dev-42"),
            Some("dev-42")
        );
    }

    #[test]
    fn extract_device_id_prefix_with_hash_wildcard() {
        assert_eq!(
            extract_device_id("telemetry/#", "telemetry/dev-99/data"),
            Some("dev-99")
        );
    }

    #[test]
    fn extract_device_id_rejects_unrelated_topic() {
        assert_eq!(
            extract_device_id("telemetry", "policies/dev-1"),
            None
        );
    }

    #[test]
    fn extract_device_id_leading_slash_on_topic() {
        assert_eq!(
            extract_device_id("telemetry", "/telemetry/dev-1"),
            Some("dev-1")
        );
    }

    #[test]
    fn extract_device_id_numeric_device_id() {
        assert_eq!(
            extract_device_id("telemetry", "telemetry/12345"),
            Some("12345")
        );
    }

    #[test]
    fn extract_device_id_hex_device_id() {
        assert_eq!(
            extract_device_id("telemetry", "telemetry/0xABCD"),
            Some("0xABCD")
        );
    }

    #[test]
    fn evaluate_triggers_gte_at_boundary() {
        let policy = Policy {
            rules: vec![Rule {
                id: "gte_check".to_string(),
                field: "temp".to_string(),
                op: Op::Gte,
                value: 50.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temp": 50.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1, "Gte should trigger when value == threshold");
    }

    #[test]
    fn evaluate_does_not_trigger_gte_below_threshold() {
        let policy = Policy {
            rules: vec![Rule {
                id: "gte_check".to_string(),
                field: "temp".to_string(),
                op: Op::Gte,
                value: 50.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temp": 49.9});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty(), "Gte should not trigger when value < threshold");
    }

    #[test]
    fn evaluate_triggers_lt_rule() {
        let policy = Policy {
            rules: vec![Rule {
                id: "under_pressure".to_string(),
                field: "pressure".to_string(),
                op: Op::Lt,
                value: 1.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"pressure": 0.5});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_id, "under_pressure");
    }

    #[test]
    fn evaluate_does_not_trigger_lt_at_boundary() {
        let policy = Policy {
            rules: vec![Rule {
                id: "lt_check".to_string(),
                field: "temp".to_string(),
                op: Op::Lt,
                value: 50.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temp": 50.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty(), "Lt should not trigger when value == threshold");
    }

    #[test]
    fn evaluate_triggers_lte_at_boundary() {
        let policy = Policy {
            rules: vec![Rule {
                id: "lte_check".to_string(),
                field: "temp".to_string(),
                op: Op::Lte,
                value: 30.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temp": 30.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1, "Lte should trigger when value == threshold");
    }

    #[test]
    fn evaluate_triggers_eq_rule() {
        let policy = Policy {
            rules: vec![Rule {
                id: "exact_match".to_string(),
                field: "status".to_string(),
                op: Op::Eq,
                value: 1.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"status": 1.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_id, "exact_match");
    }

    #[test]
    fn evaluate_does_not_trigger_eq_when_different() {
        let policy = Policy {
            rules: vec![Rule {
                id: "eq_check".to_string(),
                field: "status".to_string(),
                op: Op::Eq,
                value: 1.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"status": 2.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty());
    }

    #[test]
    fn evaluate_triggers_neq_rule() {
        let policy = Policy {
            rules: vec![Rule {
                id: "not_idle".to_string(),
                field: "state".to_string(),
                op: Op::Neq,
                value: 0.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"state": 5.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_id, "not_idle");
    }

    #[test]
    fn evaluate_does_not_trigger_neq_when_equal() {
        let policy = Policy {
            rules: vec![Rule {
                id: "neq_check".to_string(),
                field: "state".to_string(),
                op: Op::Neq,
                value: 0.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"state": 0.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty());
    }

    #[test]
    fn evaluate_multiple_rules_partial_trigger() {
        let policy = Policy {
            rules: vec![
                Rule {
                    id: "high_temp".to_string(),
                    field: "temperature".to_string(),
                    op: Op::Gt,
                    value: 50.0,
                    publish_topic: None,
                },
                Rule {
                    id: "low_battery".to_string(),
                    field: "battery".to_string(),
                    op: Op::Lt,
                    value: 20.0,
                    publish_topic: None,
                },
            ],
        };
        let telemetry = json!({"temperature": 60.0, "battery": 80.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].rule_id, "high_temp");
    }

    #[test]
    fn evaluate_all_rules_triggered() {
        let policy = Policy {
            rules: vec![
                Rule {
                    id: "high_temp".to_string(),
                    field: "temperature".to_string(),
                    op: Op::Gt,
                    value: 50.0,
                    publish_topic: None,
                },
                Rule {
                    id: "low_battery".to_string(),
                    field: "battery".to_string(),
                    op: Op::Lt,
                    value: 20.0,
                    publish_topic: None,
                },
            ],
        };
        let telemetry = json!({"temperature": 60.0, "battery": 5.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn evaluate_empty_policy_returns_no_actions() {
        let policy = Policy { rules: vec![] };
        let telemetry = json!({"temperature": 100.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty());
    }

    #[test]
    fn evaluate_uses_custom_publish_topic() {
        let policy = Policy {
            rules: vec![Rule {
                id: "r1".to_string(),
                field: "temp".to_string(),
                op: Op::Gt,
                value: 0.0,
                publish_topic: Some("custom/alerts/high-temp".to_string()),
            }],
        };
        let telemetry = json!({"temp": 10.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].publish_topic, "custom/alerts/high-temp",
            "should use the rule's custom publish_topic"
        );
    }

    #[test]
    fn evaluate_default_publish_topic_includes_device_id() {
        let policy = Policy {
            rules: vec![Rule {
                id: "r1".to_string(),
                field: "temp".to_string(),
                op: Op::Gt,
                value: 0.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temp": 10.0});
        let actions = evaluate(&policy, "dev-xyz", &telemetry);
        assert_eq!(actions[0].publish_topic, "alerts/dev-xyz");
    }

    #[test]
    fn evaluate_skips_non_numeric_field_value() {
        let policy = Policy {
            rules: vec![Rule {
                id: "r1".to_string(),
                field: "temperature".to_string(),
                op: Op::Gt,
                value: 50.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temperature": "hot"});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(
            actions.is_empty(),
            "non-numeric field values should be skipped, not cause errors"
        );
    }

    #[test]
    fn evaluate_action_contains_correct_device_id() {
        let policy = Policy {
            rules: vec![Rule {
                id: "r1".to_string(),
                field: "v".to_string(),
                op: Op::Gt,
                value: 0.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"v": 1.0});
        let actions = evaluate(&policy, "sensor-0xABCD", &telemetry);
        assert_eq!(actions[0].device_id, "sensor-0xABCD");
    }

    #[test]
    fn policy_serde_round_trip() {
        let policy = Policy {
            rules: vec![Rule {
                id: "r1".to_string(),
                field: "temp".to_string(),
                op: Op::Gt,
                value: 42.5,
                publish_topic: Some("alerts/custom".to_string()),
            }],
        };
        let json_str = serde_json::to_string(&policy).expect("serialize");
        let deserialized: Policy = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(policy, deserialized);
    }

    #[test]
    fn op_serialization_uses_snake_case() {
        assert_eq!(serde_json::to_string(&Op::Gt).unwrap(), "\"gt\"");
        assert_eq!(serde_json::to_string(&Op::Gte).unwrap(), "\"gte\"");
        assert_eq!(serde_json::to_string(&Op::Lt).unwrap(), "\"lt\"");
        assert_eq!(serde_json::to_string(&Op::Lte).unwrap(), "\"lte\"");
        assert_eq!(serde_json::to_string(&Op::Eq).unwrap(), "\"eq\"");
        assert_eq!(serde_json::to_string(&Op::Neq).unwrap(), "\"neq\"");
    }

    #[test]
    fn rule_publish_topic_defaults_to_none() {
        let json_str = r#"{"id":"r1","field":"temp","op":"gt","value":10.0}"#;
        let rule: Rule = serde_json::from_str(json_str).expect("deserialize");
        assert_eq!(rule.publish_topic, None, "publish_topic should default to None");
    }

    #[test]
    fn evaluate_gt_does_not_trigger_at_boundary() {
        let policy = Policy {
            rules: vec![Rule {
                id: "gt_boundary".to_string(),
                field: "temp".to_string(),
                op: Op::Gt,
                value: 50.0,
                publish_topic: None,
            }],
        };
        let telemetry = json!({"temp": 50.0});
        let actions = evaluate(&policy, "dev-1", &telemetry);
        assert!(actions.is_empty(), "Gt should not trigger when value == threshold");
    }
}
