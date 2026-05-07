use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

const TABLE: &str = "store";

pub struct SurrealBackend {
  sql_url: String,
  client: Client,
  namespace: String,
  database: String,
}

#[derive(Deserialize)]
struct StatementResult {
  #[serde(default)]
  status: String,
  #[serde(default)]
  result: Value,
}

impl SurrealBackend {
  pub fn from_settings(settings: &Settings) -> Result<Self> {
    // The plan's settings shape used `url` for the rpc url; we accept
    // either a fully-qualified `url` (used as-is) or host/port and
    // build `http://host:port/sql`.
    let raw = match (&settings.url, &settings.host) {
      (Some(u), _) => {
        // Most ueberDB configs point at /rpc, but the HTTP query
        // endpoint lives at /sql — rewrite if needed.
        if u.ends_with("/rpc") {
          format!("{}/sql", u.trim_end_matches("/rpc"))
        } else if u.ends_with("/sql") {
          u.clone()
        } else {
          format!("{}/sql", u.trim_end_matches('/'))
        }
      }
      (None, Some(h)) => {
        let port = settings.port.unwrap_or(8000);
        format!("http://{h}:{port}/sql")
      }
      _ => return Err(UeberError::Config("surrealdb: url or host required".into())),
    };
    Ok(Self {
      sql_url: raw,
      namespace: settings.database.clone().unwrap_or_else(|| "test".into()),
      database: settings.database.clone().unwrap_or_else(|| "test".into()),
      client: Client::new(),
    })
  }

  async fn run_sql(&self, sql: &str) -> Result<Vec<StatementResult>> {
    // SurrealDB v2 renamed the namespace/database routing headers.
    // The legacy `NS`/`DB` headers return "Specify a namespace to use".
    let mut req = self
      .client
      .post(&self.sql_url)
      .header("Accept", "application/json")
      .header("Surreal-NS", &self.namespace)
      .header("Surreal-DB", &self.database)
      .body(sql.to_string());
    // SurrealDB's default user is `root`/`root` when started with
    // `--user root --pass root`.
    req = req.basic_auth("root", Some("root"));
    let resp = req
      .send()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    if !resp.status().is_success() {
      let status = resp.status();
      let body = resp.text().await.unwrap_or_default();
      return Err(UeberError::Backend(anyhow::anyhow!(
        "surrealdb sql: status {status}: {body}"
      )));
    }
    resp
      .json::<Vec<StatementResult>>()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))
  }
}

/// Encode a key as a SurrealQL record id wrapped in backticks. Backticks
/// inside the key are escaped by doubling.
fn rid(key: &str) -> String {
  let escaped = key.replace('`', "``");
  format!("{TABLE}:`{escaped}`")
}

#[async_trait]
impl Backend for SurrealBackend {
  async fn init(&mut self) -> Result<()> {
    // No schema setup needed — SurrealDB tables are schemaless by default.
    // Sanity-check the connection so configuration errors fail fast.
    self.run_sql("INFO FOR DB;").await.map_err(|e| match e {
      UeberError::Backend(b) => UeberError::BackendInit(b.to_string()),
      other => other,
    })?;
    Ok(())
  }

  async fn close(&self) -> Result<()> {
    Ok(())
  }

  async fn get(&self, key: &str) -> Result<Option<Value>> {
    let sql = format!("SELECT val FROM {};", rid(key));
    let results = self.run_sql(&sql).await?;
    let first = results
      .into_iter()
      .next()
      .ok_or_else(|| UeberError::Backend(anyhow::anyhow!("no result")))?;
    // result is an array of records; pluck `value` from the first one.
    if let Value::Array(rows) = first.result {
      for row in rows {
        if let Value::Object(mut obj) = row
          && let Some(v) = obj.remove("val").or_else(|| obj.remove("value"))
        {
          return Ok(Some(v));
        }
      }
    }
    Ok(None)
  }

  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    // CONTENT replaces the record, matching ueberDB's "set is overwrite"
    // semantics.
    let json_str = serde_json::to_string(&json!({ "val": value }))?;
    // SurrealQL accepts JSON object literals as expressions, so we can
    // splice the encoded body directly after CONTENT.
    let sql = format!("UPSERT {} CONTENT {};", rid(key), json_str);
    self.run_sql(&sql).await?;
    Ok(())
  }

  async fn remove(&self, key: &str) -> Result<()> {
    let sql = format!("DELETE {};", rid(key));
    self.run_sql(&sql).await?;
    Ok(())
  }

  async fn find_keys(&self, key: &str, _not_key: Option<&str>) -> Result<Vec<String>> {
    // SurrealDB's record-id query language across v1/v2 is finicky for
    // arbitrary keys, so the wrapper-side glob filter does the work
    // here: pull every id from `store` and let `compile_find_pattern`
    // (applied in db.rs::find_keys when supports_native_glob is false)
    // do the matching.
    let _ = key;
    let sql = format!("SELECT id FROM {TABLE};");
    let results = self.run_sql(&sql).await?;
    let first = match results.into_iter().next() {
      Some(r) => r,
      None => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    if let Value::Array(rows) = first.result {
      for row in rows {
        if let Value::Object(obj) = row
          && let Some(Value::String(id)) = obj.get("id")
        {
          // id format is "store:`<key>`" or "store:<bare>".
          // Strip the table prefix and any wrapping backticks.
          if let Some(rest) = id.strip_prefix(&format!("{TABLE}:")) {
            let unwrapped = rest
              .strip_prefix('`')
              .and_then(|s| s.strip_suffix('`'))
              .unwrap_or(rest);
            out.push(unwrapped.replace("``", "`"));
          } else {
            out.push(id.clone());
          }
        }
      }
    }
    Ok(out)
  }

  async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
    if ops.is_empty() {
      return Ok(());
    }
    // Concatenate every op into a single SQL request — one HTTP round-
    // trip beats one-per-op for the coalesced wrapper flush.
    let mut sql = String::with_capacity(ops.len() * 64);
    for op in ops {
      match op {
        BulkOp::Set { key, value } => {
          let json_str = serde_json::to_string(&json!({ "val": value }))?;
          sql.push_str(&format!("UPSERT {} CONTENT {};\n", rid(key), json_str));
        }
        BulkOp::Remove { key } => {
          sql.push_str(&format!("DELETE {};\n", rid(key)));
        }
      }
    }
    self.run_sql(&sql).await?;
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
