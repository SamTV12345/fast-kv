use napi::{Error, Status};
use rusqlite::Error as RusqliteError;
use rusqlite::{params, params_from_iter, Connection, ToSql};

#[napi(js_name = "SQLite")]
pub struct SQLite {
  db: Option<Connection>,
}

// Define your own error wrapper
pub struct SqliteErrorWrapper(RusqliteError);

// Implement `From` for your new wrapper type
impl From<RusqliteError> for SqliteErrorWrapper {
  fn from(err: RusqliteError) -> Self {
    SqliteErrorWrapper(err)
  }
}

// Implement `From` for converting SqliteErrorWrapper to napi::Error
impl From<SqliteErrorWrapper> for Error {
  fn from(wrapper: SqliteErrorWrapper) -> Self {
    Error::new(
      Status::GenericFailure,
      format!("Rusqlite error: {}", wrapper.0),
    )
  }
}

const CREATE_TABLE_SQL: &str =
  "CREATE TABLE IF NOT EXISTS store (key TEXT PRIMARY KEY, value TEXT)";

#[napi(object)]
pub struct BulkObject {
  pub r#type: String,
  pub key: String,
  pub value: Option<String>,
}

#[napi]
impl SQLite {
  #[napi(constructor)]
  pub fn new(filename: String) -> napi::Result<Self> {
    let conn;

    if filename == ":memory:" {
      let memory_conn = Connection::open_in_memory().map_err(SqliteErrorWrapper::from)?;
      memory_conn
        .execute(CREATE_TABLE_SQL, [])
        .map_err(SqliteErrorWrapper::from)?;
      conn = Some(memory_conn);
    } else if !filename.is_empty() {
      let file_conn = Connection::open(&filename).map_err(SqliteErrorWrapper::from)?;
      file_conn
        .execute(CREATE_TABLE_SQL, [])
        .map_err(SqliteErrorWrapper::from)?;
      conn = Some(file_conn)
    } else {
      return Err(napi::Error::new(
        napi::Status::GenericFailure,
        "filename is empty".to_string(),
      ));
    }

    Ok(SQLite { db: conn })
  }
  #[napi]
  pub fn find_keys(&self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
    match &self.db {
      Some(db) => {
        let mut query = "SELECT key FROM store WHERE key LIKE ?".to_string();
        let mut results = vec![];
        let mut params: Vec<Box<dyn ToSql>> = vec![];
        let res_key = key.replace("*", "%");
        params.push(Box::new(res_key));

        if let Some(not_key) = not_key {
          let res_not_key = not_key.replace("*", "%");
          query.push_str(" AND key NOT LIKE ?");
          params.push(Box::new(res_not_key));
        }
        let mut stmt = db.prepare(&query).map_err(SqliteErrorWrapper::from)?;
        let rows = stmt
          .query_map(params_from_iter(params), |row| {
            let str: String = row.get(0).unwrap();
            Ok(str)
          })
          .map_err(SqliteErrorWrapper::from)?;

        for row in rows {
          results.push(row.map_err(SqliteErrorWrapper::from)?);
        }

        Ok(results)
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  #[napi]
  pub fn get(&self, key: String) -> napi::Result<Option<String>> {
    match &self.db {
      Some(db) => {
        let mut prep_get = db
          .prepare("SELECT value FROM store WHERE key = ?")
          .map_err(SqliteErrorWrapper::from)?;
        let mut result = prep_get.query([key]).map_err(SqliteErrorWrapper::from)?;
        match result.next() {
          Ok(r) => {
            if let Some(found_row) = r {
              let value: String = found_row.get(0).map_err(SqliteErrorWrapper::from)?;
              return Ok(Option::from(value));
            }
            Ok(None)
          }
          Err(e) => Err(Error::from(SqliteErrorWrapper::from(e))),
        }
      }
      None => Err(napi::Error::new(
        napi::Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  #[napi]
  pub fn set(&self, key: String, value: String) -> napi::Result<Option<i32>> {
    match &self.db {
      Some(db) => {
        let mut prep_get = db
          .prepare("REPLACE INTO store VALUES (?,?)")
          .map_err(SqliteErrorWrapper::from)?;
        let res = prep_get
          .execute([key, value])
          .map_err(SqliteErrorWrapper::from)?;
        Ok(Some(res as i32))
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  #[napi]
  pub fn remove(&self, key: String) -> napi::Result<()> {
    match &self.db {
      Some(db) => {
        let mut prep_statement = db
          .prepare("DELETE FROM store WHERE key = ?")
          .map_err(SqliteErrorWrapper::from)?;
        prep_statement
          .execute([key])
          .map_err(SqliteErrorWrapper::from)?;
        Ok(())
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }
  #[napi]
  pub fn do_bulk(&mut self, bulk_object: Vec<BulkObject>) -> napi::Result<()> {
    match self.db {
      Some(ref mut db) => {
        let tx = db.transaction().map_err(SqliteErrorWrapper::from)?;

        for bulk_ob in bulk_object {
          if bulk_ob.r#type == "set" {
            let mut prep_statement = tx
              .prepare("REPLACE INTO store VALUES (?,?)")
              .map_err(SqliteErrorWrapper::from)?;
            prep_statement
              .execute(params![bulk_ob.key, bulk_ob.value.unwrap()])
              .map_err(SqliteErrorWrapper::from)?;
          } else {
            let mut prep_statement = tx
              .prepare("DELETE FROM store WHERE key = ?")
              .map_err(SqliteErrorWrapper::from)?;
            prep_statement
              .execute(params![bulk_ob.key])
              .map_err(SqliteErrorWrapper::from)?;
          }
        }

        tx.commit().map_err(SqliteErrorWrapper::from)?;
        Ok(())
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }
  #[napi]
  pub fn close(&mut self) -> napi::Result<()> {
    if let Some(db) = self.db.take() {
      // Take ownership and drop the connection
      db.close().map_err(|(_, e)| SqliteErrorWrapper::from(e))?;
    }
    Ok(())
  }
}
