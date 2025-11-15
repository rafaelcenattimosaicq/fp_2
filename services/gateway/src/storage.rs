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

    pub fn query_range(
        &self,
        start_ms: i64,
        end_ms: i64,
        register_ids: &[String],
    ) -> Vec<(i64, String, f64)> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };

        if register_ids.is_empty() {
            let Ok(mut stmt) = conn.prepare(
                "SELECT timestamp_ms, register_id, value FROM register_history
                 WHERE timestamp_ms BETWEEN ?1 AND ?2
                 ORDER BY timestamp_ms ASC",
            ) else {
                return Vec::new();
            };

            stmt.query_map(params![start_ms, end_ms], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_or_else(|_| Vec::new(), |rows| rows.filter_map(Result::ok).collect())
        } else {
            // can't use a static prepared statement here because the number of
            // register_ids varies. The placeholders are positional (?3, ?4, ...)
            // which is ugly but avoids any SQL injection since they're all bound.
            let placeholders: Vec<String> = register_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 3))
                .collect();
            let sql = format!(
                "SELECT timestamp_ms, register_id, value FROM register_history
                 WHERE timestamp_ms BETWEEN ?1 AND ?2
                 AND register_id IN ({})
                 ORDER BY timestamp_ms ASC",
                placeholders.join(", ")
            );

            let Ok(mut stmt) = conn.prepare(&sql) else {
                return Vec::new();
            };

            let params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = {
                let mut v: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                v.push(Box::new(start_ms));
                v.push(Box::new(end_ms));
                for id in register_ids {
                    v.push(Box::new(id.clone()));
                }
                v
            };

            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params_vec.iter().map(AsRef::as_ref).collect();

            stmt.query_map(param_refs.as_slice(), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_or_else(|_| Vec::new(), |rows| rows.filter_map(Result::ok).collect())
        }
    }

    pub fn available_registers(&self) -> Vec<String> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };

        let Ok(mut stmt) = conn.prepare(
            "SELECT DISTINCT register_id FROM register_history ORDER BY register_id",
        ) else {
            return Vec::new();
        };

        stmt.query_map([], |row| row.get(0))
            .map_or_else(|_| Vec::new(), |rows| rows.filter_map(Result::ok).collect())
    }

    /// rough row count for the status page. Not exact, we don't want to
    /// pay for a full COUNT(*) on every UI refresh.
    #[allow(dead_code, reason = "reserved for the status page UI that shows disk usage")]
    pub fn approx_row_count(&self) -> Option<i64> {
        let conn = self.conn.lock().ok()?;
        // this reads from sqlite_stat1 which is only populated after ANALYZE.
        // falls back to max(rowid) which over-counts if rows were deleted but
        // is close enough for a "disk usage" indicator in the UI.
        conn.query_row(
            "SELECT MAX(id) FROM register_history", [], |r| r.get(0),
        ).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_db() -> (tempfile::TempDir, HistoryDb) {
        let tmp = tempfile::tempdir().unwrap();
        let db_file = tmp.path().join("test.db");
        let conn = Connection::open(&db_file).unwrap();
        conn.execute_batch(
            "CREATE TABLE register_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_ms INTEGER NOT NULL,
                register_id TEXT NOT NULL,
                value REAL NOT NULL
            );",
        )
        .unwrap();

        let db = HistoryDb {
            conn: Arc::new(Mutex::new(conn)),
            rows_since_prune: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        };
        (tmp, db)
    }

    #[test]
    fn insert_and_query_back() {
        let (_tmp, db) = tmp_db();

        let mut values = HashMap::new();
        values.insert("temperature".to_string(), RegisterValue::Float(25.5));
        values.insert("rpm".to_string(), RegisterValue::Unsigned(1200));
        values.insert(
            "mode".to_string(),
            RegisterValue::Enum("cooling".to_string()),
        );

        db.insert_poll(&values);

        let rows = db.query_range(0, i64::MAX, &[]);
        assert_eq!(rows.len(), 2, "should store Float and Unsigned but not Enum");

        let rows = db.query_range(0, i64::MAX, &["temperature".to_string()]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "temperature");
        assert!((rows[0].2 - 25.5).abs() < f64::EPSILON);
    }

    #[test]
    fn available_registers_sorted() {
        let (_tmp, db) = tmp_db();

        let mut v1 = HashMap::new();
        v1.insert("temp".to_string(), RegisterValue::Float(20.0));
        v1.insert("rpm".to_string(), RegisterValue::Unsigned(800));
        db.insert_poll(&v1);
        db.insert_poll(&v1);

        let regs = db.available_registers();
        assert_eq!(regs, vec!["rpm", "temp"]);
    }

    // regression: we had a bug where query_range with an empty register_ids
    // slice would return nothing because the IN clause was empty. Now it
    // takes a different code path (no IN clause at all).
    #[test]
    fn query_range_empty_filter_returns_all() {
        let (_tmp, db) = tmp_db();

        let mut v = HashMap::new();
        v.insert("a".to_string(), RegisterValue::Float(1.0));
        v.insert("b".to_string(), RegisterValue::Float(2.0));
        db.insert_poll(&v);

        let all = db.query_range(0, i64::MAX, &[]);
        assert_eq!(all.len(), 2);

        let just_a = db.query_range(0, i64::MAX, &["a".to_string()]);
        assert_eq!(just_a.len(), 1);
    }
}