use crate::backends::{factory, Backend};
use crate::error::{Result as UResult, UeberError};
use crate::settings::{Settings, WrapperSettings};
use crate::wrapper::cache::ReadCache;
use crate::wrapper::find_keys::compile_find_pattern;
use crate::wrapper::locks::KeyLocks;
use crate::wrapper::logger::Logger;
use crate::wrapper::metrics::{Metrics, MetricsCore};
use crate::wrapper::sub_path::{get_sub, set_sub};
use crate::wrapper::write_buffer::WriteBuffer;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

#[napi]
pub struct Database {
    type_: String,
    settings: Settings,
    #[allow(dead_code)]
    wrapper_settings: WrapperSettings,
    backend: AsyncMutex<Option<Arc<dyn Backend>>>,
    cache: Arc<ReadCache>,
    write_buffer: Arc<WriteBuffer>,
    locks: Arc<KeyLocks>,
    metrics: Arc<MetricsCore>,
    #[allow(dead_code)]
    logger: Logger,
    flush_token: AsyncMutex<Option<CancellationToken>>,
}

#[napi]
impl Database {
    #[napi(constructor)]
    pub fn new(
        type_: String,
        settings: Settings,
        wrapper_settings: Option<WrapperSettings>,
    ) -> Self {
        let ws = wrapper_settings.unwrap_or_default();
        Self {
            type_,
            settings,
            cache: Arc::new(ReadCache::new(ws.cache_capacity())),
            write_buffer: Arc::new(WriteBuffer::new(ws.write_interval_ms(), ws.bulk_limit())),
            locks: Arc::new(KeyLocks::default()),
            metrics: Arc::new(MetricsCore::default()),
            logger: Logger::none(),
            backend: AsyncMutex::new(None),
            flush_token: AsyncMutex::new(None),
            wrapper_settings: ws,
        }
    }

    #[napi]
    pub async fn init(&self) -> Result<()> {
        let mut b = factory(&self.type_, &self.settings).await?;
        b.init().await?;
        let backend: Arc<dyn Backend> = Arc::from(b);
        let token = self
            .write_buffer
            .clone()
            .spawn_flush_task(backend.clone(), self.metrics.clone());
        *self.backend.lock().await = Some(backend);
        *self.flush_token.lock().await = Some(token);
        Ok(())
    }

    async fn backend_arc(&self) -> UResult<Arc<dyn Backend>> {
        self.backend
            .lock()
            .await
            .clone()
            .ok_or(UeberError::NotInitialized)
    }

    #[napi]
    pub async fn close(&self) -> Result<()> {
        let backend = self.backend_arc().await?;
        // Final flush of any buffered writes.
        self.write_buffer.flush_now(&backend, &self.metrics).await?;
        // Stop the periodic flush task.
        if let Some(token) = self.flush_token.lock().await.take() {
            token.cancel();
        }
        // Close the backend (now safe — close takes &self).
        backend.close().await?;
        // Drop the backend so future calls return NotInitialized.
        *self.backend.lock().await = None;
        Ok(())
    }

    #[napi]
    pub async fn flush(&self) -> Result<()> {
        let backend = self.backend_arc().await?;
        self.write_buffer.flush_now(&backend, &self.metrics).await?;
        Ok(())
    }

    #[napi]
    pub async fn get(&self, key: String) -> Result<Option<Value>> {
        let backend = self.backend_arc().await?;
        self.metrics.inc(&self.metrics.reads);
        if let Some(buf) = self.write_buffer.buffered_get(&key) {
            return Ok(buf);
        }
        if let Some(v) = self.cache.get(&key).await {
            self.metrics.inc(&self.metrics.cache_hits);
            return Ok(Some((*v).clone()));
        }
        self.metrics.inc(&self.metrics.cache_misses);
        let lock = self.locks.get(&key);
        let _g = lock.lock().await;
        let v = backend.get(&key).await?;
        if let Some(ref vv) = v {
            self.cache.put(key, Arc::new(vv.clone())).await;
        }
        Ok(v)
    }

    #[napi]
    pub async fn set(&self, key: String, value: Value) -> Result<()> {
        let backend = self.backend_arc().await?;
        self.metrics.inc(&self.metrics.writes);
        let lock = self.locks.get(&key);
        let _g = lock.lock().await;
        self.cache.invalidate(&key).await;
        if self.write_buffer.enabled() {
            self.write_buffer.enqueue_set(key, value);
            Ok(())
        } else {
            backend.set(&key, &value).await?;
            Ok(())
        }
    }

    #[napi]
    pub async fn remove(&self, key: String) -> Result<()> {
        let backend = self.backend_arc().await?;
        self.metrics.inc(&self.metrics.removes);
        let lock = self.locks.get(&key);
        let _g = lock.lock().await;
        self.cache.invalidate(&key).await;
        if self.write_buffer.enabled() {
            self.write_buffer.enqueue_remove(key);
            Ok(())
        } else {
            backend.remove(&key).await?;
            Ok(())
        }
    }

    #[napi(js_name = "getSub")]
    pub async fn get_sub(&self, key: String, path: Vec<String>) -> Result<Option<Value>> {
        let v = self.get(key).await?;
        Ok(v.and_then(|val| get_sub(&val, &path)))
    }

    #[napi(js_name = "setSub")]
    pub async fn set_sub(&self, key: String, path: Vec<String>, value: Value) -> Result<()> {
        let mut existing = self.get(key.clone()).await?.unwrap_or(Value::Null);
        set_sub(&mut existing, &path, value)?;
        self.set(key, existing).await
    }

    #[napi(js_name = "findKeys")]
    pub async fn find_keys(
        &self,
        key: String,
        not_key: Option<String>,
    ) -> Result<Vec<String>> {
        let backend = self.backend_arc().await?;
        // Flush so backend sees the latest writes.
        self.write_buffer.flush_now(&backend, &self.metrics).await?;
        if backend.supports_native_glob() {
            return Ok(backend.find_keys(&key, not_key.as_deref()).await?);
        }
        let pattern = compile_find_pattern(&key, not_key.as_deref());
        let all = backend.find_keys(&key, not_key.as_deref()).await?;
        Ok(all.into_iter().filter(|k| pattern.matches(k)).collect())
    }

    #[napi]
    pub fn metrics(&self) -> Metrics {
        self.metrics.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test(flavor = "multi_thread")]
    async fn full_round_trip_through_wrapper() {
        let db = Database::new(
            "_stub".into(),
            Settings::default(),
            Some(WrapperSettings {
                cache: Some(10),
                write_interval: Some(0),
                bulk_limit: None,
                json: None,
            }),
        );
        db.init().await.unwrap();
        db.set("a".into(), json!(1)).await.unwrap();
        assert_eq!(db.get("a".into()).await.unwrap(), Some(json!(1)));
        // setSub on a fresh key creates an object.
        db.set_sub("b".into(), vec!["x".into()], json!(2))
            .await
            .unwrap();
        assert_eq!(
            db.get_sub("b".into(), vec!["x".into()]).await.unwrap(),
            Some(json!(2))
        );
        db.remove("a".into()).await.unwrap();
        assert!(db.get("a".into()).await.unwrap().is_none());
        db.close().await.unwrap();
    }
}
