use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use serde_json::Value;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use tokio::sync::Mutex;

const CREATE_TABLE_SQL: &str =
    "CREATE TABLE IF NOT EXISTS store (key TEXT PRIMARY KEY, value TEXT NOT NULL)";

pub struct SqliteBackend {
    url: String,
    pool: Mutex<Option<SqlitePool>>,
    in_memory: bool,
}

impl SqliteBackend {
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let filename = settings
            .filename
            .clone()
            .unwrap_or_else(|| ":memory:".into());
        let in_memory = filename == ":memory:";
        let url = if in_memory {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{filename}?mode=rwc")
        };
        Ok(Self {
            url,
            pool: Mutex::new(None),
            in_memory,
        })
    }

    async fn with_pool<R, F, Fut>(&self, f: F) -> Result<R>
    where
        F: FnOnce(SqlitePool) -> Fut,
        Fut: std::future::Future<Output = Result<R>>,
    {
        let pool = {
            let guard = self.pool.lock().await;
            guard.as_ref().ok_or(UeberError::NotInitialized)?.clone()
        };
        f(pool).await
    }
}

#[async_trait]
impl Backend for SqliteBackend {
    async fn init(&mut self) -> Result<()> {
        let pool = SqlitePoolOptions::new()
            // In-memory databases share state only within one connection.
            .max_connections(if self.in_memory { 1 } else { 5 })
            .connect(&self.url)
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        sqlx::query(CREATE_TABLE_SQL)
            .execute(&pool)
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        *self.pool.lock().await = Some(pool);
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        if let Some(p) = self.pool.lock().await.take() {
            p.close().await;
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        self.with_pool(|p| async move {
            let row: Option<(String,)> = sqlx::query_as("SELECT value FROM store WHERE key = ?")
                .bind(key)
                .fetch_optional(&p)
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            match row {
                Some((s,)) => Ok(Some(serde_json::from_str(&s)?)),
                None => Ok(None),
            }
        })
        .await
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        let json = serde_json::to_string(value)?;
        self.with_pool(|p| async move {
            sqlx::query("REPLACE INTO store (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(json)
                .execute(&p)
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(())
        })
        .await
    }

    async fn remove(&self, key: &str) -> Result<()> {
        self.with_pool(|p| async move {
            sqlx::query("DELETE FROM store WHERE key = ?")
                .bind(key)
                .execute(&p)
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(())
        })
        .await
    }

    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
        let pattern = key.replace('*', "%");
        let not_pattern = not_key.map(|nk| nk.replace('*', "%"));
        self.with_pool(|p| async move {
            let rows = if let Some(np) = not_pattern {
                sqlx::query("SELECT key FROM store WHERE key LIKE ? AND key NOT LIKE ?")
                    .bind(pattern)
                    .bind(np)
                    .fetch_all(&p)
                    .await
            } else {
                sqlx::query("SELECT key FROM store WHERE key LIKE ?")
                    .bind(pattern)
                    .fetch_all(&p)
                    .await
            }
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(rows.into_iter().map(|r| r.get::<String, _>(0)).collect())
        })
        .await
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
        self.with_pool(|p| async move {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            for op in ops {
                match op {
                    BulkOp::Set { key, value } => {
                        let json = serde_json::to_string(value)?;
                        sqlx::query("REPLACE INTO store (key, value) VALUES (?, ?)")
                            .bind(key)
                            .bind(json)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                    }
                    BulkOp::Remove { key } => {
                        sqlx::query("DELETE FROM store WHERE key = ?")
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
        })
        .await
    }

    fn supports_native_glob(&self) -> bool {
        true
    }

    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        if self.in_memory {
            DefaultWrapperHints {
                cache: Some(0),
                write_interval: Some(0),
                json: Some(true),
            }
        } else {
            DefaultWrapperHints {
                cache: Some(1000),
                write_interval: Some(100),
                json: Some(true),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn round_trip_in_memory() {
        let mut b = SqliteBackend::from_settings(&Settings {
            filename: Some(":memory:".into()),
            ..Default::default()
        })
        .unwrap();
        b.init().await.unwrap();
        b.set("k", &json!({"x": 1})).await.unwrap();
        assert_eq!(b.get("k").await.unwrap(), Some(json!({"x": 1})));
        b.do_bulk(&[BulkOp::Remove { key: "k".into() }])
            .await
            .unwrap();
        assert!(b.get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn find_keys_uses_native_like() {
        let mut b = SqliteBackend::from_settings(&Settings {
            filename: Some(":memory:".into()),
            ..Default::default()
        })
        .unwrap();
        b.init().await.unwrap();
        b.set("a:1", &json!(1)).await.unwrap();
        b.set("a:2", &json!(2)).await.unwrap();
        b.set("b:1", &json!(3)).await.unwrap();
        let mut keys = b.find_keys("a:*", None).await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["a:1", "a:2"]);
    }
}
