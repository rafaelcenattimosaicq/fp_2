use async_trait::async_trait;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

use super::{Device, Gateway, RegistryStore, StoreError};

pub struct SqliteRegistryStore {
    conn: Mutex<Connection>,
}

impl SqliteRegistryStore {
    pub fn new(p: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Connection::open(p)?;

        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        // 3s busy timeout -- without this we get SQLITE_BUSY on every other
        // heartbeat during bulk device onboarding through MQTT. found out the
        // hard way during the joinville pilot
        db.execute_batch("PRAGMA busy_timeout=3000;")?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS gateways (
                gateway_id          TEXT PRIMARY KEY,
                status              TEXT NOT NULL DEFAULT 'pending_approval',
                meta                TEXT NOT NULL DEFAULT '{}',
                created_at          TEXT NOT NULL,
                last_seen_at        TEXT NOT NULL,
                enrollment_token_hash TEXT,
                access_token_hash   TEXT
            );",
        )?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS devices (
                gateway_id   TEXT NOT NULL,
                device_id    TEXT NOT NULL,
                status       TEXT NOT NULL DEFAULT 'active',
                meta         TEXT NOT NULL DEFAULT '{}',
                created_at   TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                PRIMARY KEY (gateway_id, device_id)
            );",
        )?;

        Ok(Self { conn: Mutex::new(db) })
    }
}

fn gen_token(n: usize) -> Result<String, StoreError> {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b)
        .map_err(|e| StoreError::Internal(format!("rng failed: {e}")))?;
    Ok(hex::encode(&b))
}

fn hash_token(t: &str) -> String {
    let mut h = Sha256::new();
    h.update(t.as_bytes());
    format!("{:x}", h.finalize())
}

// didnt want to pull in the hex crate just for encoding
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn row_to_gw(r: &rusqlite::Row) -> rusqlite::Result<Gateway> {
    let s: String = r.get(2)?;
    let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
    Ok(Gateway {
        gateway_id: r.get(0)?,
        status: r.get(1)?,
        meta: v,
        created_at: r.get(3)?,
        last_seen_at: r.get(4)?,
    })
}

fn row_to_dev(r: &rusqlite::Row) -> rusqlite::Result<Device> {
    let s: String = r.get(3)?;
    let v: serde_json::Value =
        serde_json::from_str(&s).unwrap_or(serde_json::Value::Object(Default::default()));
    Ok(Device {
        gateway_id: r.get(0)?,
        device_id: r.get(1)?,
        status: r.get(2)?,
        meta: v,
        created_at: r.get(4)?,
        last_seen_at: r.get(5)?,
    })
}

// SQLITE_BUSY still sneaks through when busy_timeout expires. surface as
// Unavailable (503) so callers know to retry instead of getting a 500
fn map_db_err(x: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(ref err, _) = x {
        if err.code == rusqlite::ErrorCode::DatabaseBusy {
            return StoreError::Unavailable(format!("database busy: {x}"));
        }
    }
    StoreError::Internal(format!("database error: {x}"))
}

// helper to grab the lock, less typing
#[inline]
fn grab_conn(conn: &Mutex<Connection>) -> Result<std::sync::MutexGuard<'_, Connection>, StoreError> {
    conn.lock().map_err(|e| StoreError::Internal(e.to_string()))
}

#[async_trait]
impl RegistryStore for SqliteRegistryStore {
    async fn list_gateways(&self) -> Result<Vec<Gateway>, StoreError> {
        let db = grab_conn(&self.conn)?;
        let mut q = db
            .prepare("SELECT gateway_id, status, meta, created_at, last_seen_at FROM gateways ORDER BY created_at DESC")
            .map_err(map_db_err)?;
        let res = q
            .query_map([], row_to_gw)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(res)
    }

    async fn get_gateway(&self, gateway_id: &str) -> Result<Option<Gateway>, StoreError> {
        let c = grab_conn(&self.conn)?;
        let mut s = c
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        let thing = s
            .query_row(params![gateway_id], row_to_gw)
            .optional()
            .map_err(map_db_err)?;
        Ok(thing)
    }

    async fn list_devices(&self, gateway_id: &str) -> Result<Vec<Device>, StoreError> {
        let db = grab_conn(&self.conn)?;
        let mut tmp = db.prepare(
            "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at FROM devices WHERE gateway_id = ?1 ORDER BY created_at DESC"
        ).map_err(map_db_err)?;
        let stuff = tmp
            .query_map(params![gateway_id], row_to_dev)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(stuff)
    }

    async fn get_device(
        &self,
        gateway_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, StoreError> {
        let c = grab_conn(&self.conn)?;
        let mut q = c
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        let r = q
            .query_row(params![gateway_id, device_id], row_to_dev)
            .optional()
            .map_err(map_db_err)?;
        Ok(r)
    }

    async fn upsert_device_seen(
        &self,
        gateway_id: &str,
        device_id: &str,
        meta: serde_json::Value,
    ) -> Result<Device, StoreError> {
        let db = grab_conn(&self.conn)?;
        let ts = chrono::Utc::now().to_rfc3339();
        let j = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".to_string());

        db.execute(
            "INSERT INTO devices (gateway_id, device_id, status, meta, created_at, last_seen_at)
             VALUES (?1, ?2, 'active', ?3, ?4, ?4)
             ON CONFLICT(gateway_id, device_id) DO UPDATE SET
                 last_seen_at = ?4,
                 meta = ?3",
            params![gateway_id, device_id, j, ts],
        )
        .map_err(map_db_err)?;

        // re-read so we return the full row
        let mut q = db
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        q.query_row(params![gateway_id, device_id], row_to_dev)
            .map_err(map_db_err)
    }

    async fn upsert_gateway_seen(
        &self,
        gateway_id: &str,
        meta: serde_json::Value,
    ) -> Result<Gateway, StoreError> {
        let c = grab_conn(&self.conn)?;
        let t = chrono::Utc::now().to_rfc3339();
        let val = serde_json::to_string(&meta)
            .map_err(|e| StoreError::Internal(format!("json serialize error: {e}")))?;

        c.execute(
            "INSERT INTO gateways (gateway_id, status, meta, created_at, last_seen_at)
             VALUES (?1, 'approved', ?2, ?3, ?3)
             ON CONFLICT(gateway_id) DO UPDATE SET
                 last_seen_at = ?3,
                 meta = ?2",
            params![gateway_id, val, t],
        )
        .map_err(map_db_err)?;

        let mut s = c
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        s.query_row(params![gateway_id], row_to_gw)
            .map_err(map_db_err)
    }

    async fn request_gateway_onboarding(
        &self,
        gateway_id: &str,
        meta: serde_json::Value,
    ) -> Result<(Gateway, String), StoreError> {
        let db = grab_conn(&self.conn)?;

        // check if already exists
        let chk: Option<String> = db
            .query_row(
                "SELECT status FROM gateways WHERE gateway_id = ?1",
                params![gateway_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_err)?;
        if chk.is_some() {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' already exists"
            )));
        }

        let ts = chrono::Utc::now().to_rfc3339();
        let j = serde_json::to_string(&meta).expect("shouldnt fail");

        let tok = gen_token(32)?;
        let th = hash_token(&tok);

        db.execute(
            "INSERT INTO gateways (gateway_id, status, meta, created_at, last_seen_at, enrollment_token_hash)
             VALUES (?1, 'pending_approval', ?2, ?3, ?3, ?4)",
            params![gateway_id, j, ts, th],
        )
        .map_err(map_db_err)?;

        let mut q = db
            .prepare("SELECT gateway_id, status, meta, created_at, last_seen_at FROM gateways WHERE gateway_id = ?1")
            .map_err(map_db_err)?;
        let ret = q
            .query_row(params![gateway_id], row_to_gw)
            .map_err(map_db_err)?;

        Ok((ret, tok))
    }

    async fn approve_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError> {
        let c = grab_conn(&self.conn)?;

        let n = c.execute(
            "UPDATE gateways SET status = 'approved' WHERE gateway_id = ?1 AND status = 'pending_approval'",
            params![gateway_id],
        ).map_err(map_db_err)?;

        if n == 0 {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' not found or not in pending_approval status"
            )));
        }

        let mut q = c
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        q.query_row(params![gateway_id], row_to_gw)
            .map_err(map_db_err)
    }

    async fn claim_gateway_access_token(
        &self,
        gateway_id: &str,
        enrollment_token: &str,
    ) -> Result<String, StoreError> {
        let db = grab_conn(&self.conn)?;

        let r: Option<(String, Option<String>)> = db
            .query_row(
                "SELECT status, enrollment_token_hash FROM gateways WHERE gateway_id = ?1",
                params![gateway_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(map_db_err)?;

        let (st, h) = match r {
            Some(v) => v,
            None => return Err(StoreError::BadRequest(format!("gateway '{gateway_id}' not found"))),
        };

        if st != "approved" {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' is not approved (status: {st})"
            )));
        }

        let expected = h.ok_or_else(|| {
            StoreError::BadRequest("no enrollment token set for this gateway".to_string())
        })?;
        if hash_token(enrollment_token) != expected {
            return Err(StoreError::BadRequest("invalid enrollment token".to_string()));
        }

        let tok = gen_token(32)?;
        let th = hash_token(&tok);

        db.execute(
            "UPDATE gateways SET access_token_hash = ?1, enrollment_token_hash = NULL
             WHERE gateway_id = ?2",
            params![th, gateway_id],
        )
        .map_err(map_db_err)?;

        Ok(tok)
    }

    async fn verify_gateway_access_token(
        &self,
        gateway_id: &str,
        token: &str,
    ) -> Result<bool, StoreError> {
        let c = grab_conn(&self.conn)?;

        let x: Option<Option<String>> = c
            .query_row(
                "SELECT access_token_hash FROM gateways WHERE gateway_id = ?1 AND status = 'approved'",
                params![gateway_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_err)?;

        // this nested Option is ugly but it works dont ask me why
        let Some(Some(v)) = x else { return Ok(false); };

        Ok(hash_token(token) == v)
    }

    async fn rotate_gateway_access_token(
        &self,
        gateway_id: &str,
        old_token: &str,
    ) -> Result<String, StoreError> {
        let ok = self.verify_gateway_access_token(gateway_id, old_token).await?;
        if !ok {
            return Err(StoreError::BadRequest(
                "invalid current access token".to_string(),
            ));
        }

        let db = grab_conn(&self.conn)?;
        let t = gen_token(32)?;
        let h = hash_token(&t);

        db.execute(
            "UPDATE gateways SET access_token_hash = ?1 WHERE gateway_id = ?2",
            params![h, gateway_id],
        ).map_err(map_db_err)?;

        Ok(t)
    }

    async fn revoke_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError> {
        let c = grab_conn(&self.conn)?;

        let n = c
            .execute(
                "UPDATE gateways SET status = 'revoked', access_token_hash = NULL
                 WHERE gateway_id = ?1",
                params![gateway_id],
            )
            .map_err(map_db_err)?;

        if n == 0 {
            return Err(StoreError::BadRequest(format!("gateway '{gateway_id}' not found")));
        }

        let mut tmp = c
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        tmp.query_row(params![gateway_id], row_to_gw)
            .map_err(map_db_err)
    }

    async fn decommission_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError> {
        let db = grab_conn(&self.conn)?;

        let x = db
            .execute(
                "UPDATE gateways SET status = 'decommissioned', access_token_hash = NULL, enrollment_token_hash = NULL
                 WHERE gateway_id = ?1",
                params![gateway_id],
            )
            .map_err(map_db_err)?;

        if x == 0 {
            return Err(StoreError::BadRequest(format!("gateway '{gateway_id}' not found")));
        }

        // TODO: should we also decommission all devices under this gateway?
        let mut s = db
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        s.query_row(params![gateway_id], row_to_gw)
            .map_err(map_db_err)
    }

    async fn register_device(
        &self,
        gateway_id: &str,
        device_id: &str,
        meta: serde_json::Value,
    ) -> Result<Device, StoreError> {
        let c = grab_conn(&self.conn)?;
        let ts = chrono::Utc::now().to_rfc3339();
        let j = serde_json::to_string(&meta)
            .map_err(|e| StoreError::Internal(format!("json serialize error: {e}")))?;

        let res = c.execute(
            "INSERT INTO devices (gateway_id, device_id, status, meta, created_at, last_seen_at)
             VALUES (?1, ?2, 'active', ?3, ?4, ?4)",
            params![gateway_id, device_id, j, ts],
        );

        match res {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(StoreError::BadRequest(format!(
                    "device '{device_id}' already exists under gateway '{gateway_id}'"
                )));
            }
            Err(e) => return Err(map_db_err(e)),
        }

        let mut q = c
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        q.query_row(params![gateway_id, device_id], row_to_dev)
            .map_err(map_db_err)
    }

    async fn revoke_device(
        &self, gateway_id: &str, device_id: &str,
    ) -> Result<Device, StoreError> {
        let db = grab_conn(&self.conn)?;

        let n = db
            .execute(
                "UPDATE devices SET status = 'revoked' WHERE gateway_id = ?1 AND device_id = ?2",
                params![gateway_id, device_id],
            )
            .map_err(map_db_err)?;

        if n == 0 {
            return Err(StoreError::BadRequest(format!(
                "device '{device_id}' not found under gateway '{gateway_id}'"
            )));
        }

        let mut item = db
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        item.query_row(params![gateway_id, device_id], row_to_dev)
            .map_err(map_db_err)
    }

    async fn decommission_device(
        &self,
        gateway_id: &str,
        device_id: &str,
    ) -> Result<Device, StoreError> {
        let c = grab_conn(&self.conn)?;

        let cnt = c.execute(
            "UPDATE devices SET status = 'decommissioned' WHERE gateway_id = ?1 AND device_id = ?2",
            params![gateway_id, device_id],
        ).map_err(map_db_err)?;

        if cnt == 0 {
            return Err(StoreError::BadRequest(format!(
                "device '{device_id}' not found under gateway '{gateway_id}'"
            )));
        }

        let mut tmp = c
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        tmp.query_row(params![gateway_id, device_id], row_to_dev)
            .map_err(map_db_err)
    }
}

trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn new_test_store() -> SqliteRegistryStore {
        SqliteRegistryStore::new(":memory:")
            .expect("in-memory SQLite store should initialize without error")
    }

    #[test]
    fn hex_encode_produces_correct_output() {
        let bytes = [0x00, 0x0f, 0xff, 0xab];
        let encoded = hex::encode(&bytes);
        assert_eq!(encoded, "000fffab");
    }

    #[test]
    fn hex_encode_empty_input() {
        let encoded = hex::encode(&[]);
        assert_eq!(encoded, "");
    }

    #[test]
    fn hash_token_is_deterministic() {
        let token = "my-secret-token";
        let h1 = hash_token(token);
        let h2 = hash_token(token);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_token_different_inputs_differ() {
        let h1 = hash_token("token-a");
        let h2 = hash_token("token-b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn generate_token_has_correct_length() {
        let token = gen_token(32).expect("token generation should succeed");
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_token_produces_unique_values() {
        let t1 = gen_token(32).expect("token generation should succeed");
        let t2 = gen_token(32).expect("token generation should succeed");
        assert_ne!(t1, t2);
    }

    #[test]
    fn optional_ext_ok_returns_some() {
        let result: rusqlite::Result<i32> = Ok(42);
        let opt = result.optional();
        assert_eq!(opt.unwrap(), Some(42));
    }

    #[test]
    fn optional_ext_no_rows_returns_none() {
        let result: rusqlite::Result<i32> = Err(rusqlite::Error::QueryReturnedNoRows);
        let opt = result.optional();
        assert_eq!(opt.unwrap(), None);
    }

    #[test]
    fn optional_ext_other_error_propagates() {
        let result: rusqlite::Result<i32> =
            Err(rusqlite::Error::InvalidParameterName("bad".to_string()));
        let opt = result.optional();
        assert!(opt.is_err());
    }

    #[test]
    fn new_store_with_in_memory_db_succeeds() {
        let _store = new_test_store();
    }

    #[tokio::test]
    async fn list_gateways_empty_on_fresh_db() {
        let store = new_test_store();
        let gateways = store.list_gateways().await.expect("list should succeed");
        assert!(gateways.is_empty());
    }

    #[tokio::test]
    async fn upsert_gateway_seen_creates_new_gateway() {
        let store = new_test_store();
        let meta = json!({"ip": "192.168.1.10"});
        let gw = store
            .upsert_gateway_seen("gw-new", meta.clone())
            .await
            .expect("upsert should succeed");
        assert_eq!(gw.gateway_id, "gw-new");
        assert_eq!(gw.status, "approved");
        assert_eq!(gw.meta, meta);
    }

    #[tokio::test]
    async fn upsert_gateway_seen_updates_existing_gateway() {
        let store = new_test_store();
        let gw1 = store
            .upsert_gateway_seen("gw-1", json!({"v": 1}))
            .await
            .expect("first upsert should succeed");
        let gw2 = store
            .upsert_gateway_seen("gw-1", json!({"v": 2}))
            .await
            .expect("second upsert should succeed");
        assert_eq!(gw2.gateway_id, "gw-1");
        assert_eq!(gw2.meta, json!({"v": 2}));
        assert_eq!(gw2.created_at, gw1.created_at);
        assert!(gw2.last_seen_at >= gw1.last_seen_at);
        let all = store.list_gateways().await.expect("list should succeed");
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn get_gateway_returns_existing() {
        let store = new_test_store();
        store
            .upsert_gateway_seen("gw-x", json!({}))
            .await
            .expect("upsert should succeed");
        let result = store
            .get_gateway("gw-x")
            .await
            .expect("get should succeed");
        assert!(result.is_some());
        assert_eq!(result.unwrap().gateway_id, "gw-x");
    }

    #[tokio::test]
    async fn get_gateway_returns_none_for_missing() {
        let store = new_test_store();
        let result = store
            .get_gateway("nonexistent")
            .await
            .expect("get should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_gateways_returns_all() {
        let store = new_test_store();
        store.upsert_gateway_seen("gw-a", json!({})).await.unwrap();
        store.upsert_gateway_seen("gw-b", json!({})).await.unwrap();
        store.upsert_gateway_seen("gw-c", json!({})).await.unwrap();
        let gateways = store.list_gateways().await.expect("list should succeed");
        assert_eq!(gateways.len(), 3);
    }

    #[tokio::test]
    async fn upsert_device_seen_creates_new_device() {
        let store = new_test_store();
        let meta = json!({"model": "XYZ"});
        let dev = store
            .upsert_device_seen("gw-1", "dev-1", meta.clone())
            .await
            .expect("upsert should succeed");
        assert_eq!(dev.gateway_id, "gw-1");
        assert_eq!(dev.device_id, "dev-1");
        assert_eq!(dev.status, "active");
        assert_eq!(dev.meta, meta);
    }

    #[tokio::test]
    async fn upsert_device_seen_updates_existing_device() {
        let store = new_test_store();
        let dev1 = store
            .upsert_device_seen("gw-1", "dev-1", json!({"v": 1}))
            .await
            .expect("first upsert should succeed");
        let dev2 = store
            .upsert_device_seen("gw-1", "dev-1", json!({"v": 2}))
            .await
            .expect("second upsert should succeed");
        assert_eq!(dev2.meta, json!({"v": 2}));
        assert_eq!(dev2.created_at, dev1.created_at);
        assert!(dev2.last_seen_at >= dev1.last_seen_at);
    }

    #[tokio::test]
    async fn list_devices_filters_by_gateway() {
        let store = new_test_store();
        store
            .upsert_device_seen("gw-a", "dev-1", json!({}))
            .await
            .unwrap();
        store
            .upsert_device_seen("gw-a", "dev-2", json!({}))
            .await
            .unwrap();
        store
            .upsert_device_seen("gw-b", "dev-3", json!({}))
            .await
            .unwrap();
        let devices_a = store
            .list_devices("gw-a")
            .await
            .expect("list should succeed");
        let devices_b = store
            .list_devices("gw-b")
            .await
            .expect("list should succeed");
        assert_eq!(devices_a.len(), 2);
        assert_eq!(devices_b.len(), 1);
        assert_eq!(devices_b[0].device_id, "dev-3");
    }

    #[tokio::test]
    async fn list_devices_empty_for_unknown_gateway() {
        let store = new_test_store();
        let devices = store
            .list_devices("gw-unknown")
            .await
            .expect("list should succeed");
        assert!(devices.is_empty());
    }

    #[tokio::test]
    async fn get_device_returns_existing() {
        let store = new_test_store();
        store
            .upsert_device_seen("gw-1", "dev-1", json!({"key": "val"}))
            .await
            .unwrap();
        let result = store
            .get_device("gw-1", "dev-1")
            .await
            .expect("get should succeed");
        assert!(result.is_some());
        let dev = result.unwrap();
        assert_eq!(dev.device_id, "dev-1");
        assert_eq!(dev.meta["key"], "val");
    }

    #[tokio::test]
    async fn get_device_returns_none_for_missing() {
        let store = new_test_store();
        let result = store
            .get_device("gw-1", "no-such-device")
            .await
            .expect("get should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn gateway_onboarding_full_lifecycle() {
        let store = new_test_store();

        let (gw, enrollment_token) = store
            .request_gateway_onboarding("gw-onboard", json!({"location": "factory-1"}))
            .await
            .expect("onboarding request should succeed");
        assert_eq!(gw.status, "pending_approval");
        assert_eq!(gw.gateway_id, "gw-onboard");
        assert!(!enrollment_token.is_empty());

        let gw = store
            .approve_gateway("gw-onboard")
            .await
            .expect("approval should succeed");
        assert_eq!(gw.status, "approved");

        let access_token = store
            .claim_gateway_access_token("gw-onboard", &enrollment_token)
            .await
            .expect("claim should succeed");
        assert_eq!(access_token.len(), 64);
        assert!(access_token.chars().all(|c| c.is_ascii_hexdigit()));

        let valid = store
            .verify_gateway_access_token("gw-onboard", &access_token)
            .await
            .expect("verify should succeed");
        assert!(valid, "newly claimed access token should be valid");
    }

    #[tokio::test]
    async fn request_onboarding_rejects_duplicate_gateway() {
        let store = new_test_store();
        store
            .request_gateway_onboarding("gw-dup", json!({}))
            .await
            .expect("first request should succeed");
        let result = store
            .request_gateway_onboarding("gw-dup", json!({}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, StoreError::BadRequest(_)),
            "expected BadRequest, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn approve_gateway_rejects_invalid_state() {
        let store = new_test_store();

        let result = store.approve_gateway("gw-missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));

        store
            .upsert_gateway_seen("gw-already-approved", json!({}))
            .await
            .unwrap();
        let result = store.approve_gateway("gw-already-approved").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn claim_token_rejects_wrong_enrollment_token() {
        let store = new_test_store();
        store
            .request_gateway_onboarding("gw-wrong", json!({}))
            .await
            .unwrap();
        store.approve_gateway("gw-wrong").await.unwrap();
        let result = store
            .claim_gateway_access_token("gw-wrong", "bogus-token")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn claim_token_rejects_unapproved_gateway() {
        let store = new_test_store();
        let (_gw, enrollment_token) = store
            .request_gateway_onboarding("gw-pending", json!({}))
            .await
            .unwrap();
        let result = store
            .claim_gateway_access_token("gw-pending", &enrollment_token)
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn verify_access_token_rejects_wrong_token() {
        let store = new_test_store();
        let (_gw, enrollment) = store
            .request_gateway_onboarding("gw-tok", json!({}))
            .await
            .unwrap();
        store.approve_gateway("gw-tok").await.unwrap();
        store
            .claim_gateway_access_token("gw-tok", &enrollment)
            .await
            .unwrap();
        let valid = store
            .verify_gateway_access_token("gw-tok", "definitely-wrong-token")
            .await
            .expect("verify should not error");
        assert!(!valid);
    }

    #[tokio::test]
    async fn verify_access_token_returns_false_for_unknown_gateway() {
        let store = new_test_store();
        let valid = store
            .verify_gateway_access_token("gw-nope", "any-token")
            .await
            .expect("verify should not error");
        assert!(!valid);
    }

    #[tokio::test]
    async fn rotate_access_token_invalidates_old_token() {
        let store = new_test_store();
        let (_gw, enrollment) = store
            .request_gateway_onboarding("gw-rot", json!({}))
            .await
            .unwrap();
        store.approve_gateway("gw-rot").await.unwrap();
        let old_token = store
            .claim_gateway_access_token("gw-rot", &enrollment)
            .await
            .unwrap();
        let new_token = store
            .rotate_gateway_access_token("gw-rot", &old_token)
            .await
            .expect("rotation should succeed");
        assert_ne!(old_token, new_token);
        let old_valid = store
            .verify_gateway_access_token("gw-rot", &old_token)
            .await
            .unwrap();
        let new_valid = store
            .verify_gateway_access_token("gw-rot", &new_token)
            .await
            .unwrap();
        assert!(!old_valid, "old token should be invalidated after rotation");
        assert!(new_valid, "new token should be valid after rotation");
    }

    #[tokio::test]
    async fn rotate_access_token_rejects_invalid_old_token() {
        let store = new_test_store();
        let (_gw, enrollment) = store
            .request_gateway_onboarding("gw-rot2", json!({}))
            .await
            .unwrap();
        store.approve_gateway("gw-rot2").await.unwrap();
        store
            .claim_gateway_access_token("gw-rot2", &enrollment)
            .await
            .unwrap();
        let result = store
            .rotate_gateway_access_token("gw-rot2", "wrong-old-token")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn revoke_gateway_sets_status_and_clears_token() {
        let store = new_test_store();
        let (_gw, enrollment) = store
            .request_gateway_onboarding("gw-rev", json!({}))
            .await
            .unwrap();
        store.approve_gateway("gw-rev").await.unwrap();
        let token = store
            .claim_gateway_access_token("gw-rev", &enrollment)
            .await
            .unwrap();
        let gw = store
            .revoke_gateway("gw-rev")
            .await
            .expect("revoke should succeed");
        assert_eq!(gw.status, "revoked");
        let valid = store
            .verify_gateway_access_token("gw-rev", &token)
            .await
            .unwrap();
        assert!(!valid, "access token should be invalid after revocation");
    }

    #[tokio::test]
    async fn decommission_gateway_sets_status() {
        let store = new_test_store();
        store.upsert_gateway_seen("gw-dec", json!({})).await.unwrap();
        let gw = store
            .decommission_gateway("gw-dec")
            .await
            .expect("decommission should succeed");
        assert_eq!(gw.status, "decommissioned");
    }

    #[tokio::test]
    async fn revoke_gateway_missing_fails() {
        let store = new_test_store();
        let result = store.revoke_gateway("gw-missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn decommission_gateway_missing_fails() {
        let store = new_test_store();
        let result = store.decommission_gateway("gw-missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn register_device_creates_active_device() {
        let store = new_test_store();
        let meta = json!({"serial": "SN-123"});
        let dev = store
            .register_device("gw-1", "dev-reg", meta.clone())
            .await
            .expect("register should succeed");
        assert_eq!(dev.gateway_id, "gw-1");
        assert_eq!(dev.device_id, "dev-reg");
        assert_eq!(dev.status, "active");
        assert_eq!(dev.meta, meta);
    }

    #[tokio::test]
    async fn register_device_rejects_duplicate() {
        let store = new_test_store();
        store
            .register_device("gw-1", "dev-dup", json!({}))
            .await
            .unwrap();
        let result = store
            .register_device("gw-1", "dev-dup", json!({}))
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn revoke_device_sets_status() {
        let store = new_test_store();
        store
            .register_device("gw-1", "dev-rev", json!({}))
            .await
            .unwrap();
        let dev = store
            .revoke_device("gw-1", "dev-rev")
            .await
            .expect("revoke should succeed");
        assert_eq!(dev.status, "revoked");
    }

    #[tokio::test]
    async fn decommission_device_sets_status() {
        let store = new_test_store();
        store
            .register_device("gw-1", "dev-decom", json!({}))
            .await
            .unwrap();
        let dev = store
            .decommission_device("gw-1", "dev-decom")
            .await
            .expect("decommission should succeed");
        assert_eq!(dev.status, "decommissioned");
    }

    #[tokio::test]
    async fn revoke_device_missing_fails() {
        let store = new_test_store();
        let result = store.revoke_device("gw-1", "dev-missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[tokio::test]
    async fn decommission_device_missing_fails() {
        let store = new_test_store();
        let result = store.decommission_device("gw-1", "dev-missing").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StoreError::BadRequest(_)));
    }

    #[test]
    fn map_db_err_wraps_rusqlite_error() {
        let db_err = rusqlite::Error::QueryReturnedNoRows;
        let store_err = map_db_err(db_err);
        assert!(
            matches!(store_err, StoreError::Internal(_)),
            "expected Internal, got: {store_err:?}"
        );
    }
}
