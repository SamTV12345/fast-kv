#![deny(clippy::all)]

use redb::{Database, ReadableTable, TableDefinition};
use std::fs;

#[macro_use]
extern crate napi_derive;

fn get_first_key(val: &str) -> String {
  val.split(":").next().unwrap().to_string()
}

#[napi(js_name = "KeyValueDB")]
pub struct KeyValueDB {
  filename: String,
  db: Database,
}

#[napi]
impl KeyValueDB {
  #[napi(constructor)]
  pub fn new(filename: String) -> Self {
    KeyValueDB {
      filename: filename.clone(),
      db: Database::create(&filename).unwrap(),
    }
  }

  #[napi]
  pub fn get(&self, key: String) -> napi::Result<Option<String>> {
    let read_txn = self.db.begin_read().unwrap();
    let table = get_first_key(&key);
    let table: TableDefinition<String, String> = TableDefinition::new(&table);
    let table = read_txn
      .open_table(table)
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
    table
      .get(key)
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
      .map(|v| v.map(|v| v.value().to_string()))
  }
  #[napi]
  pub fn set(&self, key: String, value: String) -> napi::Result<()> {
    let write_txn = self.db.begin_write().unwrap();
    let table = get_first_key(&key);
    let table: TableDefinition<String, String> = TableDefinition::new(&table);
    {
      let mut table = write_txn
        .open_table(table)
        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
      table
        .insert(key, value)
        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
    }
    write_txn
      .commit()
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
  }
  #[napi]
  pub fn remove(&self, key: String) -> napi::Result<()> {
    let write_txn = self.db.begin_write().unwrap();
    let table = get_first_key(&key);
    let table: TableDefinition<String, String> = TableDefinition::new(&table);
    {
      let mut table = write_txn
        .open_table(table)
        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
      table
        .remove(key)
        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
    }
    write_txn
      .commit()
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
  }
  #[napi]
  pub fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
    let mut regex_key = key.replace("*", ".*");
    regex_key.push_str("$");

    let regex = regex::Regex::new(&regex_key).unwrap();
    let mut found_keys = Vec::new();
    let read_txn = self.db.begin_read().unwrap();
    let table = get_first_key(&key);
    let table: TableDefinition<String, String> = TableDefinition::new(&table);
    let table = read_txn
      .open_table(table)
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
    let iter = table
      .iter()
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

    iter.for_each(|x| {
      let res = x.unwrap();

      if let Some(not_key) = &not_key {
        let mut not_regex_key = not_key.replace("*", ".*");
        not_regex_key.push_str("$");

        let not_regex = regex::Regex::new(&not_regex_key).unwrap();
        if res.0.value().to_string() != *not_key {
          if regex.is_match(&res.0.value().to_string())
            && !not_regex.is_match(&res.0.value().to_string())
          {
            found_keys.push(res.0.value().to_string());
          }
        }
      } else if regex.is_match(&res.0.value().to_string()) {
        found_keys.push(res.0.value().to_string());
      }
    });
    Ok(found_keys)
  }
  #[napi]
  pub fn destroy(&self) {
    fs::remove_file(&self.filename).unwrap();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_first_key() {
    assert_eq!(get_first_key("test:key"), "test");
  }

  #[test]
  fn test_find_keys_test() {
    let db = KeyValueDB::new("test.db".to_string());
    db.set("test:key".to_string(), "value".to_string());
    db.set("test:key2".to_string(), "value".to_string());
    db.set("test:key3".to_string(), "value".to_string());
    let keys = db.find_keys("test".to_string(), None).unwrap();
    assert_eq!(
      keys,
      vec![
        "test:key".to_string(),
        "test:key2".to_string(),
        "test:key3".to_string()
      ]
    );
  }

  #[test]
  fn test_find_keys_test_2() {
    let db = KeyValueDB::new("test.db".to_string());
    db.set("key:test".to_string(), "value".to_string());
    db.set("key:test2".to_string(), "value".to_string());
    db.set("key:123".to_string(), "value".to_string());
    let keys = db.find_keys("key:test2*".to_string(), None).unwrap();
    assert_eq!(keys, vec!["test:key2".to_string()]);
  }
}
