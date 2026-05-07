use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use crate::wrapper::find_keys::compile_find_pattern;
use async_trait::async_trait;
use scylla::{Session, SessionBuilder};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct CassandraBackend {
    contact_points: Vec<String>,
    keyspace: String,
    table: String,
    session: Arc<Mutex<Option<Arc<Session>>>>,
}

impl CassandraBackend {
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        // Settings.client_options is a serde_json::Value carrying
        // contactPoints / keyspace / localDataCenter, mirroring ueberDB.
        let opts = settings.client_options.clone().unwrap_or(Value::Null);
        let contact_points = opts
            .get("contactPoints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .or_else(|| {
                settings
                    .host
                    .as_ref()
                    .map(|h| vec![format!("{}:{}", h, settings.port.unwrap_or(9042))])
            })
            .unwrap_or_else(|| vec!["127.0.0.1:9042".to_string()]);
        let keyspace = opts
            .get("keyspace")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| settings.database.clone())
            .unwrap_or_else(|| "ueberdb".into());
        let table = settings
            .column_family
            .clone()
            .unwrap_or_else(|| "store".into());
        Ok(Self {
            contact_points,
            keyspace,
            table,
            session: Arc::new(Mutex::new(None)),
        })
    }

    async fn session(&self) -> Result<Arc<Session>> {
        self.session
            .lock()
            .await
            .clone()
            .ok_or(UeberError::NotInitialized)
    }
}

#[async_trait]
impl Backend for CassandraBackend {
    async fn init(&mut self) -> Result<()> {
        let mut sb = SessionBuilder::new();
        for cp in &self.contact_points {
            sb = sb.known_node(cp);
        }
        let session = sb
            .build()
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        // Create keyspace + table if missing. Use SimpleStrategy with rf=1
        // for the test harness; production deployments override the
        // keyspace via settings.client_options.
        let create_ks = format!(
            "CREATE KEYSPACE IF NOT EXISTS {ks} \
             WITH REPLICATION = {{'class': 'SimpleStrategy', 'replication_factor': 1}}",
            ks = self.keyspace
        );
        session
            .query_unpaged(create_ks, &[])
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        let use_ks = format!("USE {ks}", ks = self.keyspace);
        session
            .query_unpaged(use_ks, &[])
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        let create_tbl = format!(
            "CREATE TABLE IF NOT EXISTS {tbl} (key text PRIMARY KEY, value text)",
            tbl = self.table
        );
        session
            .query_unpaged(create_tbl, &[])
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        *self.session.lock().await = Some(Arc::new(session));
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        *self.session.lock().await = None;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        let s = self.session().await?;
        let q = format!(
            "SELECT value FROM {ks}.{tbl} WHERE key = ?",
            ks = self.keyspace,
            tbl = self.table
        );
        let result = s
            .query_unpaged(q, (key,))
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        let mut iter = result
            .rows_typed_or_empty::<(String,)>();
        match iter.next() {
            Some(row) => {
                let (s,) = row.map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                Ok(Some(serde_json::from_str(&s)?))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        let s = self.session().await?;
        let json = serde_json::to_string(value)?;
        let q = format!(
            "INSERT INTO {ks}.{tbl} (key, value) VALUES (?, ?)",
            ks = self.keyspace,
            tbl = self.table
        );
        s.query_unpaged(q, (key, json))
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let s = self.session().await?;
        let q = format!(
            "DELETE FROM {ks}.{tbl} WHERE key = ?",
            ks = self.keyspace,
            tbl = self.table
        );
        s.query_unpaged(q, (key,))
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
        // Cassandra has no LIKE without SASI indexes; fall back to a full
        // scan + wrapper-side regex matching. Scope is fine for ueberdb's
        // typical usage (small key population per Etherpad pad).
        let s = self.session().await?;
        let p = compile_find_pattern(key, not_key);
        let q = format!(
            "SELECT key FROM {ks}.{tbl}",
            ks = self.keyspace,
            tbl = self.table
        );
        let result = s
            .query_unpaged(q, &[])
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        let mut out = Vec::new();
        for row in result.rows_typed_or_empty::<(String,)>() {
            let (k,) = row.map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            if p.matches(&k) {
                out.push(k);
            }
        }
        Ok(out)
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
        for op in ops {
            match op {
                BulkOp::Set { key, value } => self.set(key, value).await?,
                BulkOp::Remove { key } => self.remove(key).await?,
            }
        }
        Ok(())
    }

    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        DefaultWrapperHints {
            cache: Some(1000),
            write_interval: Some(100),
            json: Some(true),
        }
    }
}
