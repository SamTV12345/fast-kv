use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use bb8_postgres::PostgresConnectionManager;
use serde_json::Value;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, NoTls};

const CREATE_TABLE_SQL: &str =
    "CREATE TABLE IF NOT EXISTS store (key TEXT PRIMARY KEY, value JSONB NOT NULL)";

enum PgClient {
    Single(Client),
    Pool(bb8::Pool<PostgresConnectionManager<NoTls>>),
}

pub struct PostgresBackend {
    cfg: tokio_postgres::Config,
    pool: bool,
    client: Option<PgClient>,
}

impl PostgresBackend {
    pub fn from_settings(settings: &Settings, force_pool: bool) -> Result<Self> {
        let mut cfg = tokio_postgres::Config::new();
        cfg.host(settings.host.as_deref().unwrap_or("localhost"));
        cfg.port(settings.port.unwrap_or(5432) as u16);
        if let Some(u) = &settings.user {
            cfg.user(u);
        }
        if let Some(p) = &settings.password {
            cfg.password(p);
        }
        if let Some(d) = &settings.database {
            cfg.dbname(d);
        }
        Ok(Self {
            cfg,
            pool: force_pool || settings.pool.unwrap_or(false),
            client: None,
        })
    }

    fn client(&self) -> Result<&PgClient> {
        self.client.as_ref().ok_or(UeberError::NotInitialized)
    }

    async fn execute(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<u64> {
        match self.client()? {
            PgClient::Single(c) => c
                .execute(sql, params)
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e))),
            PgClient::Pool(p) => {
                let c = p
                    .get()
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
                c.execute(sql, params)
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))
            }
        }
    }

    async fn query_value(&self, sql: &str, params: &[&(dyn ToSql + Sync)]) -> Result<Option<Value>> {
        let row = match self.client()? {
            PgClient::Single(c) => c
                .query_opt(sql, params)
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?,
            PgClient::Pool(p) => {
                let c = p
                    .get()
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
                c.query_opt(sql, params)
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
            }
        };
        Ok(row.map(|r| r.get::<_, Value>(0)))
    }

    async fn query_keys(
        &self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<String>> {
        let rows = match self.client()? {
            PgClient::Single(c) => c
                .query(sql, params)
                .await
                .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?,
            PgClient::Pool(p) => {
                let c = p
                    .get()
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
                c.query(sql, params)
                    .await
                    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?
            }
        };
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }
}

#[async_trait]
impl Backend for PostgresBackend {
    async fn init(&mut self) -> Result<()> {
        if self.pool {
            let mgr = PostgresConnectionManager::new(self.cfg.clone(), NoTls);
            let pool = bb8::Pool::builder()
                .max_size(15)
                .build(mgr)
                .await
                .map_err(|e| UeberError::BackendInit(e.to_string()))?;
            {
                let c = pool
                    .get()
                    .await
                    .map_err(|e| UeberError::BackendInit(e.to_string()))?;
                c.batch_execute(CREATE_TABLE_SQL)
                    .await
                    .map_err(|e| UeberError::BackendInit(e.to_string()))?;
            }
            self.client = Some(PgClient::Pool(pool));
        } else {
            let (client, conn) = self
                .cfg
                .connect(NoTls)
                .await
                .map_err(|e| UeberError::BackendInit(e.to_string()))?;
            // The connection driver has to be polled in the background; if we
            // don't spawn it nothing flows. The task ends when the client drops.
            tokio::spawn(async move {
                let _ = conn.await;
            });
            client
                .batch_execute(CREATE_TABLE_SQL)
                .await
                .map_err(|e| UeberError::BackendInit(e.to_string()))?;
            self.client = Some(PgClient::Single(client));
        }
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        // The Drop impl on Client/Pool tears down connections; nothing to do here
        // because we only have shared access. The Database wrapper drops the
        // backend Arc after close.
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Value>> {
        self.query_value("SELECT value FROM store WHERE key = $1", &[&key])
            .await
    }

    async fn set(&self, key: &str, value: &Value) -> Result<()> {
        self.execute(
            "INSERT INTO store(key, value) VALUES ($1, $2) \
             ON CONFLICT(key) DO UPDATE SET value = EXCLUDED.value",
            &[&key, &value],
        )
        .await?;
        Ok(())
    }

    async fn remove(&self, key: &str) -> Result<()> {
        self.execute("DELETE FROM store WHERE key = $1", &[&key])
            .await?;
        Ok(())
    }

    async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
        let pattern = key.replace('*', "%");
        match not_key {
            Some(nk) => {
                let np = nk.replace('*', "%");
                self.query_keys(
                    "SELECT key FROM store WHERE key LIKE $1 AND key NOT LIKE $2",
                    &[&pattern, &np],
                )
                .await
            }
            None => {
                self.query_keys("SELECT key FROM store WHERE key LIKE $1", &[&pattern])
                    .await
            }
        }
    }

    async fn do_bulk(&self, ops: &[BulkOp]) -> Result<()> {
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
