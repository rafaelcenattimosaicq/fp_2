// fetches device policies (descriptors) from the cloud API.
//
// the API is a Lambda behind API Gateway that reads YAML files from S3.
// each device has exactly ONE policy, the one-policy-per-device rule is
// enforced by the Lambda, not by us. If somehow the API returns multiple
// policies for a device we just take the first one and hope for the best.
//
// this whole module is dead code right now (cloud sync was descoped).
// the call path from main.rs that would invoke fetch_descriptor_for_device()
// was removed in commit d99146e6 when we reverted the Cloud Map changes.

use crate::cloud::auth::TokenManager;
use crate::device_descriptor::DeviceDescriptor;
use serde::Deserialize;

// aPI responses. The list endpoint returns an array of these.
#[derive(Debug, Deserialize)]
struct PolicyItem {
    name: String,
    // lambda also sends createdAt, updatedAt, deviceId... we ignore them
}

#[derive(Debug, Deserialize)]
struct PolicyBody {
    content: String, // raw YAML string of the descriptor
}

/// fetches the policy descriptor for a single device from the cloud API.
/// returns None if the device has no policy assigned yet (new/unprovisioned device).
///
/// the flow: GET /policies?deviceId=0xABCD -> take first result -> GET /policies/{name}
/// -> parse the YAML content field into a DeviceDescriptor.
///
/// we re-use the same Bearer token for both calls. In theory the token
/// could expire between them if the Lambda cold-starts badly, but the 60s
/// buffer in TokenManager should cover it (fingers crossed).
pub async fn fetch_descriptor_for_device(
    tok_mgr: &mut TokenManager,
    api_url: &str,
    dev_id: &str,
) -> Result<Option<(String, DeviceDescriptor)>, Box<dyn std::error::Error + Send + Sync>> {
    let tok = tok_mgr.get_token().await?;
    let cl = tok_mgr.http_client();

    // step 1: list policies for this device
    let u = format!("{api_url}/policies");
    let r = cl
        .get(&u)
        .query(&[("deviceId", dev_id)])
        .header("Authorization", format!("Bearer {tok}"))
        .send().await?;

    if !r.status().is_success() {
        // aPI returns 404 for devices that aren't in the system at all,
        // vs empty array for devices that exist but have no policy.
        // we treat both as "no policy" because the gateway doesn't care
        // about the distinction, either way we fall back to local YAML.
        if r.status().as_u16() == 404 {
            return Ok(None);
        }
        let s = r.status();
        let tmp = r.text().await.unwrap_or_default();
        return Err(format!("policies list failed ({s}): {tmp}").into());
    }

    let v: Vec<PolicyItem> = r.json().await?;
    if v.is_empty() { return Ok(None); }

    // one-policy-per-device: just grab [0]
    let n = &v[0].name;

    // step 2: fetch the actual policy content
    let u2 = format!("{api_url}/policies/{n}");
    let r2 = cl.get(&u2)
        .header("Authorization", format!("Bearer {tok}"))
        .send().await?
        .error_for_status()?;

    let data: PolicyBody = r2.json().await?;

    // parse the YAML. This can fail if someone uploads a malformed policy
    // to S3, happened once during testing when a policy had tabs instead
    // of spaces (classic YAML footgun).
    let d: DeviceDescriptor = serde_yaml::from_str(&data.content)
        .map_err(|e| format!("bad YAML in policy '{n}': {e}"))?;

    Ok(Some((n.clone(), d)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // verify that the YAML we get back from the API actually deserializes
    // into a DeviceDescriptor. This is a real-ish policy trimmed down.
    #[test]
    fn parses_the client_style_yaml() {
        let yaml = r#"
yaml_version: "1.00.00"
characteristics:
  parameters: []
  status:
    - id: "STATUS_TEMP"
      type: "integer"
      address: 100
      multiplier: 10.0
services: []
"#;
        let desc: DeviceDescriptor = serde_yaml::from_str(yaml)
            .expect("should parse the client-style descriptor");

        let chars = desc.characteristics.expect("characteristics present");
        assert_eq!(chars.status.len(), 1);
        assert_eq!(chars.status[0].id, "STATUS_TEMP");
        // address 100 = 0x64 = compressor discharge temperature on the client VCC units
        assert_eq!(chars.status[0].address, Some(100));
    }

    #[test]
    fn policy_item_from_json() {
        // lambda response includes extra fields we don't care about
        let json = r#"{"name": "the client-vcc3-v7", "createdAt": "2025-01-15T10:00:00Z", "deviceId": "0x1234"}"#;
        let item: PolicyItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.name, "the client-vcc3-v7");
    }

    #[test]
    fn policy_body_content_field() {
        let json = r#"{"name": "the client-vcc3-v7", "content": "yaml_version: 1\nservices: []"}"#;
        let body: PolicyBody = serde_json::from_str(json).unwrap();
        assert!(body.content.starts_with("yaml_version"));
    }
}
