#![deny(clippy::all)]

mod dirty;
mod memory;
mod sqlite;
mod utils;

use redb::{Database, ReadableTable, TableDefinition};
use std::fs;

#[macro_use]
extern crate napi_derive;

#[napi(js_name = "KeyValueDB")]
pub struct KeyValueDB {
  filename: String,
  db: Option<Database>,
}

const TABLE: TableDefinition<String, String> = TableDefinition::new("store");

#[napi]
impl KeyValueDB {
  #[napi(constructor)]
  pub fn new(filename: String) -> napi::Result<Self> {
    Ok(KeyValueDB {
      filename: filename.clone(),
      db: Some(
        Database::create(&filename)
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?,
      ),
    })
  }

  #[napi]
  pub fn get(&self, key: String) -> napi::Result<Option<String>> {
    match &self.db {
      Some(db) => {
        let read_txn = db
          .begin_read()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        let table = read_txn.open_table(TABLE);
        if table.is_err() {
          return Ok(None);
        }

        let binding =
          table.map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        binding
          .get(key)
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
          .map(|v| v.map(|v| v.value().to_string()))
      }
      None => Err(napi::Error::new(
        napi::Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }
  #[napi]
  pub fn set(&self, key: String, value: String) -> napi::Result<()> {
    match &self.db {
      Some(db) => {
        let write_txn = db
          .begin_write()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        {
          let mut table = write_txn
            .open_table(TABLE)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
          table
            .insert(key, value)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        }
        write_txn
          .commit()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
      }
      None => Err(napi::Error::new(
        napi::Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }
  #[napi]
  pub fn remove(&self, key: String) -> napi::Result<()> {
    match &self.db {
      Some(db) => {
        let write_txn = db
          .begin_write()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        {
          let mut table = write_txn
            .open_table(TABLE)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
          table
            .remove(key)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        }
        write_txn
          .commit()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
      }
      None => Err(napi::Error::new(
        napi::Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  #[napi]
  pub fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
    match &self.db {
      Some(db) => {
        let regex = utils::update_regex(&key)
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        let mut found_keys = Vec::new();
        let read_txn = db
          .begin_read()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        let table = read_txn.open_table(TABLE);

        if table.is_err() {
          return Ok(vec![]);
        }

        let binding =
          table.map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        let iter = binding
          .iter()
          .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        iter.for_each(|x| {
          let res = x.unwrap();

          if let Some(not_key) = &not_key {
            let not_regex = utils::update_regex(not_key)
              .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
              .unwrap();

            if res.0.value() != *not_key
              && regex.is_match(&res.0.value().to_string())
              && !not_regex.is_match(&res.0.value().to_string())
            {
              found_keys.push(res.0.value().to_string());
            }
          } else if regex.is_match(&res.0.value().to_string()) {
            found_keys.push(res.0.value().to_string());
          }
        });
        Ok(found_keys)
      }
      None => Err(napi::Error::new(
        napi::Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  #[napi]
  pub fn close(&mut self) -> napi::Result<()> {
    if let Some(db) = self.db.take() {
      // Take ownership and drop the connection
      drop(db);
    }
    Ok(())
  }

  #[napi]
  pub fn destroy(&self) -> napi::Result<()> {
    fs::remove_file(&self.filename)
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
    Ok(())
  }
}
