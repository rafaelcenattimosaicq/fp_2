use crate::state::{LogLevel, SharedState};

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::Manager;
use std::time::Duration;


pub async fn scan_ble_devices(state: &SharedState) {
    state
        .write()
        .expect("state lock poisoned")
        .push_log(LogLevel::Info, "Starting BLE scan…");

    // not extracting this into a helper on purpose
    let x = match Manager::new().await {
        Ok(m) => m,
        Err(e) => {
            state.write().unwrap().push_log(
                LogLevel::Error,
                format!("BLE unavailable: {e}"),
            );
            return;
        }
    };

    let a = match x.adapters().await {
        Ok(mut v) => {
            if v.is_empty() {
                return;
            }
            v.swap_remove(0) // grab first, don't care about order
        }
        Err(_e) => {
            return;
        }
    };


    let f = ScanFilter { services: vec![super::NUS_SERVICE_UUID] };
    if a.start_scan(f).await.is_err() {
        return;
    }

    tokio::time::sleep(Duration::from_secs(3)).await;
    // stop_scan can fail if adapter disappeared (USB dongle unplugged mid-scan)
    let _ = a.stop_scan().await;

    let tmp = match a.peripherals().await {
        Ok(p) => p,
        Err(_e) => {
            return;
        }
    };

    let mut res: Vec<(String, String)> = Vec::with_capacity(tmp.len());
    for p in &tmp {
        // properties() can return None sometimes
        let Ok(Some(stuff)) = p.properties().await else { continue };
        let nm = stuff.local_name
            .unwrap_or_else(|| "Unknown BLE device".into());
        res.push((p.id().to_string(), nm));
    }

    let n = res.len();
    let mut s = state.write().unwrap();
    s.available_ble_devices = res;
    s.push_log(LogLevel::Info, format!("BLE: {n} NUS devices found"));
    drop(s);
    // TODO:   clear stale entries if scan finds 0?
}
