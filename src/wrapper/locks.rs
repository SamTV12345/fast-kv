use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Per-key tokio mutex registry.
///
/// `get(key)` returns a shared `Arc<Mutex<()>>` keyed by string. Distinct keys
/// each get their own mutex, so they don't block each other; same-key callers
/// share the mutex and serialize.
#[derive(Default)]
pub struct KeyLocks {
    map: DashMap<String, Arc<Mutex<()>>>,
}

impl KeyLocks {
    pub fn get(&self, key: &str) -> Arc<Mutex<()>> {
        if let Some(m) = self.map.get(key) {
            return m.clone();
        }
        self.map
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_key_serializes() {
        let locks = KeyLocks::default();
        let m1 = locks.get("a");
        let _g = m1.lock().await;
        let m2 = locks.get("a");
        assert!(m2.try_lock().is_err());
    }

    #[tokio::test]
    async fn different_keys_independent() {
        let locks = KeyLocks::default();
        let m1 = locks.get("a");
        let _g = m1.lock().await;
        let m2 = locks.get("b");
        assert!(m2.try_lock().is_ok());
    }

    #[tokio::test]
    async fn repeated_get_returns_same_mutex() {
        let locks = KeyLocks::default();
        let m1 = locks.get("a");
        let m2 = locks.get("a");
        // Same Arc<Mutex>: locking one and trying the other must conflict.
        let _g = m1.lock().await;
        assert!(m2.try_lock().is_err());
    }
}
