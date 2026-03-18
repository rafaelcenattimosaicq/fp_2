use crate::nes::coordinator_client;
use crate::state::{LogLevel, SharedState, TrackedQuery};
use std::time::Duration;


const POLL_INTERVAL: Duration = Duration::from_secs(10);


const OPTIMIZING_TIMEOUT_SECS: u64 = 90;

pub async fn run_query_monitor(coord_url: String, state: SharedState) {

    tokio::time::sleep(Duration::from_secs(30)).await;

    tracing::info!("Query monitor started");
    log(&state, LogLevel::Info, "Query monitor: watching for stuck queries");

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let queries = match coordinator_client::fetch_all_queries(&coord_url).await {
            Ok(q) => q,
            Err(e) => {
                tracing::debug!(error = %e, "Query monitor: failed to fetch query catalog");
                continue; // coordinator might be restarting, try again 
            }
        };

        let now = now_unix_secs();

        let prev = state.read().ok()
            .map(|s| s.tracked_queries.clone())
            .unwrap_or_default();

        let mut tracked: Vec<TrackedQuery> = Vec::new();

        for entry in &queries {
            let old = prev.iter().find(|t| t.query_id == entry.query_id);
            let first_seen = old.map_or(now, |o| o.first_seen_secs);
            let was_stopped = old.is_some_and(|o| o.auto_stopped);

            if let Some(o) = old {
                if o.status != entry.status {
                    let msg = format!("Query monitor: query {} transitioned {} -> {}",
                        entry.query_id, o.status, entry.status);
                    tracing::info!("{}", msg);
                    log(&state, LogLevel::Info, &msg);
                }
            } else {
                tracing::info!("Query monitor: new query {} (status: {})",
                    entry.query_id, entry.status);
                log(&state, LogLevel::Info,
                    format!("Query monitor: new query {} (status: {})", entry.query_id, entry.status));
            }

            let mut auto_stopped = was_stopped;

            // detect queries stuck in OPTIMIZING
            let stuck = entry.status == "OPTIMIZING"
                && old.is_none_or(|o| o.status == "OPTIMIZING" || o.status == "REGISTERED");
            if stuck && !was_stopped {
                let dur = now.saturating_sub(first_seen);
                if dur >= OPTIMIZING_TIMEOUT_SECS {
                    let msg = format!(
                        "Query monitor: query {} stuck in OPTIMIZING for {}s, auto-stopping",
                        entry.query_id, dur);
                    tracing::warn!("{}", msg);
                    log(&state, LogLevel::Warn, &msg);

                    match coordinator_client::stop_query(&coord_url, entry.query_id).await {
                        Ok(()) => log(&state, LogLevel::Info,
                            format!("Query monitor: query {} auto-stopped", entry.query_id)),
                        Err(e) => tracing::warn!(query_id = entry.query_id, error = %e,
                            "Query monitor: auto-stop failed"),
                    }
                    auto_stopped = true;
                }
            }

            // log FAILED transitions prominently so they show up in the UI
            if entry.status == "FAILED" && old.is_none_or(|o| o.status != "FAILED") {
                let msg = format!("Query monitor: query {} FAILED", entry.query_id);
                tracing::warn!("{}", msg);
                log(&state, LogLevel::Error, &msg);
            }

            tracked.push(TrackedQuery {
                query_id: entry.query_id,
                status: entry.status.clone(),
                first_seen_secs: first_seen,
                auto_stopped,
            });
        }

        // update state atomically so the UI sees a consistent snapshot
        state.write().unwrap().tracked_queries = tracked;
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
