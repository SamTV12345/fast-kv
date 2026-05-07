use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use crate::wrapper::find_keys::compile_find_pattern;
use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize)]
struct Record {
  key: String,
  val: Value,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  deleted: Option<bool>,
}

pub struct DirtyBackend {
  path: PathBuf,
  file: Mutex<Option<File>>,
  index: Mutex<HashMap<String, Value>>,
}

impl DirtyBackend {
  pub fn from_settings(settings: &Settings) -> Result<Self> {
    let filename = settings
      .filename
      .as_ref()
      .ok_or_else(|| UeberError::Config("dirty: filename required".into()))?;
    Ok(Self {
      path: PathBuf::from(filename),
      file: Mutex::new(None),
      index: Mutex::new(HashMap::new()),
    })
  }
}

#[async_trait]
impl Backend for DirtyBackend {
  async fn init(&mut self) -> Result<()> {
    let f = OpenOptions::new()
      .read(true)
      .create(true)
      .append(true)
      .open(&self.path)
      .await
      .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    // Replay log into in-memory index.
    let mut idx = self.index.lock().await;
    let read_f = File::open(&self.path)
      .await
      .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    let mut reader = BufReader::new(read_f).lines();
    while let Some(line) = reader
      .next_line()
      .await
      .map_err(|e| UeberError::BackendInit(e.to_string()))?
    {
      if line.trim().is_empty() {
        continue;
      }
      if let Ok(rec) = serde_json::from_str::<Record>(&line) {
        if rec.deleted.unwrap_or(false) {
          idx.remove(&rec.key);
        } else {
          idx.insert(rec.key, rec.val);
        }
      }
    }
    *self.file.lock().await = Some(f);
    Ok(())
  }

  async fn close(&self) -> Result<()> {
    if let Some(mut f) = self.file.lock().await.take() {
      let _ = f.flush().await;
    }
    Ok(())
  }

  async fn get(&self, key: &str) -> Result<Option<Value>> {
    Ok(self.index.lock().await.get(key).cloned())
  }

  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    let rec = Record {
      key: key.to_string(),
      val: value.clone(),
      deleted: None,
    };
    let line = serde_json::to_string(&rec)? + "\n";
    let mut guard = self.file.lock().await;
    let f = guard.as_mut().ok_or(UeberError::NotInitialized)?;
    f.write_all(line.as_bytes())
      .await
      .context("dirty: append")
      .map_err(UeberError::Backend)?;
    f.flush()
      .await
      .context("dirty: flush")
      .map_err(UeberError::Backend)?;
    self
      .index
      .lock()
      .await
      .insert(key.to_string(), value.clone());
    Ok(())
  }

  async fn remove(&self, key: &str) -> Result<()> {
    let rec = Record {
      key: key.to_string(),
      val: Value::Null,
      deleted: Some(true),
    };
    let line = serde_json::to_string(&rec)? + "\n";
    let mut guard = self.file.lock().await;
    let f = guard.as_mut().ok_or(UeberError::NotInitialized)?;
    f.write_all(line.as_bytes())
      .await
      .context("dirty: append")
      .map_err(UeberError::Backend)?;
    f.flush()
      .await
      .context("dirty: flush")
      .map_err(UeberError::Backend)?;
    self.index.lock().await.remove(key);
    Ok(())
  }

  async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
    let p = compile_find_pattern(key, not_key);
    Ok(
      self
        .index
        .lock()
        .await
        .keys()
        .filter(|k| p.matches(k))
        .cloned()
        .collect(),
    )
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
      cache: Some(0),
      write_interval: Some(0),
      json: Some(false),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  use tempfile::tempdir;

  #[tokio::test]
  async fn round_trip_with_replay() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("test.dirty").to_string_lossy().into_owned();

    // Phase 1: write + close.
    {
      let s = Settings {
        filename: Some(p.clone()),
        ..Default::default()
      };
      let mut b = DirtyBackend::from_settings(&s).unwrap();
      b.init().await.unwrap();
      b.set("k", &json!({"a": 1})).await.unwrap();
      b.set("doomed", &json!(99)).await.unwrap();
      b.remove("doomed").await.unwrap();
      b.close().await.unwrap();
    }

    // Phase 2: reopen, replay must reflect prior writes.
    {
      let s = Settings {
        filename: Some(p.clone()),
        ..Default::default()
      };
      let mut b = DirtyBackend::from_settings(&s).unwrap();
      b.init().await.unwrap();
      assert_eq!(b.get("k").await.unwrap(), Some(json!({"a": 1})));
      assert!(b.get("doomed").await.unwrap().is_none());
    }
  }
}
