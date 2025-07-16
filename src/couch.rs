use std::borrow::Cow;
use couch_rs::{Client, CouchDocument};
use couch_rs::document::TypedCouchDocument;
use couch_rs::error::CouchError;
use couch_rs::types::document::DocumentId;
use napi::{Error, Status};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::sqlite::SqliteErrorWrapper;

#[napi(js_name = "Couch")]
pub struct Couch {
    db: Option<Client>,
    database: Option<couch_rs::database::Database>,
    settings: CouchDBSettings,
}

#[napi(object)]
pub struct CouchDBSettings {
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub database: String,
}

// Define your own error wrapper
pub struct CouchDBErrorWrapper(CouchError);

impl From<CouchError> for CouchDBErrorWrapper {
    fn from(err: CouchError) -> Self {
        CouchDBErrorWrapper(err)
    }
}

impl From<CouchDBErrorWrapper> for Error {
    fn from(wrapper: CouchDBErrorWrapper) -> Self {
        Error::new(
            Status::GenericFailure,
            format!("Rusqlite error: {}", wrapper.0),
        )
    }
}

#[derive(Serialize, Deserialize, CouchDocument, Debug, Clone)]
pub struct StringDoc {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub _id: DocumentId,
    /// Document Revision, provided by `CouchDB`, helps negotiating conflicts
    #[serde(skip_serializing_if = "String::is_empty")]
    pub _rev: String,
    pub value: String,
}

//#[napi]
impl Couch {
    //#[napi(constructor)]
    pub fn new(settings: CouchDBSettings) -> napi::Result<Self> {
        let client = Client::new(&format!("http://{}:{}", settings.host, settings.port), &settings
            .user, &settings.password).map_err(CouchDBErrorWrapper::from)?;


        Ok(Couch {
            db: Some(client),
            settings,
            database: None,
        })
    }


    async fn init(&mut self) -> napi::Result<()> {
        if let Some(db) = &self.db {
            let db = db.db(&self.settings.database).await.map_err(CouchDBErrorWrapper::from)?;
            self.database = Some(db);
        }

        Ok(())
    }

    async fn get(&self, key: &str) -> napi::Result<Option<String>> {
        if let Some(db) = &self.db {
            match db.db(&self.settings.database).await {
                Ok(db) => {
                    let result: StringDoc = db.get(key).await.map_err
                    (CouchDBErrorWrapper::from)?;
                    Ok(Some(result.value))
                }
                Err(e) => Err(CouchDBErrorWrapper::from(e).into()),
            }
        } else {
            panic!("CouchDB client is not initialized");
        }
    }


    async fn find_keys(&self, key: &str, not_key: Option<String>) -> napi::Result<Vec<String>> {
        if let Some(db) = &self.database {
            let pfx_len = key.find("*");
            let pfx = match pfx_len {
                Some(len) => &key[..len],
                None => key,
            };
            // Regex ggf. dynamisch erzeugen
            let regex = self.create_find_regex(key, not_key.clone());

            let selector = if pfx_len.is_some() {
                json!({
                "_id": {
                    "$gte": pfx,
                    "$lte": format!("{}\u{fff0}", pfx),
                    "$regex": regex.as_str()
                }
            })
            } else {
                json!({
                "_id": pfx
            });

                let query = couch_rs::types::find::FindQuery::new(selector).fields(vec!["_id".to_string()]);
                let result = db.find::<serde_json::Value>(&query).await.map_err(CouchDBErrorWrapper::from)?;

                let ids = result.rows::<StringDoc>
                    .iter()
                    .filter_map(|doc| doc.get("_id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect();
            };
        } else {
            panic!("CouchDB client is not initialized");
        }
    }
}