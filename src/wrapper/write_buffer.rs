use crate::backends::{Backend, BulkOp};
use crate::error::Result;
use crate::wrapper::metrics::MetricsCore;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
enum BufferedOp {
    Set(Value),
    Remove,
}

pub struct WriteBuffer {
    pub(crate) pending: Arc<DashMap<String, BufferedOp>>,
    notify: Arc<Notify>,
    bulk_limit: usize,
    interval_ms: u64,
    enabled: bool,
}

impl WriteBuffer {
    pub fn new(interval_ms: u64, bulk_limit: usize) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            notify: Arc::new(Notify::new()),
            bulk_limit,
            interval_ms,
            enabled: interval_ms > 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns Some(value) if a Set is queued, Some(None) if a Remove is queued,
    /// or None if no entry is buffered for that key.
    pub fn buffered_get(&self, key: &str) -> Option<Option<Value>> {
        self.pending.get(key).map(|op| match op.value() {
            BufferedOp::Set(v) => Some(v.clone()),
            BufferedOp::Remove => None,
        })
    }

    pub fn enqueue_set(&self, key: String, value: Value) {
        self.pending.insert(key, BufferedOp::Set(value));
        self.notify.notify_one();
    }

    pub fn enqueue_remove(&self, key: String) {
        self.pending.insert(key, BufferedOp::Remove);
        self.notify.notify_one();
    }

    /// Drain up to `bulk_limit` ops from the pending map.
    fn drain_batch(&self) -> Vec<BulkOp> {
        let mut out = Vec::with_capacity(self.pending.len().min(self.bulk_limit));
        let keys: Vec<String> = self.pending.iter().map(|e| e.key().clone()).collect();
        for k in keys.into_iter().take(self.bulk_limit) {
            if let Some((key, op)) = self.pending.remove(&k) {
                out.push(match op {
                    BufferedOp::Set(v) => BulkOp::Set { key, value: v },
                    BufferedOp::Remove => BulkOp::Remove { key },
                });
            }
        }
        out
    }

    /// Spawn the background flush task. Returns the cancellation handle.
    pub fn spawn_flush_task(
        self: Arc<Self>,
        backend: Arc<dyn Backend>,
        metrics: Arc<MetricsCore>,
    ) -> CancellationToken {
        let token = CancellationToken::new();
        if !self.enabled {
            return token;
        }

        let token_clone = token.clone();
        let interval = Duration::from_millis(self.interval_ms);
        let this = self.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        Self::drain_and_flush(&this, &backend, &metrics).await;
                        break;
                    }
                    _ = ticker.tick() => {
                        Self::drain_and_flush(&this, &backend, &metrics).await;
                    }
                    _ = this.notify.notified() => {
                        if this.pending.len() >= this.bulk_limit {
                            Self::drain_and_flush(&this, &backend, &metrics).await;
                        }
                    }
                }
            }
        });

        token
    }

    async fn drain_and_flush(
        this: &Arc<Self>,
        backend: &Arc<dyn Backend>,
        metrics: &Arc<MetricsCore>,
    ) {
        loop {
            let batch = this.drain_batch();
            if batch.is_empty() {
                break;
            }
            metrics.inc(&metrics.bulks);
            metrics.inc(&metrics.flushes);
            if backend.do_bulk(&batch).await.is_err() {
                break;
            }
            if this.pending.is_empty() {
                break;
            }
        }
    }

    /// Drain the buffer to the backend synchronously. Used by `flush()` and `close()`.
    pub async fn flush_now(
        &self,
        backend: &Arc<dyn Backend>,
        metrics: &Arc<MetricsCore>,
    ) -> Result<()> {
        loop {
            let batch = self.drain_batch();
            if batch.is_empty() {
                return Ok(());
            }
            metrics.inc(&metrics.bulks);
            metrics.inc(&metrics.flushes);
            backend.do_bulk(&batch).await?;
            if self.pending.is_empty() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::test_stub::StubBackend;
    use serde_json::json;

    #[tokio::test]
    async fn coalesces_repeated_sets_for_same_key() {
        let wb = WriteBuffer::new(0, 100);
        wb.enqueue_set("k".into(), json!(1));
        wb.enqueue_set("k".into(), json!(2));
        let batch = wb.drain_batch();
        assert_eq!(batch.len(), 1);
        match &batch[0] {
            BulkOp::Set { key, value } => {
                assert_eq!(key, "k");
                assert_eq!(value, &json!(2));
            }
            _ => panic!("expected Set"),
        }
    }

    #[tokio::test]
    async fn flush_now_writes_pending_to_backend() {
        let wb = WriteBuffer::new(0, 100);
        let backend: Arc<dyn Backend> = Arc::new(StubBackend::default());
        let metrics = Arc::new(MetricsCore::default());

        wb.enqueue_set("a".into(), json!(1));
        wb.enqueue_set("b".into(), json!(2));
        wb.flush_now(&backend, &metrics).await.unwrap();

        assert!(metrics.snapshot().bulks >= 1);
        // Pending must be empty after flush.
        assert_eq!(wb.pending.len(), 0);
    }

    #[tokio::test]
    async fn enabled_only_when_interval_positive() {
        assert!(!WriteBuffer::new(0, 100).enabled());
        assert!(WriteBuffer::new(1, 100).enabled());
    }
}
