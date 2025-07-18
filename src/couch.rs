use crate::general::BulkObject;
use crate::utils::create_find_regex;
use couch_rs::document::TypedCouchDocument;
use couch_rs::error::CouchError;
use couch_rs::types::document::DocumentId;
use couch_rs::{Client, CouchDocument};
use napi::{Error, Status};
use serde::{Deserialize, Serialize};
use serde_json::json;

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
      format!("Couch error: {}", wrapper.0),
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
  #[serde(skip_serializing_if = "Option::is_none")]
  pub _deleted: Option<bool>,
  pub value: String,
}

#[napi]
impl Couch {
  #[napi(constructor)]
  pub fn new(settings: CouchDBSettings) -> napi::Result<Self> {
    let client = Client::new(
      &format!("http://{}:{}", settings.host, settings.port),
      &settings.user,
      &settings.password,
    )
    .map_err(CouchDBErrorWrapper::from)?;

    Ok(Couch {
      db: Some(client),
      settings,
      database: None,
    })
  }

  #[napi]
  pub async unsafe fn init(&mut self) -> napi::Result<()> {
    if let Some(db) = &self.db {
      let db = db
        .db(&self.settings.database)
        .await
        .map_err(CouchDBErrorWrapper::from)?;
      self.database = Some(db);
    } else {
      return Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ));
    }

    Ok(())
  }
  #[napi]
  pub async unsafe fn get(&self, key: String) -> napi::Result<Option<String>> {
    if let Some(db) = &self.db {
      match db.db(&self.settings.database).await {
        Ok(db) => {
          let result = db
            .get::<StringDoc>(&key)
            .await
            .map_err(CouchDBErrorWrapper::from);
          match result {
            Ok(result) => {
              return Ok(Some(result.value));
            }
            Err(e) => {
              match e {
                CouchDBErrorWrapper(couch_rs::error::CouchError::OperationFailed(
                  ref op_failed,
                )) => {
                  if op_failed.status == 404 {
                    return Ok(None); // Document not found
                  } else {
                    return Err(CouchDBErrorWrapper::from(e).into());
                  }
                }
                _ => {
                  return Err(CouchDBErrorWrapper::from(e).into());
                }
              }
            }
          }
        }
        Err(e) => Err(CouchDBErrorWrapper::from(e).into()),
      }
    } else {
      Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ))
    }
  }
  #[napi]
  pub async fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
    if let Some(db) = &self.database {
      let pfx_len = key.find("*");
      let pfx = match pfx_len {
        Some(len) => &key[..len],
        None => &key,
      };
      // Regex ggf. dynamisch erzeugen
      let regex = create_find_regex(&key, not_key.clone());

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
        })
      };

      let query = couch_rs::types::find::FindQuery::new(selector).fields(vec!["_id".to_string()]);
      let result = db
        .find::<serde_json::Value>(&query)
        .await
        .map_err(CouchDBErrorWrapper::from)?;

      let ids = result
        .rows
        .iter()
        .filter_map(|doc| {
          doc
            .get("_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        })
        .collect();
      Ok(ids)
    } else {
      Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ))
    }
  }
  #[napi]
  pub async fn set(&self, key: String, value: String) -> napi::Result<()> {
    if let Some(db) = &self.database {
      let mut doc = StringDoc {
        _id: DocumentId::from(key.clone()),
        _rev: String::new(),
        value,
        _deleted: None,
      };
      db.upsert(&mut doc)
        .await
        .map_err(CouchDBErrorWrapper::from)?;
      Ok(())
    } else {
      Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ))
    }
  }
  #[napi]
  pub async fn do_bulk(&self, bulk: Vec<BulkObject>) -> napi::Result<()> {
    if let Some(db) = &self.database {
      let document_ids: Vec<DocumentId> = bulk
        .iter()
        .map(|item| DocumentId::from(item.key.clone()))
        .collect();
      let documents = db
        .get_bulk::<StringDoc>(document_ids)
        .await
        .map_err(CouchDBErrorWrapper::from)?;

      let mut bulk_documents: Vec<StringDoc> = vec![];

      for (item, doc) in bulk.iter().zip(documents.rows.iter()) {
        match item.r#type.as_str() {
          "set" => {
            let mut doc_to_update = doc.clone();
            doc_to_update.value = item.value.clone().map_or("".to_string(), |v| v);
            bulk_documents.push(doc_to_update);
          }
          "remove" => {
            let mut doc_to_update = doc.clone();
            doc_to_update._deleted = Some(true);
            bulk_documents.push(doc_to_update);
          }
          _ => {
            return Err(Error::new(
              Status::GenericFailure,
              format!("Unknown action: {}", item.r#type),
            ));
          }
        }
      }

      db.bulk_docs(&mut *bulk_documents)
        .await
        .map_err(CouchDBErrorWrapper::from)?;

      Ok(())
    } else {
      Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ))
    }
  }

  #[napi]
  pub async fn remove(&self, key: String) -> napi::Result<()> {
    if let Some(db) = &self.database {
      let document = db
        .get::<StringDoc>(key.as_str())
        .await
        .map_err(CouchDBErrorWrapper::from)?;
      db.remove(&document)
        .await
        .map_err(CouchDBErrorWrapper::from)?;
      Ok(())
    } else {
      Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ))
    }
  }

  #[napi]
  pub fn close(&mut self) -> napi::Result<()> {
    self.db = None;
    self.database = None;
    Ok(())
  }

  #[napi]
  pub async fn destroy(&self) -> napi::Result<()> {
    if let Some(db) = &self.db {
      db.destroy_db(&self.settings.database)
        .await
        .map_err(CouchDBErrorWrapper::from)?;
      Ok(())
    } else {
      return Err(napi::Error::from_reason(
        "CouchDB client is not initialized",
      ));
    }
  }
}
