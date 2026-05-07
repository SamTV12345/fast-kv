use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum BulkOp {
    Set { key: String, value: Value },
    Remove { key: String },
}

#[async_trait]
pub trait Backend: Send + Sync {
    async fn init(&mut self) -> Result<()>;
    async fn close(&self) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<Value>>;
    async fn set(&self, key: &str, value: &Value) -> Result<()>;
    async fn remove(&self, key: &str) -> Result<()>;
    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>>;
    async fn do_bulk(&self, _ops: &[BulkOp]) -> Result<()> {
        Err(UeberError::DoBulkNotImplemented)
    }
    fn supports_native_glob(&self) -> bool {
        false
    }
    fn default_wrapper_settings(&self) -> DefaultWrapperHints {
        DefaultWrapperHints::default()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultWrapperHints {
    pub cache: Option<u32>,
    pub write_interval: Option<u32>,
    pub json: Option<bool>,
}

pub mod cassandra;
pub mod couch;
pub mod dirty;
pub mod dirty_git;
pub mod elasticsearch;
pub mod memory;
pub mod mongodb;
pub mod mssql;
pub mod mysql;
pub mod postgres;
pub mod redis;
pub mod rusty;
pub mod sqlite;
pub mod surrealdb;

pub async fn factory(type_: &str, settings: &Settings) -> Result<Box<dyn Backend>> {
    match type_ {
        "memory" => Ok(Box::new(memory::MemoryBackend::default())),
        "dirty" => Ok(Box::new(dirty::DirtyBackend::from_settings(settings)?)),
        "sqlite" => Ok(Box::new(sqlite::SqliteBackend::from_settings(settings)?)),
        "rustydb" | "rusty" => Ok(Box::new(rusty::RustyBackend::from_settings(settings)?)),
        "postgres" => Ok(Box::new(postgres::PostgresBackend::from_settings(
            settings, false,
        )?)),
        "postgrespool" => Ok(Box::new(postgres::PostgresBackend::from_settings(
            settings, true,
        )?)),
        "mysql" | "mariadb" => Ok(Box::new(mysql::MysqlBackend::from_settings(settings)?)),
        "mssql" => Ok(Box::new(mssql::MssqlBackend::from_settings(settings)?)),
        "redis" => Ok(Box::new(redis::RedisBackend::from_settings(settings)?)),
        "mongodb" => Ok(Box::new(mongodb::MongoBackend::from_settings(settings)?)),
        "couch" => Ok(Box::new(couch::CouchBackend::from_settings(settings)?)),
        "dirty_git" => Ok(Box::new(dirty_git::DirtyGitBackend::from_settings(settings)?)),
        "elasticsearch" => Ok(Box::new(
            elasticsearch::ElasticsearchBackend::from_settings(settings)?,
        )),
        "surrealdb" => Ok(Box::new(surrealdb::SurrealBackend::from_settings(settings)?)),
        "cassandra" => Ok(Box::new(cassandra::CassandraBackend::from_settings(settings)?)),
        #[cfg(test)]
        "_stub" => Ok(Box::new(test_stub::StubBackend::default())),
        other => Err(UeberError::UnknownBackend(other.to_string())),
    }
}

#[cfg(test)]
pub mod test_stub;

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_stub::StubBackend;
    use serde_json::json;

    #[tokio::test]
    async fn factory_unknown_backend_errors() {
        let s = Settings::default();
        let result = factory("nope", &s).await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(matches!(err, UeberError::UnknownBackend(_)));
    }

    #[tokio::test]
    async fn stub_round_trip() {
        let mut b = StubBackend::default();
        b.init().await.unwrap();
        b.set("k", &json!(1)).await.unwrap();
        assert_eq!(b.get("k").await.unwrap(), Some(json!(1)));
        b.remove("k").await.unwrap();
        assert!(b.get("k").await.unwrap().is_none());
        // Verify call log captured the sequence.
        let log = b.call_log.lock().unwrap().clone();
        assert_eq!(log, vec!["init", "set:k", "get:k", "remove:k", "get:k"]);
    }
}
