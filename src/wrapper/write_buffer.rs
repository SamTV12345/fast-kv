use crate::backends::{Backend, BulkOp};
use crate::error::Result;
use crate::wrapper::metrics::MetricsCore;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex as AsyncMutex, Notify};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
enum BufferedOp {
    Set(Value),
    Remove,
}

pub struct WriteBuffer {
    pub(crate) pending: Arc<DashMap<String, BufferedOp>>,
    notify: Arc<Notify>,
    // Serializes concurrent flushers so the background ticker and an
    // explicit `flush_now()` (e.g. from `Database::close`) never race on
    // the same key. Backends with rev-tracking (CouchDB) blew up when
    // both ran do_bulk for the same key at the same time.
    flush_gate: Arc<AsyncMutex<()>>,
    bulk_limit: usize,
    interval_ms: u64,
    enabled: bool,
}

impl WriteBuffer {
    pub fn new(interval_ms: u64, bulk_limit: usize) -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
            notify: Arc::new(Notify::new()),
            flush_gate: Arc::new(AsyncMutex::new(())),
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

    /// Snapshot up to `bulk_limit` ops from the pending map without removing them.
    /// Entries are removed by `commit_flushed` only after `do_bulk` succeeds —
    /// otherwise concurrent `buffered_get` could see a key in neither the buffer
    /// nor the backend during the do_bulk window.
    fn snapshot_batch(&self) -> Vec<BulkOp> {
        self.pending
            .iter()
            .take(self.bulk_limit)
            .map(|e| match e.value() {
                BufferedOp::Set(v) => BulkOp::Set {
                    key: e.key().clone(),
                    value: v.clone(),
                },
                BufferedOp::Remove => BulkOp::Remove {
                    key: e.key().clone(),
                },
            })
            .collect()
    }

    /// Remove entries from `pending` whose value matches what was just flushed.
    /// If a concurrent `enqueue_*` re-wrote the key after the snapshot, the
    /// values won't match and we leave it for the next flush.
    fn commit_flushed(&self, batch: &[BulkOp]) {
        for op in batch {
            match op {
                BulkOp::Set { key, value } => {
                    self.pending.remove_if(key, |_, current| match current {
                        BufferedOp::Set(v) => v == value,
                        BufferedOp::Remove => false,
                    });
                }
                BulkOp::Remove { key } => {
                    self.pending.remove_if(key, |_, current| {
                        matches!(current, BufferedOp::Remove)
                    });
                }
            }
        }
    }

    /// Spawn the background flush task. Returns the cancellation handle and
    /// a `JoinHandle` so `close()` can await full shutdown — otherwise the
    /// final post-cancel drain can outlive `Database::close` and race against
    /// the next test's backend on shared on-disk state (e.g. dirty_git's
    /// .git directory).
    pub fn spawn_flush_task(
        self: Arc<Self>,
        backend: Arc<dyn Backend>,
        metrics: Arc<MetricsCore>,
    ) -> (CancellationToken, tokio::task::JoinHandle<()>) {
        let token = CancellationToken::new();
        if !self.enabled {
            // Spawn a no-op task so the JoinHandle type is consistent.
            let h = tokio::spawn(async {});
            return (token, h);
        }

        let token_clone = token.clone();
        let interval = Duration::from_millis(self.interval_ms);
        let this = self.clone();

        let handle = tokio::spawn(async move {
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

        (token, handle)
    }

    async fn drain_and_flush(
        this: &Arc<Self>,
        backend: &Arc<dyn Backend>,
        metrics: &Arc<MetricsCore>,
    ) {
        let _g = this.flush_gate.lock().await;
        loop {
            let batch = this.snapshot_batch();
            if batch.is_empty() {
                break;
            }
            metrics.inc(&metrics.bulks);
            metrics.inc(&metrics.flushes);
            if backend.do_bulk(&batch).await.is_err() {
                break;
            }
            this.commit_flushed(&batch);
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
        let _g = self.flush_gate.lock().await;
        loop {
            let batch = self.snapshot_batch();
            if batch.is_empty() {
                return Ok(());
            }
            metrics.inc(&metrics.bulks);
            metrics.inc(&metrics.flushes);
            backend.do_bulk(&batch).await?;
            self.commit_flushed(&batch);
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
        let batch = wb.snapshot_batch();
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
    async fn buffered_get_visible_until_commit_flushed() {
        // Regression: previously `drain_batch` removed pending entries before
        // do_bulk completed, so a concurrent `buffered_get` could see neither
        // the buffer nor the backend.
        let wb = WriteBuffer::new(0, 100);
        wb.enqueue_set("k".into(), json!(1));
        let snapshot = wb.snapshot_batch();
        // Snapshot taken — value still visible to readers.
        assert_eq!(wb.buffered_get("k"), Some(Some(json!(1))));
        wb.commit_flushed(&snapshot);
        assert!(wb.buffered_get("k").is_none());
    }

    #[tokio::test]
    async fn commit_flushed_keeps_concurrently_rewritten_entry() {
        let wb = WriteBuffer::new(0, 100);
        wb.enqueue_set("k".into(), json!(1));
        let snapshot = wb.snapshot_batch();
        // Simulate a concurrent set after the snapshot but before commit.
        wb.enqueue_set("k".into(), json!(2));
        wb.commit_flushed(&snapshot);
        // The newer write must survive the commit.
        assert_eq!(wb.buffered_get("k"), Some(Some(json!(2))));
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
