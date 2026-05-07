use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use urlencoding::encode;

pub struct ElasticsearchBackend {
    base: String,
    index: String,
    client: Client,
}

#[derive(Deserialize)]
struct GetResponse {
    found: bool,
    #[serde(default)]
    _source: Option<Source>,
}

#[derive(Deserialize)]
struct Source {
    value: Value,
}

#[derive(Deserialize)]
struct SearchHit {
    _id: String,
}
#[derive(Deserialize)]
struct SearchHits {
    hits: Vec<SearchHit>,
}
#[derive(Deserialize)]
struct SearchResponse {
    hits: SearchHits,
}

impl ElasticsearchBackend {
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let host = settings.host.as_deref().unwrap_or("localhost");
        let port = settings.port.unwrap_or(9200);
        let base = format!("http://{host}:{port}");
        let index = settings
            .base_index
            .clone()
            .unwrap_or_else(|| "ueberdb".into());
        Ok(Self {
            base,
            index,
            client: Client::new(),
        })
    }

    fn doc_url(&self, key: &str) -> String {
        format!(
            "{}/{}/_doc/{}?refresh=true",
            self.base,
            self.index,
            encode(key)
        )
    }
}

#[async_trait]
impl Backend for ElasticsearchBackend {
    async fn init(&mut self) -> Result<()> {
        // Explicit mapping:
        // - `key` is a keyword we can run wildcard queries against (ES rejects
        //   wildcard on `_id` directly).
        // - `value` has `enabled: false` so ES stores the JSON in _source but
        //   doesn't index it — this dodges the "tried to parse field X as
        //   object, but found a concrete value" mapping conflict that hits
        //   when one test sets `value: {a:1}` and the next sets `value: "s"`.
        let mapping = json!({
            "mappings": {
                "properties": {
                    "key": { "type": "keyword" },
                    "value": { "type": "object", "enabled": false }
                }
            }
        });
        let resp = self
            .client
            .put(format!("{}/{}", self.base, self.index))
            .json(&mapping)
            .send()
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        match resp.status() {
            StatusCode::OK | StatusCode::CREATED => Ok(()),
            // 400 + resource_already_exists_exception is fine for a re-init.
            StatusCode::BAD_REQUEST => Ok(()),
            other => Err(UeberError::BackendInit(format!(
                "elasticsearch index create: status {other}"
            ))),
        }
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        let url = format!("{}/{}/_doc/{}", self.base, self.index, encode(key));
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        match resp.status() {
            StatusCode::OK => {
                let body: GetResponse = resp
                    .json()
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
                if body.found {
                    Ok(body._source.map(|s| s.value))
                } else {
                    Ok(None)
                }
            }
            StatusCode::NOT_FOUND => Ok(None),
            other => Err(UeberError::Backend(anyhow::anyhow!(
                "elasticsearch get: status {other}"
            ))),
        }
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        let resp = self
            .client
            .put(self.doc_url(key))
            .json(&json!({ "key": key, "value": value }))
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(UeberError::Backend(anyhow::anyhow!(
                "elasticsearch set: status {status}: {body}"
            )));
        }
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.doc_url(key))
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        match resp.status() {
            s if s.is_success() => Ok(()),
            StatusCode::NOT_FOUND => Ok(()),
            other => Err(UeberError::Backend(anyhow::anyhow!(
                "elasticsearch remove: status {other}"
            ))),
        }
    }

    async fn find_keys(&self, key: &str, _not_key: Option<&str>) -> Result<Vec<String>> {
        // Wildcard query against the keyword `key` field — ES rejects
        // wildcard on the meta `_id` field. The wrapper layer applies the
        // full `*` glob + notKey filter on top, so passing the raw glob
        // through to ES is enough.
        let url = format!("{}/{}/_search?size=10000", self.base, self.index);
        let resp = self
            .client
            .post(&url)
            .json(&json!({
                "query": { "wildcard": { "key": key } }
            }))
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(UeberError::Backend(anyhow::anyhow!(
                "elasticsearch search: status {status}: {body}"
            )));
        }
        let body: SearchResponse = resp
            .json()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(body.hits.hits.into_iter().map(|h| h._id).collect())
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
        if ops.is_empty() {
            return Ok(());
        }
        let mut body = String::with_capacity(ops.len() * 80);
        for op in ops {
            match op {
                BulkOp::Set { key, value } => {
                    body.push_str(&serde_json::to_string(&json!({
                        "index": { "_index": self.index, "_id": key }
                    }))?);
                    body.push('\n');
                    body.push_str(&serde_json::to_string(
                        &json!({ "key": key, "value": value }),
                    )?);
                    body.push('\n');
                }
                BulkOp::Remove { key } => {
                    body.push_str(&serde_json::to_string(&json!({
                        "delete": { "_index": self.index, "_id": key }
                    }))?);
                    body.push('\n');
                }
            }
        }
        let resp = self
            .client
            .post(format!("{}/_bulk?refresh=true", self.base))
            .header("Content-Type", "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(UeberError::Backend(anyhow::anyhow!(
                "elasticsearch _bulk: status {status}: {text}"
            )));
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
