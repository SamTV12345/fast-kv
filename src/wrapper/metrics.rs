use napi_derive::napi;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct MetricsCore {
  pub reads: AtomicU64,
  pub writes: AtomicU64,
  pub removes: AtomicU64,
  pub cache_hits: AtomicU64,
  pub cache_misses: AtomicU64,
  pub flushes: AtomicU64,
  pub bulks: AtomicU64,
}

#[napi(object)]
pub struct Metrics {
  pub reads: u32,
  pub writes: u32,
  pub removes: u32,
  pub cache_hits: u32,
  pub cache_misses: u32,
  pub flushes: u32,
  pub bulks: u32,
}

impl MetricsCore {
  pub fn snapshot(&self) -> Metrics {
    Metrics {
      reads: self.reads.load(Ordering::Relaxed) as u32,
      writes: self.writes.load(Ordering::Relaxed) as u32,
      removes: self.removes.load(Ordering::Relaxed) as u32,
      cache_hits: self.cache_hits.load(Ordering::Relaxed) as u32,
      cache_misses: self.cache_misses.load(Ordering::Relaxed) as u32,
      flushes: self.flushes.load(Ordering::Relaxed) as u32,
      bulks: self.bulks.load(Ordering::Relaxed) as u32,
    }
  }

  pub fn inc(&self, c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn counters_increment() {
    let m = MetricsCore::default();
    m.inc(&m.reads);
    m.inc(&m.reads);
    m.inc(&m.cache_hits);
    let snap = m.snapshot();
    assert_eq!(snap.reads, 2);
    assert_eq!(snap.cache_hits, 1);
    assert_eq!(snap.writes, 0);
  }
}
