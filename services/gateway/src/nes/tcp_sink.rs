use crate::state::{LogLevel, SharedState};
use crate::telemetry::build_telemetry_json;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

// tCP sink was the original approach before we switched to MQTT_SOURCE.
// kept as it maybe stable in the future version of NES
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

pub async fn run_nes_tcp_sink(
    host: &str,
    port: u16,
    gw_id: &str,
    dev_id: &str,
    poll_interval: Duration,
    state: SharedState,
) {
    let a = format!("{host}:{port}");

    loop {
        let mut conn = {
            let mut bo = INITIAL_BACKOFF;
            loop {
                match TcpStream::connect(&a).await {
                    Ok(s) => break s,
                    Err(_e) => {
                        tokio::time::sleep(bo).await;
                        bo = (bo * 2).min(MAX_BACKOFF);
                    }
                }
            }
        };

        state.write().unwrap().push_log(LogLevel::Info, format!("NES: connected to {a}"));

        // pump telemetry until the connection breaks
        loop {
            tokio::time::sleep(poll_interval).await;

            let v = match state.read() {
                Ok(s) => s.register_values.clone(),
                Err(_) => continue, // lock poisoned, skip this tick
            };
            if v.is_empty() { continue; }

            let j = build_telemetry_json(gw_id, dev_id, &v);
            let buf = match serde_json::to_string(&j) {
                Ok(s) => format!("{s}\n"),
                Err(_e) => {
                    continue;
                }
            };

            if conn.write_all(buf.as_bytes()).await.is_err() {
                break; // outer loop will reconnect
            }
        }
    } // reconnect loop
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_descriptor::RegisterValue;
    use crate::state::new_shared_state;
    use std::collections::HashMap;
    use tokio::io::AsyncBufReadExt;
    use tokio::net::TcpListener;


    #[tokio::test]
    async fn sends_json_line_to_tcp_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let state = new_shared_state("gw-test".to_string());
        {
            let mut s = state.write().expect("lock");
            s.register_values.insert("TEMP".to_string(), RegisterValue::Float(25.5));
            s.register_values.insert("RPM".to_string(), RegisterValue::Unsigned(3000));
        }

        let ss = state.clone();
        let sink = tokio::spawn(async move {
            run_nes_tcp_sink("127.0.0.1", addr.port(), "gw-test", "dev-001",
                Duration::from_millis(50), ss).await;
        });

        let (stream, _) = listener.accept().await.expect("accept");
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("read line");

        let json: serde_json::Value = serde_json::from_str(&line).expect("valid JSON");
        assert_eq!(json["GATEWAY_ID"], "gw-test");
        assert_eq!(json["DEVICE_ID"], "dev-001");
        assert_eq!(json["TEMP"], 25.5);
        assert_eq!(json["RPM"], 3000);
        assert!(json["timestamp"].as_i64().is_some());

        sink.abort();
    }

    // verify the sink reconnects 
    #[tokio::test]
    async fn reconnects_after_server_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let state = new_shared_state("gw-test".to_string());
        {
            let mut s = state.write().expect("lock");
            s.register_values.insert("V".to_string(), RegisterValue::Float(1.0));
        }

        let ss = state.clone();
        let sink = tokio::spawn(async move {
            run_nes_tcp_sink("127.0.0.1", addr.port(), "gw-rc", "dev-rc",
                Duration::from_millis(50), ss).await;
        });

        // first connection, read one line then drop it
        {
            let (stream, _) = listener.accept().await.expect("first accept");
            let mut reader = tokio::io::BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("first line");
            assert!(!line.is_empty(), "first connection should produce data");
        } // stream dropped here, sink should detect disconnect

        // sink should reconnect within 5s
        let accept_result = tokio::time::timeout(
            Duration::from_secs(5), listener.accept(),
        ).await;
        assert!(accept_result.is_ok(), "sink should reconnect within 5s");

        let (stream, _) = accept_result.expect("checked").expect("accept");
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("second line");
        assert!(!line.is_empty(), "second connection should produce data");

        sink.abort();
    }

    // when register_values is empty the sink should NOT send anything.
    // found this bug because an empty JSON object "{}" confused the NES
    // cSV parser and caused the entire physical source to be marked as errored.
    #[tokio::test]
    async fn skips_empty_register_values() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");

        let state = new_shared_state("gw-empty".to_string());

        let ss = state.clone();
        let sink = tokio::spawn(async move {
            run_nes_tcp_sink("127.0.0.1", addr.port(), "gw-empty", "dev-x",
                Duration::from_millis(50), ss).await;
        });

        let (stream, _) = listener.accept().await.expect("accept");
        let mut reader = tokio::io::BufReader::new(stream);
        let mut line = String::new();

        let result = tokio::time::timeout(
            Duration::from_millis(200), reader.read_line(&mut line),
        ).await;
        assert!(result.is_err(), "sink should not send anything when registers are empty");

        {
            let mut s = state.write().expect("lock");
            s.register_values.insert("X".to_string(), RegisterValue::Unsigned(42));
        }

        let mut line2 = String::new();
        let result2 = tokio::time::timeout(
            Duration::from_secs(2), reader.read_line(&mut line2),
        ).await;
        assert!(result2.is_ok(), "sink should send data once registers are populated");

        let json: serde_json::Value = serde_json::from_str(&line2).expect("valid JSON");
        assert_eq!(json["X"], 42);

        sink.abort();
    }

    #[test]
    fn json_line_format_is_valid() {
        let mut vals = HashMap::new();
        vals.insert("TEMP".to_string(), RegisterValue::Float(22.0));

        let json = build_telemetry_json("gw-1", "dev-1", &vals);
        let line = format!("{}\n", serde_json::to_string(&json).expect("serialize"));

        assert!(line.ends_with('\n'));
        assert!(!line.ends_with("\n\n")); // only one newline
        let parsed: serde_json::Value = serde_json::from_str(line.trim()).expect("parse");
        assert_eq!(parsed["TEMP"], 22.0);
    }
}
