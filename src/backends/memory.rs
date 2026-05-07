use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::Result;
use crate::wrapper::find_keys::compile_find_pattern;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct MemoryBackend {
  data: RwLock<HashMap<String, Value>>,
}

#[async_trait]
impl Backend for MemoryBackend {
  async fn init(&mut self) -> Result<()> {
    Ok(())
  }
  async fn close(&self) -> Result<()> {
    self.data.write().await.clear();
    Ok(())
  }
  async fn get(&self, key: &str) -> Result<Option<Value>> {
    Ok(self.data.read().await.get(key).cloned())
  }
  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    self
      .data
      .write()
      .await
      .insert(key.to_string(), value.clone());
    Ok(())
  }
  async fn remove(&self, key: &str) -> Result<()> {
    self.data.write().await.remove(key);
    Ok(())
  }
  async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
    let p = compile_find_pattern(key, not_key);
    Ok(
      self
        .data
        .read()
        .await
        .keys()
        .filter(|k| p.matches(k))
        .cloned()
        .collect(),
    )
  }
  async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
    let mut guard = self.data.write().await;
    for op in ops {
      match op {
        BulkOp::Set { key, value } => {
          guard.insert(key.clone(), value.clone());
        }
        BulkOp::Remove { key } => {
          guard.remove(key);
        }
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

  #[tokio::test]
  async fn round_trip() {
    let mut b = MemoryBackend::default();
    b.init().await.unwrap();
    b.set("k", &json!(1)).await.unwrap();
    assert_eq!(b.get("k").await.unwrap(), Some(json!(1)));
    b.remove("k").await.unwrap();
    assert!(b.get("k").await.unwrap().is_none());
  }

  #[tokio::test]
  async fn find_keys_glob() {
    let b = MemoryBackend::default();
    b.set("a:1", &json!(1)).await.unwrap();
    b.set("a:2", &json!(2)).await.unwrap();
    b.set("b:1", &json!(3)).await.unwrap();
    let mut keys = b.find_keys("a:*", None).await.unwrap();
    keys.sort();
    assert_eq!(keys, vec!["a:1", "a:2"]);
  }

  #[tokio::test]
  async fn do_bulk_applies_in_order() {
    let b = MemoryBackend::default();
    b.do_bulk(&[
      BulkOp::Set {
        key: "x".into(),
        value: json!(1),
      },
      BulkOp::Set {
        key: "y".into(),
        value: json!(2),
      },
      BulkOp::Remove { key: "x".into() },
    ])
    .await
    .unwrap();
    assert!(b.get("x").await.unwrap().is_none());
    assert_eq!(b.get("y").await.unwrap(), Some(json!(2)));
  }
}
