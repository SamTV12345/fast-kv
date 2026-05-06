use moka::future::Cache;
use serde_json::Value;
use std::sync::Arc;

/// LRU read-through cache. Disabled when capacity is 0.
pub struct ReadCache {
    inner: Option<Cache<String, Arc<Value>>>,
}

impl ReadCache {
    pub fn new(capacity: u64) -> Self {
        if capacity == 0 {
            Self { inner: None }
        } else {
            Self {
                inner: Some(Cache::builder().max_capacity(capacity).build()),
            }
        }
    }

    pub async fn get(&self, key: &str) -> Option<Arc<Value>> {
        match &self.inner {
            Some(c) => c.get(key).await,
            None => None,
        }
    }

    pub async fn put(&self, key: String, value: Arc<Value>) {
        if let Some(c) = &self.inner {
            c.insert(key, value).await;
        }
    }

    pub async fn invalidate(&self, key: &str) {
        if let Some(c) = &self.inner {
            c.invalidate(key).await;
        }
    }

    pub fn enabled(&self) -> bool {
        self.inner.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn round_trip() {
        let c = ReadCache::new(10);
        c.put("k".into(), Arc::new(json!(1))).await;
        assert_eq!(c.get("k").await.as_deref(), Some(&json!(1)));
        c.invalidate("k").await;
        assert!(c.get("k").await.is_none());
    }

    #[tokio::test]
    async fn disabled_when_capacity_zero() {
        let c = ReadCache::new(0);
        c.put("k".into(), Arc::new(json!(1))).await;
        assert!(c.get("k").await.is_none());
        assert!(!c.enabled());
    }
}
