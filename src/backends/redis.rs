use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde_json::Value;
use tokio::sync::Mutex;

pub struct RedisBackend {
  url: String,
  conn: Mutex<Option<ConnectionManager>>,
}

impl RedisBackend {
  pub fn from_settings(settings: &Settings) -> Result<Self> {
    let url = settings
      .url
      .clone()
      .or_else(|| {
        settings.host.as_ref().map(|h| {
          let port = settings.port.unwrap_or(6379);
          format!("redis://{h}:{port}/")
        })
      })
      .unwrap_or_else(|| "redis://localhost/".into());
    Ok(Self {
      url,
      conn: Mutex::new(None),
    })
  }

  async fn cm(&self) -> Result<ConnectionManager> {
    self
      .conn
      .lock()
      .await
      .clone()
      .ok_or(UeberError::NotInitialized)
  }
}

#[async_trait]
impl Backend for RedisBackend {
  async fn init(&mut self) -> Result<()> {
    let client =
      redis::Client::open(self.url.as_str()).map_err(|e| UeberError::BackendInit(e.to_string()))?;
    let cm = ConnectionManager::new(client)
      .await
      .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    *self.conn.lock().await = Some(cm);
    Ok(())
  }

  async fn close(&self) -> Result<()> {
    *self.conn.lock().await = None;
    Ok(())
  }

  async fn get(&self, key: &str) -> Result<Option<Value>> {
    let mut cm = self.cm().await?;
    let raw: Option<String> = cm
      .get(key)
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    match raw {
      Some(s) => Ok(Some(serde_json::from_str(&s)?)),
      None => Ok(None),
    }
  }

  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    let mut cm = self.cm().await?;
    let json = serde_json::to_string(value)?;
    let _: () = cm
      .set(key, json)
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    Ok(())
  }

  async fn remove(&self, key: &str) -> Result<()> {
    let mut cm = self.cm().await?;
    let _: () = cm
      .del(key)
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    Ok(())
  }

  async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
    let mut cm = self.cm().await?;
    // Redis SCAN's MATCH pattern uses the same `*` glob ueberDB exposes,
    // so the pattern can be passed straight through. The safe_iterators
    // feature makes next_item return Result<T, _> so partial-decode
    // failures aren't silently swallowed.
    let mut iter = cm
      .scan_match::<_, String>(key)
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    let mut out: Vec<String> = Vec::new();
    while let Some(item) = iter.next_item().await {
      let k = item.map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      out.push(k);
    }
    // Redis doesn't have NOT-MATCH, so filter exclusions client-side.
    if let Some(nk) = not_key {
      let pattern = crate::wrapper::find_keys::compile_find_pattern(key, Some(nk));
      out.retain(|k| pattern.matches(k));
    }
    Ok(out)
  }

  async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
    let mut cm = self.cm().await?;
    let mut pipe = redis::pipe();
    for op in ops {
      match op {
        BulkOp::Set { key, value } => {
          let json = serde_json::to_string(value)?;
          pipe.set(key, json).ignore();
        }
        BulkOp::Remove { key } => {
          pipe.del(key).ignore();
        }
      }
    }
    let _: () = pipe
      .query_async(&mut cm)
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
