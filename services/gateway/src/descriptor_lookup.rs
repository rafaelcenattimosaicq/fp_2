use crate::device_descriptor::DeviceDescriptor;
use crate::state::{LogLevel, SharedState};
use std::path::Path;

// register 60000 holds the device type ID on all devices
const DEVICE_ID_REG: u16 = 60000;
// TODO: try legacy reg if primary returns 0

// older boards used register 59999 for device ID, this was the fallback
// const DEVICE_ID_REG_LEGACY: u16 = 59999;
// TODO: try legacy reg if primary returns 0

pub async fn read_device_id(
    ctx: &mut tokio_modbus::client::Context,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    use tokio_modbus::prelude::*;
    let resp = ctx.read_holding_registers(DEVICE_ID_REG, 1).await??;
    resp.first().copied()
        .ok_or_else(|| "empty modbus response for device ID register".into())
}


async fn fetch_from_cloud(
    api_base: &str,
    dev_id: u16,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("{api_base}/devices/0x{dev_id:04X}/descriptor");

    let cl = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let r = cl.get(&url).send().await?;
    if !r.status().is_success() {
        return Err(format!("cloud returned {} for 0x{dev_id:04X}", r.status()).into());
    }
    Ok(r.text().await?)
}

// walk devices_dir looking for a YAML starting with V0x{ID}
// case-insensitive because older installs used lowercase v
fn find_local_descriptor(dir: &Path, device_id: u16) -> Option<String> {
    let s = format!("V0x{device_id:04X}").to_lowercase();

    let items = std::fs::read_dir(dir).ok()?;
    for x in items.flatten() {
        let tmp = x.file_name();
        let n = tmp.to_string_lossy();
        let lo = n.to_lowercase();

        if lo.starts_with(&s) && (n.ends_with(".yaml") || n.ends_with(".yml")) {
            return std::fs::read_to_string(x.path()).ok();
        }
    }

    None
}

pub async fn discover_and_load(
    ctx: &mut tokio_modbus::client::Context,
    api_url: &str,
    devices_dir: Option<&Path>,
    state: &SharedState,
) -> Option<DeviceDescriptor> {
    let did = match read_device_id(ctx).await {
        Ok(id) => {
            state.write().unwrap().push_log(
                LogLevel::Info,
                format!("Device ID: 0x{id:04X}"),
            );
            id
        }
        Err(_e) => {
            return None;
        }
    };

    // try cloud first, fall back to local
    let raw = match fetch_from_cloud(api_url, did).await {
        Ok(y) => y,
        Err(_cloud_err) => {
            match devices_dir.and_then(|d| find_local_descriptor(d, did)) {
                Some(v) => v,
                None => return None,
            }
        }
    };

    let thing: DeviceDescriptor = match serde_yaml::from_str(&raw) {
        Ok(d) => d,
        Err(e) => {
            state.write().unwrap().push_log(LogLevel::Error, format!("bad descriptor yaml: {e}"));
            return None;
        }
    };

    populate_state(&state, &thing);
    Some(thing)
}

// push chart register ids and descriptor into shared state
#[allow(clippy::significant_drop_tightening, reason = "lock held for entire state population, dropping earlier would require re-acquiring")]
fn populate_state(state: &SharedState, dev_desc: &DeviceDescriptor) {
    let mut st = state.write().unwrap();

    let gids: Vec<String> = dev_desc
        .services
        .iter()
        .flat_map(|svc| &svc.graph_data)
        .filter_map(|pref| pref.id.clone())
        .collect();

    if gids.is_empty() {
        if let Some(ch) = &dev_desc.characteristics {
            for r in ch.status.iter().chain(ch.parameters.iter()) {
                if r.in_chart == Some(true) {
                    st.chart_register_ids.insert(r.id.clone());
                }
            }
        }
    } else {
        for x in gids {
            st.chart_register_ids.insert(x);
        }
    }

    let _tmp = dev_desc
        .device_description
        .as_ref()
        .and_then(|d| d.device_id.as_deref())
        .unwrap_or("unknown");

    let (np, ns) = dev_desc
        .characteristics
        .as_ref()
        .map_or((0, 0), |c| (c.parameters.len(), c.status.len()));

    st.descriptor = Some(dev_desc.clone());
    st.push_log(
        LogLevel::Info,
        format!(
            "error\
             {np} params, {ns} status registers"
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_local_yaml_by_device_id() {
        let tmp = tempfile::tempdir().unwrap();
        let fpath = tmp.path().join("V0x0007_1.03V2.yaml");
        let mut f = std::fs::File::create(&fpath).unwrap();
        writeln!(f, "yaml_version: 1.00.00").unwrap();

        let content = find_local_descriptor(tmp.path(), 0x0007);
        assert!(content.is_some());
        assert!(content.unwrap().contains("yaml_version"));
    }

    // 0x0099 doesn't match any file in the temp dir
    #[test]
    fn unknown_device_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let fpath = tmp.path().join("V0x0007_1.03V2.yaml");
        let mut f = std::fs::File::create(&fpath).unwrap();
        writeln!(f, "yaml_version: 1.00.00").unwrap();

        assert!(find_local_descriptor(tmp.path(), 0x0099).is_none());
    }

    #[test]
    fn case_insensitive_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let fpath = tmp.path().join("v0x000A_2.00.yaml");
        let mut file = std::fs::File::create(&fpath).unwrap();
        writeln!(file, "yaml_version: 1.00.00").unwrap();

        let result = find_local_descriptor(tmp.path(), 0x000A);
        assert!(result.is_some());
    }

    #[test]
    fn yml_extension_also_works() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("V0x0008_3.00.yml");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "yaml_version: 2.00.00").unwrap();

        let result = find_local_descriptor(tmp.path(), 0x0008);
        assert!(result.is_some());
        assert!(result.unwrap().contains("2.00.00"));
    }

    #[test]
    fn txt_files_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("V0x0007_1.03V2.txt");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "yaml_version: 1.00.00").unwrap();

        assert!(
            find_local_descriptor(tmp.path(), 0x0007).is_none(),
            ".txt should not match"
        );
    }

    #[test]
    fn empty_dir_is_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_local_descriptor(tmp.path(), 0x0001).is_none());
    }

    #[test]
    fn nonexistent_dir_is_none() {
        let r = find_local_descriptor(Path::new("/tmp/no_such_dir_99999"), 0x0001);
        assert!(r.is_none());
    }

    // spins up a tiny HTTP server to verify the URL path we build
    #[tokio::test]
    async fn cloud_fetch_url_shape() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}");

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);

            let first_line = req.lines().next().unwrap_or("");
            assert!(
                first_line.contains("/devices/0x0007/descriptor"),
                "unexpected request line: {first_line}"
            );

            let body = "yaml_version: 1.00.00";
            let http_resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            tokio::io::AsyncWriteExt::write_all(&mut sock, http_resp.as_bytes())
                .await
                .unwrap();
        });

        let result = fetch_from_cloud(&base_url, 0x0007).await;
        assert!(result.is_ok(), "expected success, got {:?}", result.err());
        assert!(result.unwrap().contains("yaml_version"));
    }

    #[tokio::test]
    async fn cloud_404_is_err() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");

        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = tokio::io::AsyncReadExt::read(&mut sock, &mut buf).await;

            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found";
            tokio::io::AsyncWriteExt::write_all(&mut sock, resp.as_bytes())
                .await
                .unwrap();
        });

        let result = fetch_from_cloud(&base, 0x0099).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("404") && msg.contains("0x0099"),
            "should mention status code and device: {msg}");
    }

    #[tokio::test]
    async fn cloud_connection_refused() {
        // port 1 should always refuse connections
        let r = fetch_from_cloud("http://127.0.0.1:1", 0x0001).await;
        assert!(r.is_err());
    }

    // SERVICE_DATA_ACQUISITION.graph_data MUST  populate chart_register_ids
    #[test]
    fn graph_data_populates_chart_ids() {
        let state = crate::state::new_shared_state("test-gw".into());
        let yaml = r#"
yaml_version: 1.00.00
services:
- id: SERVICE_DATA_ACQUISITION
  graph_data:
  - id: STATUS_ID_TEMP_CABINET
  - id: STATUS_ID_COMP_SPEED
  - id: STATUS_ID_COMP_POWER
"#;
        let descriptor: DeviceDescriptor = serde_yaml::from_str(yaml).unwrap();
        populate_state(&state, &descriptor);

        let s = state.read().unwrap();
        assert_eq!(s.chart_register_ids.len(), 3);
        assert!(s.chart_register_ids.contains("STATUS_ID_TEMP_CABINET"));
        assert!(s.chart_register_ids.contains("STATUS_ID_COMP_SPEED"));
        assert!(s.chart_register_ids.contains("STATUS_ID_COMP_POWER"));
        assert!(s.descriptor.is_some());
    }

    // set in_chart flags on registers
    #[test]
    fn in_chart_fallback() {
        let state = crate::state::new_shared_state("test-gw".into());
        let yaml = r#"
yaml_version: 1.00.00
characteristics:
  parameters: []
  status:
  - id: TEMP
    address: 100
    in_chart: true
  - id: INTERNAL
    address: 101
"#;
        let desc: DeviceDescriptor = serde_yaml::from_str(yaml).unwrap();
        populate_state(&state, &desc);

        let s = state.read().unwrap();
        assert_eq!(s.chart_register_ids.len(), 1, "only in_chart=true should appear");
        assert!(s.chart_register_ids.contains("TEMP"));
    }

    #[test]
    fn real_descriptor_smoke() {
        let yaml_path = std::path::Path::new("devices/V0x0007_1.03V2.yaml");
        if !yaml_path.exists() {
            return;
        }

        let raw = std::fs::read_to_string(yaml_path).unwrap();
        let dev_desc: DeviceDescriptor = serde_yaml::from_str(&raw).unwrap();

        let state = crate::state::new_shared_state("test-gw".into());
        populate_state(&state, &dev_desc);

        let s = state.read().unwrap();
        assert!(
            s.chart_register_ids.len() > 5,
            "real descriptor should have many graph registers, got {}",
            s.chart_register_ids.len()
        );
        assert!(s.chart_register_ids.contains("STATUS_ID_TEMP_CABINET"));
        assert!(s.chart_register_ids.contains("STATUS_ID_COMP_SPEED"));
        // cPU_USAGE is internal-only, should not be in the chart list
        assert!(!s.chart_register_ids.contains("STATUS_ID_CPU_USAGE"));
    }
}
