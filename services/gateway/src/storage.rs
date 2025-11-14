use crate::device_descriptor::RegisterValue;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// 90 days. We used to keep 30 but customers with seasonal HVAC cycles
// wanted to compare "same week last year", 90 is a compromise because
// the Pi's SD card is only 16GB and the DB can hit ~800MB on a gateway
// with 40+ registers polled at 1s intervals.
const RETENTION_SECS: i64 = 90 * 24 * 3600;

// floor for how many rows we'll accumulate before the next prune.
// without this the DELETE on open can take 10+ seconds on a Pi Zero
// after a long uptime, and the UI just shows a spinner. We now prune
// incrementally via prune_if_needed() instead.
const PRUNE_BATCH: i64 = 50_000;

#[derive(Debug, Clone)]
pub struct HistoryDb {
    conn: Arc<Mutex<Connection>>,
    rows_since_prune: Arc<std::sync::atomic::AtomicU32>,
}

impl HistoryDb {
    pub fn open(gateway_id: &str) -> Result<Self, rusqlite::Error> {
        let path = db_path(gateway_id);

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&path)?;

        // wAL is non-negotiable: the UI polls query_range() every 2s while
        // insert_poll() is still running from the Modbus loop. Without WAL the
        // uI thread blocks on the writer and the chart stutters visibly.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        // 3000ms busy timeout, saw deadlocks on the CM4 with the default.
        // could probably be lower now that we batch inserts but haven't tested.
        conn.execute_batch("PRAGMA busy_timeout=3000;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS register_history (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                register_id  TEXT    NOT NULL,
                value        REAL    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_rh_ts
                ON register_history(timestamp_ms);
            CREATE INDEX IF NOT EXISTS idx_rh_reg_ts
                ON register_history(register_id, timestamp_ms);",
        )?;

        // do one big prune on startup to catch up if we've been offline a while,
        // but only if the table already exists (first run = nothing to prune)
        let cutoff_ms = now_ms() - RETENTION_SECS * 1000;
        let pruned = conn.execute(
            "DELETE FROM register_history WHERE timestamp_ms < ?1",
            params![cutoff_ms],
        )?;
        if pruned > 0 {
            tracing::info!(pruned, "startup prune");
        }

        tracing::info!(
            path = %path.display(),
            "History database opened"
        );

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            rows_since_prune: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        })
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn insert_poll(&self, values: &HashMap<String, RegisterValue>) {
        let Ok(conn) = self.conn.lock() else { return };
        let ts = now_ms();

        let Ok(tx) = conn.unchecked_transaction() else {
            tracing::warn!("could not begin transaction");
            return;
        };

        let mut inserted = 0u32;
        for (register_id, value) in values {
            let numeric = match value {
                RegisterValue::Float(f) => Some(*f),
                RegisterValue::Unsigned(u) => Some(*u as f64),
                RegisterValue::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
                RegisterValue::Enum(_) | RegisterValue::Bitwise(_) => None,
            };

            if let Some(v) = numeric {
                if let Err(e) = tx.execute(
                    "INSERT INTO register_history (timestamp_ms, register_id, value) VALUES (?1, ?2, ?3)",
                    params![ts, register_id, v],
                ) {
                    // this fires when the SD card is full on field units, worth
                    // logging so the tech doesn't stare at an empty chart for 20 min
                    // wondering why there's no data. Ask me how I know.
                    tracing::warn!(register = %register_id, error = %e, "history insert failed");
                }
                inserted += 1;
            }
        }

        let _ = tx.commit();

        let total = self.rows_since_prune.fetch_add(inserted, std::sync::atomic::Ordering::Relaxed);
        #[allow(clippy::cast_possible_truncation, reason = "PRUNE_BATCH is a small constant that fits in u32")]
        if total + inserted > PRUNE_BATCH as u32 {
            drop(conn);
            self.prune_old_rows();
        }
    }

    fn prune_old_rows(&self) {
        let Ok(conn) = self.conn.lock() else { return };
        let cutoff = now_ms() - RETENTION_SECS * 1000;

        // dELETE with LIMIT isn't standard SQLite, so we use a subquery.
        // this keeps the prune under ~50ms even on slow SD cards.
        match conn.execute(
            "DELETE FROM register_history WHERE id IN (
                SELECT id FROM register_history WHERE timestamp_ms < ?1 LIMIT 10000
            )",
            params![cutoff],
        ) {
            Ok(n) if n > 0 => tracing::debug!(n, "incremental prune"),
            Ok(_) => {}
            Err(e) => tracing::warn!("prune failed: {e}"),
        }
        self.rows_since_prune.store(0, std::sync::atomic::Ordering::Relaxed);
    }

}

fn db_path(gateway_id: &str) -> PathBuf {
    let base = dirs_fallback();
    base.join("gateway").join(gateway_id).join("history.db")
}

fn dirs_fallback() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_or_else(
            |_| PathBuf::from("."),
            |h| PathBuf::from(h).join(".local").join("share"),
        )
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
