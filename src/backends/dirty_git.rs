use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::backends::dirty::DirtyBackend;
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use git2::{IndexAddOption, Repository, Signature};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex as AsyncMutex;
use tokio::task;

pub struct DirtyGitBackend {
  inner: DirtyBackend,
  repo_dir: Arc<PathBuf>,
  file_name: String,
  // libgit2 isn't async-safe and the index lock isn't reentrant. Hold
  // a single Repository for this backend's lifetime, behind a sync
  // mutex that we lock from inside spawn_blocking. The async wrapper
  // mutex serializes the spawn_blocking calls themselves so two
  // commits never sit on the blocking pool at the same time.
  repo: Arc<StdMutex<Option<Repository>>>,
  commit_lock: Arc<AsyncMutex<()>>,
}

impl DirtyGitBackend {
  pub fn from_settings(settings: &Settings) -> Result<Self> {
    let inner = DirtyBackend::from_settings(settings)?;
    let filename = settings
      .filename
      .as_ref()
      .ok_or_else(|| UeberError::Config("dirty_git: filename required".into()))?;
    let path = PathBuf::from(filename);
    let repo_dir = path
      .parent()
      .map(PathBuf::from)
      .unwrap_or_else(|| PathBuf::from("."));
    let file_name = path
      .file_name()
      .ok_or_else(|| UeberError::Config("dirty_git: filename has no basename".into()))?
      .to_string_lossy()
      .into_owned();
    Ok(Self {
      inner,
      repo_dir: Arc::new(repo_dir),
      file_name,
      repo: Arc::new(StdMutex::new(None)),
      commit_lock: Arc::new(AsyncMutex::new(())),
    })
  }

  async fn commit(&self) -> Result<()> {
    // Async serialization so spawn_blocking calls don't pile up on the
    // blocking thread pool — keeps the libgit2 work strictly sequential.
    let _async_g = self.commit_lock.lock().await;
    let repo_arc = self.repo.clone();
    let file = self.file_name.clone();
    task::spawn_blocking(move || -> Result<()> {
      let mut g = repo_arc.lock().expect("repo mutex poisoned");
      let repo = g.as_mut().ok_or(UeberError::NotInitialized)?;
      let mut index = repo
        .index()
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      index
        .add_all([file.as_str()].iter(), IndexAddOption::DEFAULT, None)
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      index
        .write()
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      let tree_oid = index
        .write_tree()
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      let tree = repo
        .find_tree(tree_oid)
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      let sig = Signature::now("ueberdb", "ueberdb@local")
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
      let parents: Vec<&git2::Commit> = parent.iter().collect();
      repo
        .commit(Some("HEAD"), &sig, &sig, "ueberdb", &tree, &parents)
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      Ok(())
    })
    .await
    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))??;
    Ok(())
  }
}

#[async_trait]
impl Backend for DirtyGitBackend {
  async fn init(&mut self) -> Result<()> {
    self.inner.init().await?;
    let dir = self.repo_dir.clone();
    let repo = task::spawn_blocking(move || -> Result<Repository> {
      Repository::open(&*dir)
        .or_else(|_| Repository::init(&*dir))
        .map_err(|e| UeberError::BackendInit(e.to_string()))
    })
    .await
    .map_err(|e| UeberError::BackendInit(e.to_string()))??;
    *self.repo.lock().expect("repo mutex poisoned") = Some(repo);
    Ok(())
  }

  async fn close(&self) -> Result<()> {
    self.inner.close().await?;
    // Drop the Repository handle so libgit2 releases its file handles.
    *self.repo.lock().expect("repo mutex poisoned") = None;
    Ok(())
  }

  async fn get(&self, key: &str) -> Result<Option<Value>> {
    self.inner.get(key).await
  }

  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    // Serialize the file write with the git commit so libgit2 doesn't
    // catch the dirty file being rewritten mid-`add_all` — that surfaces
    // as "file changed before we could read it" under concurrent writes.
    let _g = self.commit_lock.lock().await;
    self.inner.set(key, value).await?;
    drop(_g);
    self.commit().await
  }

  async fn remove(&self, key: &str) -> Result<()> {
    let _g = self.commit_lock.lock().await;
    self.inner.remove(key).await?;
    drop(_g);
    self.commit().await
  }

  async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
    self.inner.find_keys(key, not_key).await
  }

  async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
    self.inner.do_bulk(ops).await?;
    // One commit per bulk batch — keeps git history readable instead
    // of a commit per inner op.
    self.commit().await
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
  use tempfile::tempdir;

  #[tokio::test]
  async fn round_trip_writes_git_commits() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("git.db").to_string_lossy().into_owned();
    let s = Settings {
      filename: Some(p),
      ..Default::default()
    };
    let mut b = DirtyGitBackend::from_settings(&s).unwrap();
    b.init().await.unwrap();
    b.set("k", &json!(1)).await.unwrap();
    b.set("k", &json!(2)).await.unwrap();
    b.remove("k").await.unwrap();

    // Two sets + one remove == three commits (plus DirtyBackend's
    // empty init line if any).
    let repo = Repository::open(dir.path()).unwrap();
    let head = repo.head().unwrap();
    let mut count = 0usize;
    let mut commit = head.peel_to_commit().unwrap();
    loop {
      count += 1;
      if commit.parent_count() == 0 {
        break;
      }
      commit = commit.parent(0).unwrap();
    }
    assert!(count >= 3, "expected ≥3 commits, got {count}");
  }
}
