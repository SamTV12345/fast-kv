use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use crate::wrapper::find_keys::compile_find_pattern;
use async_trait::async_trait;
use redb::{Database, ReadableTable, TableDefinition};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task;

const TABLE: TableDefinition<&str, &str> = TableDefinition::new("store");

pub struct RustyBackend {
    path: PathBuf,
    db: Mutex<Option<Arc<Database>>>,
}

impl RustyBackend {
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let p = settings
            .filename
            .clone()
            .ok_or_else(|| UeberError::Config("rusty: filename required".into()))?;
        Ok(Self {
            path: PathBuf::from(p),
            db: Mutex::new(None),
        })
    }

    fn db_arc(&self) -> Result<Arc<Database>> {
        self.db
            .lock()
            .unwrap()
            .clone()
            .ok_or(UeberError::NotInitialized)
    }
}

#[async_trait]
impl Backend for RustyBackend {
    async fn init(&mut self) -> Result<()> {
        let p = self.path.clone();
        let db = task::spawn_blocking(move || Database::create(&p))
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        // Ensure the store table exists so reads on a fresh DB don't fail.
        {
            let wt = db
                .begin_write()
                .map_err(|e| UeberError::BackendInit(e.to_string()))?;
            wt.open_table(TABLE)
                .map_err(|e| UeberError::BackendInit(e.to_string()))?;
            wt.commit()
                .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        }
        *self.db.lock().unwrap() = Some(Arc::new(db));
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        // Drop the Arc<Database>; redb closes when refcount hits zero.
        *self.db.lock().unwrap() = None;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        let db = self.db_arc()?;
        let key = key.to_string();
        let raw = task::spawn_blocking(move || -> Result<Option<String>> {
            let rt = db
                .begin_read()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            let t = rt
                .open_table(TABLE)
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(t.get(key.as_str())
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
                .map(|v| v.value().to_string()))
        })
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))??;
        match raw {
            Some(s) => Ok(Some(serde_json::from_str(&s)?)),
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        let db = self.db_arc()?;
        let k = key.to_string();
        let v = serde_json::to_string(value)?;
        task::spawn_blocking(move || -> Result<()> {
            let wt = db
                .begin_write()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            {
                let mut t = wt
                    .open_table(TABLE)
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                t.insert(k.as_str(), v.as_str())
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            }
            wt.commit()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let db = self.db_arc()?;
        let k = key.to_string();
        task::spawn_blocking(move || -> Result<()> {
            let wt = db
                .begin_write()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            {
                let mut t = wt
                    .open_table(TABLE)
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                t.remove(k.as_str())
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            }
            wt.commit()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
    }

    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
        let db = self.db_arc()?;
        let p = compile_find_pattern(key, not_key);
        let keys = task::spawn_blocking(move || -> Result<Vec<String>> {
            let rt = db
                .begin_read()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            let t = rt
                .open_table(TABLE)
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            let iter = t
                .iter()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            let mut out = Vec::new();
            for entry in iter {
                let (k, _) = entry.map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                out.push(k.value().to_string());
            }
            Ok(out)
        })
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))??;
        Ok(keys.into_iter().filter(|k| p.matches(k)).collect())
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
        let db = self.db_arc()?;
        // Encode each Set's JSON outside the blocking task (cheap, avoids moving Value).
        let owned: Vec<(String, Option<String>)> = ops
            .iter()
            .map(|op| match op {
                BulkOp::Set { key, value } => {
                    Ok((key.clone(), Some(serde_json::to_string(value)?)))
                }
                BulkOp::Remove { key } => Ok((key.clone(), None)),
            })
            .collect::<Result<_>>()?;
        task::spawn_blocking(move || -> Result<()> {
            let wt = db
                .begin_write()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            {
                let mut t = wt
                    .open_table(TABLE)
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                for (key, encoded) in &owned {
                    match encoded {
                        Some(json) => {
                            t.insert(key.as_str(), json.as_str())
                                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                        }
                        None => {
                            t.remove(key.as_str())
                                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                        }
                    }
                }
            }
            wt.commit()
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
            Ok(())
        })
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
    }

    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        DefaultWrapperHints {
            cache: Some(1000),
            write_interval: Some(100),
            json: Some(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn round_trip() {
        let d = tempdir().unwrap();
        let p = d.path().join("rusty.redb").to_string_lossy().into_owned();
        let s = Settings {
            filename: Some(p),
            ..Default::default()
        };
        let mut b = RustyBackend::from_settings(&s).unwrap();
        b.init().await.unwrap();
        b.set("k", &json!(42)).await.unwrap();
        assert_eq!(b.get("k").await.unwrap(), Some(json!(42)));
        b.remove("k").await.unwrap();
        assert!(b.get("k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn do_bulk_atomic() {
        let d = tempdir().unwrap();
        let p = d.path().join("bulk.redb").to_string_lossy().into_owned();
        let s = Settings {
            filename: Some(p),
            ..Default::default()
        };
        let mut b = RustyBackend::from_settings(&s).unwrap();
        b.init().await.unwrap();
        b.do_bulk(&[
            BulkOp::Set {
                key: "a".into(),
                value: json!(1),
            },
            BulkOp::Set {
                key: "b".into(),
                value: json!(2),
            },
            BulkOp::Remove { key: "a".into() },
        ])
        .await
        .unwrap();
        assert!(b.get("a").await.unwrap().is_none());
        assert_eq!(b.get("b").await.unwrap(), Some(json!(2)));
    }
}
