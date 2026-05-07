use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use urlencoding::encode;

pub struct CouchBackend {
  base: String,
  db: String,
  client: Client,
  // Cache last-known _rev per key so updates / deletes don't need a
  // separate HEAD round-trip on every write.
  revs: Mutex<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
struct DocEnvelope {
  #[serde(default)]
  _rev: Option<String>,
  value: Value,
}

#[derive(Deserialize)]
struct AllDocsRow {
  id: String,
}
#[derive(Deserialize)]
struct AllDocsResponse {
  rows: Vec<AllDocsRow>,
}

impl CouchBackend {
  pub fn from_settings(settings: &Settings) -> Result<Self> {
    let host = settings.host.as_deref().unwrap_or("localhost");
    let port = settings.port.unwrap_or(5984);
    let auth = match (&settings.user, &settings.password) {
      (Some(u), Some(p)) => format!("{}:{}@", encode(u), encode(p)),
      _ => String::new(),
    };
    let base = format!("http://{auth}{host}:{port}");
    let db = settings
      .database
      .clone()
      .ok_or_else(|| UeberError::Config("couch: database required".into()))?;
    Ok(Self {
      base,
      db,
      client: Client::new(),
      revs: Mutex::new(Default::default()),
    })
  }

  fn doc_url(&self, key: &str) -> String {
    format!("{}/{}/{}", self.base, self.db, encode(key))
  }

  async fn ensure_database(&self) -> Result<()> {
    let url = format!("{}/{}", self.base, self.db);
    let resp = self
      .client
      .put(&url)
      .send()
      .await
      .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    // 201 created, 412 precondition failed (already exists) — both fine.
    match resp.status() {
      StatusCode::CREATED | StatusCode::PRECONDITION_FAILED => Ok(()),
      other => Err(UeberError::BackendInit(format!(
        "couch: unexpected create status {other}"
      ))),
    }
  }
}

#[async_trait]
impl Backend for CouchBackend {
  async fn init(&mut self) -> Result<()> {
    self.ensure_database().await
  }

  async fn close(&self) -> Result<()> {
    self.revs.lock().await.clear();
    Ok(())
  }

  async fn get(&self, key: &str) -> Result<Option<Value>> {
    let resp = self
      .client
      .get(self.doc_url(key))
      .send()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    match resp.status() {
      StatusCode::OK => {
        let env: DocEnvelope = resp
          .json()
          .await
          .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        if let Some(rev) = env._rev {
          self.revs.lock().await.insert(key.to_string(), rev);
        }
        Ok(Some(env.value))
      }
      StatusCode::NOT_FOUND => Ok(None),
      other => Err(UeberError::Backend(anyhow::anyhow!(
        "couch get: unexpected status {other}"
      ))),
    }
  }

  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    let mut body = json!({ "value": value });
    let cached_rev = self.revs.lock().await.get(key).cloned();
    if let Some(rev) = cached_rev {
      body
        .as_object_mut()
        .unwrap()
        .insert("_rev".into(), rev.into());
    }
    loop {
      let resp = self
        .client
        .put(self.doc_url(key))
        .json(&body)
        .send()
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      match resp.status() {
        StatusCode::CREATED | StatusCode::ACCEPTED => {
          #[derive(Deserialize)]
          struct PutOk {
            rev: String,
          }
          let r: PutOk = resp
            .json()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
          self.revs.lock().await.insert(key.to_string(), r.rev);
          return Ok(());
        }
        StatusCode::CONFLICT => {
          // _rev cache was stale — re-fetch the current rev and retry.
          let head = self
            .client
            .head(self.doc_url(key))
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
          if head.status() == StatusCode::NOT_FOUND {
            body.as_object_mut().unwrap().remove("_rev");
          } else {
            let etag = head
              .headers()
              .get(reqwest::header::ETAG)
              .ok_or_else(|| UeberError::Backend(anyhow::anyhow!("missing ETag on HEAD")))?
              .to_str()
              .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
              .trim_matches('"')
              .to_string();
            body
              .as_object_mut()
              .unwrap()
              .insert("_rev".into(), etag.into());
          }
          continue;
        }
        other => {
          return Err(UeberError::Backend(anyhow::anyhow!(
            "couch set: unexpected status {other}"
          )));
        }
      }
    }
  }

  async fn remove(&self, key: &str) -> Result<()> {
    let mut cached_rev = self.revs.lock().await.get(key).cloned();
    // Retry once on 409 (stale cached rev). A cluster's eventually-consistent
    // ETag header isn't reliable for the rev so this path always confirms
    // via GET on a stale-rev retry.
    for attempt in 0..2 {
      let rev = match (cached_rev.clone(), attempt) {
        (Some(r), 0) => r,
        _ => {
          // Authoritative rev lookup via GET — its body always carries
          // the current `_rev`, unlike HEAD's ETag which can lag in
          // clustered CouchDB.
          #[derive(Deserialize)]
          struct GetRev {
            _rev: String,
          }
          let resp = self
            .client
            .get(self.doc_url(key))
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
          match resp.status() {
            StatusCode::OK => {
              let g: GetRev = resp
                .json()
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
              g._rev
            }
            StatusCode::NOT_FOUND => {
              self.revs.lock().await.remove(key);
              return Ok(());
            }
            other => {
              return Err(UeberError::Backend(anyhow::anyhow!(
                "couch remove (lookup): unexpected status {other}"
              )));
            }
          }
        }
      };
      let resp = self
        .client
        .delete(self.doc_url(key))
        .query(&[("rev", &rev)])
        .send()
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
      match resp.status() {
        StatusCode::OK | StatusCode::ACCEPTED | StatusCode::NOT_FOUND => {
          self.revs.lock().await.remove(key);
          return Ok(());
        }
        StatusCode::CONFLICT if attempt == 0 => {
          cached_rev = None;
          continue;
        }
        other => {
          return Err(UeberError::Backend(anyhow::anyhow!(
            "couch remove: unexpected status {other}"
          )));
        }
      }
    }
    unreachable!()
  }

  async fn find_keys(&self, _key: &str, _not_key: Option<&str>) -> Result<Vec<String>> {
    // CouchDB's _all_docs returns every key; the wrapper filters with
    // compile_find_pattern (the Backend::find_keys wrapper does this for
    // backends that don't override supports_native_glob).
    let url = format!("{}/{}/_all_docs", self.base, self.db);
    let resp = self
      .client
      .get(&url)
      .send()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    if !resp.status().is_success() {
      return Err(UeberError::Backend(anyhow::anyhow!(
        "couch _all_docs: status {}",
        resp.status()
      )));
    }
    let body: AllDocsResponse = resp
      .json()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    Ok(
      body
        .rows
        .into_iter()
        .filter(|r| !r.id.starts_with("_design/"))
        .map(|r| r.id)
        .collect(),
    )
  }

  async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
    for op in ops {
      match op {
        BulkOp::Set { key, value } => self.set(key, value).await?,
        BulkOp::Remove { key } => self.remove(key).await?,
      }
    }
    Ok(())
  }

  fn default_wrapper_settings(&self) -> DefaultWrapperHints {
    DefaultWrapperHints {
      cache: Some(1000),
      write_interval: Some(100),
      json: Some(true),
    }
  }
}
