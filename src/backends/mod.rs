use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum BulkOp {
    Set { key: String, value: Value },
    Remove { key: String },
}

#[async_trait]
pub trait Backend: Send + Sync {
    async fn init(&mut self) -> Result<()>;
    async fn close(&self) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<Value>>;
    async fn set(&self, key: &str, value: &Value) -> Result<()>;
    async fn remove(&self, key: &str) -> Result<()>;
    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>>;
    async fn do_bulk(&self, _ops: &[BulkOp]) -> Result<()> {
        Err(UeberError::DoBulkNotImplemented)
    }
    fn supports_native_glob(&self) -> bool {
        false
    }
    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        DefaultWrapperHints::default()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultWrapperHints {
    pub cache: Option<u32>,
    pub write_interval: Option<u32>,
    pub json: Option<bool>,
}

pub async fn factory(type_: &str, _settings: &Settings) -> Result<Box<dyn Backend>> {
    match type_ {
        #[cfg(test)]
        "_stub" => Ok(Box::new(test_stub::StubBackend::default())),
        // Phase 3 tasks each register a backend here.
        other => Err(UeberError::UnknownBackend(other.to_string())),
    }
}

#[cfg(test)]
pub mod test_stub;

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_stub::StubBackend;
    use serde_json::json;

    #[tokio::test]
    async fn factory_unknown_backend_errors() {
        let s = Settings::default();
        let result = factory("nope", &s).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, UeberError::UnknownBackend(_)));
    }

    #[tokio::test]
    async fn stub_round_trip() {
        let mut b = StubBackend::default();
        b.init().await.unwrap();
        b.set("k", &json!(1)).await.unwrap();
        assert_eq!(b.get("k").await.unwrap(), Some(json!(1)));
        b.remove("k").await.unwrap();
        assert!(b.get("k").await.unwrap().is_none());
        // Verify call log captured the sequence.
        let log = b.call_log.lock().unwrap().clone();
        assert_eq!(log, vec!["init", "set:k", "get:k", "remove:k", "get:k"]);
    }
}
