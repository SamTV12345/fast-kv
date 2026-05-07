use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use sqlx::Row;

// Per-column CHARACTER SET pins the columns to a textual encoding even when
// the connection or schema defaults differ; without it sqlx 0.8 sometimes
// decodes the columns as BLOB on MySQL 8 and a `String` decode then fails.
const CREATE_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS store (\
    `key` VARCHAR(100) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin PRIMARY KEY, \
    `value` MEDIUMTEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL\
) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin";

pub struct MysqlBackend {
    url: String,
    pool: Option<MySqlPool>,
}

impl MysqlBackend {
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let host = settings.host.as_deref().unwrap_or("localhost");
        let port = settings.port.unwrap_or(3306);
        let user = settings
            .user
            .as_deref()
            .ok_or_else(|| UeberError::Config("mysql: user required".into()))?;
        let pass = settings
            .password
            .as_deref()
            .ok_or_else(|| UeberError::Config("mysql: password required".into()))?;
        let db = settings
            .database
            .as_deref()
            .ok_or_else(|| UeberError::Config("mysql: database required".into()))?;
        let url = format!("mysql://{user}:{pass}@{host}:{port}/{db}?charset=utf8mb4");
        Ok(Self { url, pool: None })
    }
}

#[async_trait]
impl Backend for MysqlBackend {
    async fn init(&mut self) -> Result<()> {
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&self.url)
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        sqlx::query(CREATE_TABLE_SQL)
            .execute(&pool)
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        self.pool = Some(pool);
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        if let Some(p) = self.pool.as_ref() {
            p.close().await;
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        let p = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
        // utf8mb4_bin makes MySQL report TEXT/VARCHAR columns as VARBINARY
        // over the wire, so sqlx 0.8 refuses to decode them into `String`.
        // Read as bytes and parse JSON from the slice instead.
        //
        // The `BINARY key = ?` clause forces a byte-exact comparison so
        // trailing spaces don't get stripped (MySQL's standard `=` for
        // VARCHAR ignores trailing whitespace) — same fix ueberDB's
        // mysql_db.ts uses.
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            "SELECT `value` FROM store WHERE `key` = ? AND BINARY `key` = ?",
        )
        .bind(key)
        .bind(key)
        .fetch_optional(p)
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        match row {
            Some((bytes,)) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        let p = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
        let json = serde_json::to_string(value)?;
        sqlx::query(
            "INSERT INTO store (`key`, `value`) VALUES (?, ?) \
             ON DUPLICATE KEY UPDATE `value` = VALUES(`value`)",
        )
        .bind(key)
        .bind(json)
        .execute(p)
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let p = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
        sqlx::query("DELETE FROM store WHERE `key` = ? AND BINARY `key` = ?")
            .bind(key)
            .bind(key)
            .execute(p)
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
        let p = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
        let pattern = key.replace('*', "%");
        let rows = if let Some(nk) = not_key {
            let np = nk.replace('*', "%");
            sqlx::query("SELECT `key` FROM store WHERE `key` LIKE ? AND `key` NOT LIKE ?")
                .bind(pattern)
                .bind(np)
        } else {
            sqlx::query("SELECT `key` FROM store WHERE `key` LIKE ?").bind(pattern)
        }
        .fetch_all(p)
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        // Same VARBINARY-vs-VARCHAR coercion problem as `get` — pull bytes
        // and rebuild Strings.
        rows.into_iter()
            .map(|r| {
                let bytes: Vec<u8> = r.get(0);
                String::from_utf8(bytes)
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))
            })
            .collect()
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
        let p = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
        let mut tx = p
            .begin()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        for op in ops {
            match op {
                BulkOp::Set { key, value } => {
                    let json = serde_json::to_string(value)?;
                    sqlx::query(
                        "INSERT INTO store (`key`, `value`) VALUES (?, ?) \
                         ON DUPLICATE KEY UPDATE `value` = VALUES(`value`)",
                    )
                    .bind(key)
                    .bind(json)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                }
                BulkOp::Remove { key } => {
                    sqlx::query("DELETE FROM store WHERE `key` = ? AND BINARY `key` = ?")
                        .bind(key)
                        .bind(key)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                }
            }
        }
        tx.commit()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    fn supports_native_glob(&self) -> bool {
        true
    }

    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        DefaultWrapperHints {
            cache: Some(1000),
            write_interval: Some(100),
            json: Some(true),
        }
    }
}
