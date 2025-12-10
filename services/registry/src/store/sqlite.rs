use async_trait::async_trait;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

use super::{Device, Gateway, RegistryStore, StoreError};

pub struct SqliteRegistryStore {
    conn: Mutex<Connection>,
}

impl SqliteRegistryStore {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        // 3 seconds. MQTT ingestion hammers the DB with concurrent upserts and
        // without this we get SQLITE_BUSY on every other heartbeat during bulk
        // device onboarding. Ask me how I know.
        conn.execute_batch("PRAGMA busy_timeout=3000;")?;

        conn.execute_batch(
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

        conn.execute_batch(
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

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

fn generate_token(byte_len: usize) -> Result<String, StoreError> {
    let mut buf = vec![0u8; byte_len];
    getrandom::getrandom(&mut buf)
        .map_err(|e| StoreError::Internal(format!("failed to generate random bytes: {e}")))?;
    Ok(hex::encode(&buf))
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn row_to_gateway(row: &rusqlite::Row) -> rusqlite::Result<Gateway> {
    let meta_str: String = row.get(2)?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta_str).unwrap_or(serde_json::Value::Object(Default::default()));
    Ok(Gateway {
        gateway_id: row.get(0)?,
        status: row.get(1)?,
        meta,
        created_at: row.get(3)?,
        last_seen_at: row.get(4)?,
    })
}

fn row_to_device(row: &rusqlite::Row) -> rusqlite::Result<Device> {
    let meta_str: String = row.get(3)?;
    let meta: serde_json::Value =
        serde_json::from_str(&meta_str).unwrap_or(serde_json::Value::Object(Default::default()));
    Ok(Device {
        gateway_id: row.get(0)?,
        device_id: row.get(1)?,
        status: row.get(2)?,
        meta,
        created_at: row.get(4)?,
        last_seen_at: row.get(5)?,
    })
}

fn map_db_err(e: rusqlite::Error) -> StoreError {
    // SQLITE_BUSY still sneaks through when busy_timeout expires, surface it
    // as Unavailable so the HTTP layer returns 503 instead of 500 and the
    // caller knows to retry.
    if let rusqlite::Error::SqliteFailure(ref ffi_err, _) = e {
        if ffi_err.code == rusqlite::ErrorCode::DatabaseBusy {
            return StoreError::Unavailable(format!("database busy: {e}"));
        }
    }
    StoreError::Internal(format!("database error: {e}"))
}

#[async_trait]
impl RegistryStore for SqliteRegistryStore {
    async fn list_gateways(&self) -> Result<Vec<Gateway>, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways ORDER BY created_at DESC",
            )
            .map_err(map_db_err)?;
        let rows = stmt
            .query_map([], row_to_gateway)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    async fn get_gateway(&self, gateway_id: &str) -> Result<Option<Gateway>, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        let result = stmt
            .query_row(params![gateway_id], row_to_gateway)
            .optional()
            .map_err(map_db_err)?;
        Ok(result)
    }

    async fn list_devices(&self, gateway_id: &str) -> Result<Vec<Device>, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(map_db_err)?;
        let rows = stmt
            .query_map(params![gateway_id], row_to_device)
            .map_err(map_db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_db_err)?;
        Ok(rows)
    }

    async fn get_device(
        &self,
        gateway_id: &str,
        device_id: &str,
    ) -> Result<Option<Device>, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        let result = stmt
            .query_row(params![gateway_id, device_id], row_to_device)
            .optional()
            .map_err(map_db_err)?;
        Ok(result)
    }

    async fn upsert_device_seen(
        &self,
        gateway_id: &str,
        device_id: &str,
        meta: serde_json::Value,
    ) -> Result<Device, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        let meta_str = serde_json::to_string(&meta)
            .map_err(|e| StoreError::Internal(format!("json serialize error: {e}")))?;

        conn.execute(
            "INSERT INTO devices (gateway_id, device_id, status, meta, created_at, last_seen_at)
             VALUES (?1, ?2, 'active', ?3, ?4, ?4)
             ON CONFLICT(gateway_id, device_id) DO UPDATE SET
                 last_seen_at = ?4,
                 meta = ?3",
            params![gateway_id, device_id, meta_str, now],
        )
        .map_err(map_db_err)?;

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        stmt.query_row(params![gateway_id, device_id], row_to_device)
            .map_err(map_db_err)
    }

    async fn upsert_gateway_seen(
        &self,
        gateway_id: &str,
        meta: serde_json::Value,
    ) -> Result<Gateway, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        let meta_str = serde_json::to_string(&meta)
            .map_err(|e| StoreError::Internal(format!("json serialize error: {e}")))?;

        conn.execute(
            "INSERT INTO gateways (gateway_id, status, meta, created_at, last_seen_at)
             VALUES (?1, 'approved', ?2, ?3, ?3)
             ON CONFLICT(gateway_id) DO UPDATE SET
                 last_seen_at = ?3,
                 meta = ?2",
            params![gateway_id, meta_str, now],
        )
        .map_err(map_db_err)?;

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        stmt.query_row(params![gateway_id], row_to_gateway)
            .map_err(map_db_err)
    }

    async fn request_gateway_onboarding(
        &self,
        gateway_id: &str,
        meta: serde_json::Value,
    ) -> Result<(Gateway, String), StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let existing: Option<String> = conn
            .query_row(
                "SELECT status FROM gateways WHERE gateway_id = ?1",
                params![gateway_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_err)?;
        if existing.is_some() {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' already exists"
            )));
        }

        let now = chrono::Utc::now().to_rfc3339();
        let meta_str = serde_json::to_string(&meta)
            .map_err(|e| StoreError::Internal(format!("json serialize error: {e}")))?;

        let enrollment_token = generate_token(32)?;
        let enrollment_hash = hash_token(&enrollment_token);

        conn.execute(
            "INSERT INTO gateways (gateway_id, status, meta, created_at, last_seen_at, enrollment_token_hash)
             VALUES (?1, 'pending_approval', ?2, ?3, ?3, ?4)",
            params![gateway_id, meta_str, now, enrollment_hash],
        )
        .map_err(map_db_err)?;

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        let gw = stmt
            .query_row(params![gateway_id], row_to_gateway)
            .map_err(map_db_err)?;

        Ok((gw, enrollment_token))
    }

    async fn approve_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let rows = conn
            .execute(
                "UPDATE gateways SET status = 'approved' WHERE gateway_id = ?1 AND status = 'pending_approval'",
                params![gateway_id],
            )
            .map_err(map_db_err)?;

        if rows == 0 {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' not found or not in pending_approval status"
            )));
        }

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        stmt.query_row(params![gateway_id], row_to_gateway)
            .map_err(map_db_err)
    }

    async fn claim_gateway_access_token(
        &self,
        gateway_id: &str,
        enrollment_token: &str,
    ) -> Result<String, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT status, enrollment_token_hash FROM gateways WHERE gateway_id = ?1",
                params![gateway_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(map_db_err)?;

        let (status, stored_hash) = row.ok_or_else(|| {
            StoreError::BadRequest(format!("gateway '{gateway_id}' not found"))
        })?;

        if status != "approved" {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' is not approved (status: {status})"
            )));
        }

        let expected_hash = stored_hash.ok_or_else(|| {
            StoreError::BadRequest("no enrollment token set for this gateway".to_string())
        })?;
        if hash_token(enrollment_token) != expected_hash {
            return Err(StoreError::BadRequest(
                "invalid enrollment token".to_string(),
            ));
        }

        let access_token = generate_token(32)?;
        let access_hash = hash_token(&access_token);

        conn.execute(
            "UPDATE gateways SET access_token_hash = ?1, enrollment_token_hash = NULL
             WHERE gateway_id = ?2",
            params![access_hash, gateway_id],
        )
        .map_err(map_db_err)?;

        Ok(access_token)
    }

    async fn verify_gateway_access_token(
        &self,
        gateway_id: &str,
        token: &str,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let stored_hash: Option<Option<String>> = conn
            .query_row(
                "SELECT access_token_hash FROM gateways WHERE gateway_id = ?1 AND status = 'approved'",
                params![gateway_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_db_err)?;

        let Some(Some(stored)) = stored_hash else {
            return Ok(false);
        };

        Ok(hash_token(token) == stored)
    }

    async fn rotate_gateway_access_token(
        &self,
        gateway_id: &str,
        old_token: &str,
    ) -> Result<String, StoreError> {
        let valid = self.verify_gateway_access_token(gateway_id, old_token).await?;
        if !valid {
            return Err(StoreError::BadRequest(
                "invalid current access token".to_string(),
            ));
        }

        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let new_token = generate_token(32)?;
        let new_hash = hash_token(&new_token);

        conn.execute(
            "UPDATE gateways SET access_token_hash = ?1 WHERE gateway_id = ?2",
            params![new_hash, gateway_id],
        )
        .map_err(map_db_err)?;

        Ok(new_token)
    }

    async fn revoke_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let rows = conn
            .execute(
                "UPDATE gateways SET status = 'revoked', access_token_hash = NULL
                 WHERE gateway_id = ?1",
                params![gateway_id],
            )
            .map_err(map_db_err)?;

        if rows == 0 {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' not found"
            )));
        }

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        stmt.query_row(params![gateway_id], row_to_gateway)
            .map_err(map_db_err)
    }

    async fn decommission_gateway(&self, gateway_id: &str) -> Result<Gateway, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;

        let rows = conn
            .execute(
                "UPDATE gateways SET status = 'decommissioned', access_token_hash = NULL, enrollment_token_hash = NULL
                 WHERE gateway_id = ?1",
                params![gateway_id],
            )
            .map_err(map_db_err)?;

        if rows == 0 {
            return Err(StoreError::BadRequest(format!(
                "gateway '{gateway_id}' not found"
            )));
        }

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, status, meta, created_at, last_seen_at
                 FROM gateways WHERE gateway_id = ?1",
            )
            .map_err(map_db_err)?;
        stmt.query_row(params![gateway_id], row_to_gateway)
            .map_err(map_db_err)
    }

    async fn register_device(
        &self,
        gateway_id: &str,
        device_id: &str,
        meta: serde_json::Value,
    ) -> Result<Device, StoreError> {
        let conn = self.conn.lock().map_err(|e| StoreError::Internal(e.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        let meta_str = serde_json::to_string(&meta)
            .map_err(|e| StoreError::Internal(format!("json serialize error: {e}")))?;

        let result = conn.execute(
            "INSERT INTO devices (gateway_id, device_id, status, meta, created_at, last_seen_at)
             VALUES (?1, ?2, 'active', ?3, ?4, ?4)",
            params![gateway_id, device_id, meta_str, now],
        );

        match result {
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

        let mut stmt = conn
            .prepare(
                "SELECT gateway_id, device_id, status, meta, created_at, last_seen_at
                 FROM devices WHERE gateway_id = ?1 AND device_id = ?2",
            )
            .map_err(map_db_err)?;
        stmt.query_row(params![gateway_id, device_id], row_to_device)
            .map_err(map_db_err)
    }
