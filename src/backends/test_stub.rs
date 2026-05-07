//! Test-only stub backend. Records calls so wrapper-layer tests can assert on them.

#![cfg(test)]

use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct StubBackend {
  pub store: Mutex<HashMap<String, Value>>,
  pub call_log: Mutex<Vec<String>>,
}

#[async_trait]
impl Backend for StubBackend {
  async fn init(&mut self) -> Result<()> {
    self.call_log.lock().unwrap().push("init".into());
    Ok(())
  }
  async fn close(&self) -> Result<()> {
    self.call_log.lock().unwrap().push("close".into());
    Ok(())
  }
  async fn get(&self, key: &str) -> Result<Option<Value>> {
    self.call_log.lock().unwrap().push(format!("get:{key}"));
    Ok(self.store.lock().unwrap().get(key).cloned())
  }
  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    self.call_log.lock().unwrap().push(format!("set:{key}"));
    self
      .store
      .lock()
      .unwrap()
      .insert(key.to_string(), value.clone());
    Ok(())
  }
  async fn remove(&self, key: &str) -> Result<()> {
    self.call_log.lock().unwrap().push(format!("remove:{key}"));
    self.store.lock().unwrap().remove(key);
    Ok(())
  }
  async fn find_keys(&self, _key: &str, _not_key: Option<&str>) -> Result<Vec<String>> {
    self.call_log.lock().unwrap().push("find_keys".into());
    Ok(self.store.lock().unwrap().keys().cloned().collect())
  }
  async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
    self
      .call_log
      .lock()
      .unwrap()
      .push(format!("do_bulk:{}", ops.len()));
    let mut s = self.store.lock().unwrap();
    for op in ops {
      match op {
        BulkOp::Set { key, value } => {
          s.insert(key.clone(), value.clone());
        }
        BulkOp::Remove { key } => {
          s.remove(key);
        }
      }
    }
    Ok(())
  }
  fn default_wrapper_settings(&self) -> DefaultWrapperHints {
    DefaultWrapperHints::default()
  }
}
