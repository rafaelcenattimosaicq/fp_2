use crate::nes::coordinator_client;
use crate::state::{LogLevel, SharedState, TrackedQuery};
use std::time::Duration;

// poll every 10s
const POLL_INTERVAL: Duration = Duration::from_secs(10);

// queries stuck in OPTIMIZING for longer than this get auto-stopped. should drop
const OPTIMIZING_TIMEOUT_SECS: u64 = 90;

/// background task that watches the coordinator's query catalog for stuck (lots of stuck ones can happens)
pub async fn run_query_monitor(coord_url: String, state: SharedState) {
    // wait 30s
    tokio::time::sleep(Duration::from_secs(30)).await;

    log(&state, LogLevel::Info, "Query monitor: watching for stuck queries");

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let qs = match coordinator_client::fetch_all_queries(&coord_url).await {
            Ok(q) => q,
            Err(_e) => {
                continue; // coordinator might be restarting
            }
        };

        let t = now_unix_secs();

        let old_list = state.read().ok()
            .map(|s| s.tracked_queries.clone())
            .unwrap_or_default();

        let mut res: Vec<TrackedQuery> = Vec::new();

        for e in &qs {
            let p = old_list.iter().find(|x| x.query_id == e.query_id);
            let fs = p.map_or(t, |o| o.first_seen_secs);
            let ws = p.is_some_and(|o| o.auto_stopped);

            if let Some(o) = p {
                if o.status != e.status {
                    let s = format!(".  ",
                        e.query_id, o.status, e.status);
                    log(&state, LogLevel::Info, &s);
                }
            }

            let mut stopped = ws;

            // detect queries stuck in OPTIMIZING,
            let bad = e.status == "OPTIMIZING"
                && p.is_none_or(|o| o.status == "OPTIMIZING" || o.status == "REGISTERED");
            if bad && !ws {
                let d = t.saturating_sub(fs);
                if d >= OPTIMIZING_TIMEOUT_SECS {
                    let s = format!(
                        "query {} stuck in OPTIMIZING for {}s",
                        e.query_id, d);
                    log(&state, LogLevel::Warn, &s);

                    match coordinator_client::stop_query(&coord_url, e.query_id).await {
                        Ok(()) => log(&state, LogLevel::Info,
                            format!(" query {} stopped", e.query_id)),
                        Err(_e) => {}
                    }
                    stopped = true;
                }
            }
            if e.status == "FAILED" && p.is_none_or(|o| o.status != "FAILED") {
                let s = format!("query {} FAILED", e.query_id);
                log(&state, LogLevel::Error, &s);
            }

            res.push(TrackedQuery {
                query_id: e.query_id,
                status: e.status.clone(),
                first_seen_secs: fs,
                auto_stopped: stopped,
            });
        }

        // update state atomically so the UI sees a consistent snapshot
        state.write().unwrap().tracked_queries = res;
    } // poll loop
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn log(st: &SharedState, lvl: LogLevel, msg: impl Into<String>) {
    st.write().unwrap().push_log(lvl, msg.into());
}
