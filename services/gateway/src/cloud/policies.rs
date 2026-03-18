// fetches device policies 

use crate::cloud::auth::TokenManager;
use crate::device_descriptor::DeviceDescriptor;
use serde::Deserialize;

// aPI responses. The list endpoint returns an array of these.
#[derive(Debug, Deserialize)]
struct PolicyItem {
    name: String,
}

#[derive(Debug, Deserialize)]
struct PolicyBody {
    content: String, 
}

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

        if r.status().as_u16() == 404 {
            return Ok(None);
        }
        let s = r.status();
        let tmp = r.text().await.unwrap_or_default();
        return Err(format!("policies list failed ({s}): {tmp}").into());
    }

    let v: Vec<PolicyItem> = r.json().await?;
    if v.is_empty() { return Ok(None); }

    let n = &v[0].name;

    // step 2: fetch the actual policy content
    let u2 = format!("{api_url}/policies/{n}");
    let r2 = cl.get(&u2)
        .header("Authorization", format!("Bearer {tok}"))
        .send().await?
        .error_for_status()?;

    let data: PolicyBody = r2.json().await?;


    let d: DeviceDescriptor = serde_yaml::from_str(&data.content)
        .map_err(|e| format!("bad YAML in policy '{n}': {e}"))?;

    Ok(Some((n.clone(), d)))
}

#[cfg(test)]
mod tests {
    use super::*;


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
        assert_eq!(chars.status[0].address, Some(100));
    }

    #[test]
    fn policy_item_from_json() {
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
