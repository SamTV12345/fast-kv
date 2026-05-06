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
    async fn close(&mut self) -> Result<()>;
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
        // Phase 3 tasks each register a backend here.
        other => Err(UeberError::UnknownBackend(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn factory_unknown_backend_errors() {
        let s = Settings::default();
        let result = factory("nope", &s).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, UeberError::UnknownBackend(_)));
    }
}
