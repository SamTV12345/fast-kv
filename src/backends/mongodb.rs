use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use futures::stream::TryStreamExt;
use mongodb::bson::{doc, to_bson, Bson, Document, Regex};
use mongodb::options::ClientOptions;
use mongodb::{Client, Collection, Database as MongoDatabase};
use serde_json::Value;
use tokio::sync::Mutex;

const COLLECTION: &str = "store";

pub struct MongoBackend {
    url: String,
    db_name: String,
    state: Mutex<Option<State>>,
}

struct State {
    _client: Client,
    db: MongoDatabase,
}

impl MongoBackend {
    pub fn from_settings(settings: &Settings) -> Result<Self> {
        let url = settings
            .url
            .clone()
            .ok_or_else(|| UeberError::Config("mongodb: url required".into()))?;
        let db_name = settings
            .database
            .clone()
            .or_else(|| settings.db_name.clone())
            .ok_or_else(|| UeberError::Config("mongodb: database required".into()))?;
        Ok(Self {
            url,
            db_name,
            state: Mutex::new(None),
        })
    }

    async fn collection(&self) -> Result<Collection<Document>> {
        let g = self.state.lock().await;
        let s = g.as_ref().ok_or(UeberError::NotInitialized)?;
        Ok(s.db.collection::<Document>(COLLECTION))
    }
}

fn glob_to_regex(glob: &str) -> Regex {
    let mut pattern = String::from("^");
    for ch in glob.chars() {
        match ch {
            '*' => pattern.push_str(".*"),
            // Escape regex metacharacters that aren't '*'.
            '.' | '+' | '?' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                pattern.push('\\');
                pattern.push(ch);
            }
            other => pattern.push(other),
        }
    }
    pattern.push('$');
    Regex {
        pattern,
        options: String::new(),
    }
}

#[async_trait]
impl Backend for MongoBackend {
    async fn init(&mut self) -> Result<()> {
        let opts = ClientOptions::parse(&self.url)
            .await
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        let client = Client::with_options(opts)
            .map_err(|e| UeberError::BackendInit(e.to_string()))?;
        let db = client.database(&self.db_name);
        *self.state.lock().await = Some(State {
            _client: client,
            db,
        });
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        *self.state.lock().await = None;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        let coll = self.collection().await?;
        let doc = coll
            .find_one(doc! { "_id": key })
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        match doc {
            Some(mut d) => {
                let v = d.remove("value").unwrap_or(Bson::Null);
                let json: Value =
                    serde_json::to_value(v).map_err(UeberError::Serde)?;
                Ok(Some(json))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        let coll = self.collection().await?;
        let bson = to_bson(value).map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        coll.update_one(
            doc! { "_id": key },
            doc! { "$set": { "value": bson } },
        )
        .upsert(true)
        .await
        .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let coll = self.collection().await?;
        coll.delete_one(doc! { "_id": key })
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        Ok(())
    }

    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
        let coll = self.collection().await?;
        let filter = match not_key {
            Some(nk) => doc! {
                "_id": { "$regex": glob_to_regex(key), "$not": glob_to_regex(nk) }
            },
            None => doc! { "_id": { "$regex": glob_to_regex(key) } },
        };
        let mut cursor = coll
            .find(filter)
            .projection(doc! { "_id": 1 })
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
        let mut out = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
        {
            if let Some(Bson::String(s)) = d.get("_id") {
                out.push(s.clone());
            }
        }
        Ok(out)
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
        // The mongodb 3.x bulk_write API isn't stable for the &str ToString
        // convenience we need; sequential update_one/delete_one keeps the
        // implementation small and is still inside a single TCP connection
        // via the connection pool.
        for op in ops {
            match op {
                BulkOp::Set { key, value } => self.set(key, value).await?,
                BulkOp::Remove { key } => self.remove(key).await?,
            }
        }
        Ok(())
    }

    fn supports_native_glob(&self) -> bool {
        true
    }

    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        DefaultWrapperHints {
            cache: Some(1000),
            write_interval: Some(100),
            json: Some(true),
        }
    }
}
