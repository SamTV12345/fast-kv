#![deny(clippy::all)]

use redb::{Database, ReadableTable, TableDefinition};
use std::fs;

#[macro_use]
extern crate napi_derive;

#[napi(js_name = "KeyValueDB")]
pub struct KeyValueDB {
    filename: String,
    db: Database,
}


const TABLE: TableDefinition<String, String> = TableDefinition::new("store");


#[napi]
impl KeyValueDB {
    #[napi(constructor)]
    pub fn new(filename: String) -> napi::Result<Self> {
        Ok(KeyValueDB {
            filename: filename.clone(),
            db: Database::create(&filename).map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?,
        })
    }

    #[napi]
    pub fn get(&self, key: String) -> napi::Result<Option<String>> {
        let read_txn = self.db.begin_read().map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        let table = read_txn
            .open_table(TABLE);
        if table.is_err() {
            return Ok(None);
        }

        let binding = table.map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        binding
            .get(key)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
            .map(|v| v.map(|v| v.value().to_string()))
    }
    #[napi]
    pub fn set(&self, key: String, value: String) -> napi::Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
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
    #[napi]
    pub fn remove(&self, key: String) -> napi::Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
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
    #[napi]
    pub fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
        let mut regex_key = "^".to_string();
        regex_key.push_str(&key);
        regex_key = regex_key.replace("*", ".*");

        regex_key.push('$');

        let regex = regex::Regex::new(&regex_key).map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        let mut found_keys = Vec::new();
        let read_txn = self.db.begin_read().map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        let table = read_txn
            .open_table(TABLE);

        if table.is_err() {
            return Ok(vec![]);
        }

        let binding = table.map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        let iter = binding
            .iter()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        iter.for_each(|x| {
            let res = x.unwrap();

            if let Some(not_key) = &not_key {
                let mut not_regex_key = "^".to_string();
                not_regex_key.push_str(&not_key);
                not_regex_key = not_regex_key.replace("*", ".*");
                not_regex_key.push('$');

                let not_regex = regex::Regex::new(&not_regex_key).unwrap();
                if res.0.value().to_string() != *not_key && regex.is_match(&res.0.value().to_string()) && !not_regex.is_match(&res.0.value().to_string()) {
                    found_keys.push(res.0.value().to_string());
                }
            } else if regex.is_match(&res.0.value().to_string()) {
                found_keys.push(res.0.value().to_string());
            }
        });
        Ok(found_keys)
    }
    #[napi]
    pub fn destroy(&self) -> napi::Result<()> {
        fs::remove_file(&self.filename).map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_keys_test() {
        let db = KeyValueDB::new("test.db".to_string()).unwrap();
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
        let db = KeyValueDB::new("test.db".to_string()).unwrap();
        db.set("key:test".to_string(), "value".to_string());
        db.set("key:test2".to_string(), "value".to_string());
        db.set("key:123".to_string(), "value".to_string());
        let keys = db.find_keys("key:test2*".to_string(), None).unwrap();
        assert_eq!(keys, vec!["key:test2".to_string()]);
    }
}
