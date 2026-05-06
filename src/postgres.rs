use crate::general::BulkObject;
use napi::{Error, Status};
use postgres::types::ToSql;
use postgres::{Client, Error as PostgresError, NoTls};

pub struct Postgres {
  db: Option<Client>,
}

// Define your own error wrapper
pub struct PostgresErrorWrapper(PostgresError);

// Implement `From` for your new wrapper type
impl From<PostgresError> for PostgresErrorWrapper {
  fn from(err: PostgresError) -> Self {
    PostgresErrorWrapper(err)
  }
}

// Implement `From` for converting SqliteErrorWrapper to napi::Error
impl From<PostgresErrorWrapper> for Error {
  fn from(wrapper: PostgresErrorWrapper) -> Self {
    Error::new(
      Status::GenericFailure,
      format!("Rusqlite error: {}", wrapper.0),
    )
  }
}

pub struct PostgresSettings {
  pub user: String,
  pub database: String,
  pub port: u16,
  pub host: String,
  pub password: String,
}

const CREATE_TABLE_SQL: &str =
  "CREATE TABLE IF NOT EXISTS store (key TEXT PRIMARY KEY, value TEXT)";

impl Postgres {
  pub fn new(filename: PostgresSettings) -> napi::Result<Self> {
    let mut client = Client::connect(
      &format!(
        "host={} user={} password={} dbname={} port={}",
        &filename.host, &filename.user, &filename.password, &filename.database, &filename.port
      ),
      NoTls,
    )
    .map_err(PostgresErrorWrapper::from)?;

    client
      .execute(CREATE_TABLE_SQL, &[])
      .map_err(PostgresErrorWrapper::from)?;
    Ok(Postgres { db: Some(client) })
  }
  pub fn find_keys(&mut self, key: String, not_key: Option<String>) -> napi::Result<Vec<String>> {
    match &mut self.db {
      Some(db) => {
        let mut query = "SELECT key FROM store WHERE key LIKE $1".to_string();
        let mut params: Vec<&(dyn ToSql + Sync)> = vec![];
        let res_key = key.replace("*", "%");
        params.push(&res_key);
        let res_not_key: String;
        if let Some(not_key) = not_key {
          res_not_key = not_key.replace("*", "%");
          query.push_str(" AND key NOT LIKE $2");
          params.push(&res_not_key);
        }
        let stmt = db.prepare(&query).map_err(PostgresErrorWrapper::from)?;

        db.execute(&stmt, &params)
          .map_err(PostgresErrorWrapper::from)?;
        let rows = db
          .query(&stmt, &params)
          .map_err(PostgresErrorWrapper::from)?
          .iter()
          .map(|r| {
            let str: String = r.get::<&str, String>("key");
            str
          })
          .collect::<Vec<String>>();

        Ok(rows)
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  pub fn get(&mut self, key: String) -> napi::Result<Option<String>> {
    match &mut self.db {
      Some(db) => {
        let prep_get = db
          .prepare("SELECT value FROM store WHERE key = $1")
          .map_err(PostgresErrorWrapper::from)?;
        let result = db
          .query(&prep_get, &[&key])
          .map_err(PostgresErrorWrapper::from)?;
        match result.first() {
          Some(r) => {
            let first_row: String = r.get("value");
            Ok(Some(first_row))
          }
          None => Ok(None),
        }
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  pub fn set(&mut self, key: String, value: String) -> napi::Result<Option<i64>> {
    match &mut self.db {
      Some(db) => {
        let prep_get = db
          .prepare(
            "INSERT INTO store (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value =
                    $2",
          )
          .map_err(PostgresErrorWrapper::from)?;
        let res = db
          .execute(&prep_get, &[&key, &value])
          .map_err(PostgresErrorWrapper::from)?;
        Ok(Some(res as i64))
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }

  pub fn remove(&mut self, key: String) -> napi::Result<()> {
    match &mut self.db {
      Some(db) => {
        let prep_statement = db
          .prepare("DELETE FROM store WHERE key = $1")
          .map_err(PostgresErrorWrapper::from)?;
        db.execute(&prep_statement, &[&key])
          .map_err(PostgresErrorWrapper::from)?;
        Ok(())
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }
  pub fn do_bulk(&mut self, bulk_object: Vec<BulkObject>) -> napi::Result<()> {
    match self.db {
      Some(ref mut db) => {
        let mut tx = db.transaction().map_err(PostgresErrorWrapper::from)?;

        for bulk_ob in bulk_object {
          if bulk_ob.r#type == "set" {
            let prep_statement = tx
              .prepare("REPLACE INTO store VALUES ($1,$2)")
              .map_err(PostgresErrorWrapper::from)?;
            tx.execute(&prep_statement, &[&bulk_ob.key, &bulk_ob.value.unwrap()])
              .map_err(PostgresErrorWrapper::from)?;
          } else {
            let prep_statement = tx
              .prepare("DELETE FROM store WHERE key = $1")
              .map_err(PostgresErrorWrapper::from)?;
            tx.execute(&prep_statement, &[&bulk_ob.key])
              .map_err(PostgresErrorWrapper::from)?;
          }
        }

        tx.commit().map_err(PostgresErrorWrapper::from)?;
        Ok(())
      }
      None => Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      )),
    }
  }
  pub fn close(&mut self) -> napi::Result<()> {
    if let Some(db) = self.db.take() {
      // Take ownership and drop the connection
      db.close().map_err(PostgresErrorWrapper::from)?;
      self.db = None;
    }
    Ok(())
  }

  pub fn destroy(&mut self) -> napi::Result<()> {
    if let Some(db) = &mut self.db {
      db.execute("DELETE FROM store", &[])
        .map_err(PostgresErrorWrapper::from)?;
      Ok(())
    } else {
      Err(Error::new(
        Status::GenericFailure,
        "Db not initialized".to_string(),
      ))
    }
  }
}
