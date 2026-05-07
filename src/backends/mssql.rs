use super::{Backend, BulkOp, DefaultWrapperHints};
use crate::error::{Result, UeberError};
use crate::settings::Settings;
use async_trait::async_trait;
use bb8_tiberius::ConnectionManager;
use serde_json::Value;
use tiberius::Config;

const CREATE_TABLE_SQL: &str = "IF NOT EXISTS ( \
    SELECT * FROM INFORMATION_SCHEMA.TABLES WHERE TABLE_NAME = 'store' \
) CREATE TABLE store (k NVARCHAR(450) PRIMARY KEY, v NVARCHAR(MAX) NOT NULL)";

// VARCHAR `=` strips trailing whitespace; the DATALENGTH guard forces a
// byte-exact match so keys with significant trailing whitespace round-trip
// correctly (matches the equivalent BINARY trick the mysql backend uses).
const UPSERT_SQL: &str = "MERGE store AS t \
    USING (SELECT @P1 AS k, @P2 AS v) AS s \
    ON (t.k = s.k AND DATALENGTH(t.k) = DATALENGTH(s.k)) \
    WHEN MATCHED THEN UPDATE SET v = s.v \
    WHEN NOT MATCHED THEN INSERT (k, v) VALUES (s.k, s.v);";

pub struct MssqlBackend {
  cfg: Config,
  pool: Option<bb8::Pool<ConnectionManager>>,
}

impl MssqlBackend {
  pub fn from_settings(settings: &Settings) -> Result<Self> {
    let mut cfg = Config::new();
    cfg.host(settings.host.as_deref().unwrap_or("localhost"));
    cfg.port(settings.port.unwrap_or(1433) as u16);
    if let Some(d) = &settings.database {
      cfg.database(d);
    }
    if let (Some(u), Some(p)) = (&settings.user, &settings.password) {
      cfg.authentication(tiberius::AuthMethod::sql_server(u, p));
    }
    // Containerized SQL Server uses a self-signed cert; trust it for the
    // test harness. Production deployments override with a valid cert.
    cfg.trust_cert();
    Ok(Self { cfg, pool: None })
  }
}

#[async_trait]
impl Backend for MssqlBackend {
  async fn init(&mut self) -> Result<()> {
    let mgr = ConnectionManager::build(self.cfg.clone())
      .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    let pool = bb8::Pool::builder()
      .max_size(5)
      .build(mgr)
      .await
      .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    {
      let mut c = pool
        .get()
        .await
        .map_err(|e| UeberError::BackendInit(e.to_string()))?;
      c.execute(CREATE_TABLE_SQL, &[])
        .await
        .map_err(|e| UeberError::BackendInit(e.to_string()))?;
    }
    self.pool = Some(pool);
    Ok(())
  }

  async fn close(&self) -> Result<()> {
    Ok(())
  }

  async fn get(&self, key: &str) -> Result<Option<Value>> {
    let pool = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
    let mut c = pool
      .get()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
    let stream = c
      .query(
        "SELECT v FROM store WHERE k = @P1 AND DATALENGTH(k) = DATALENGTH(@P1)",
        &[&key],
      )
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    let row = stream
      .into_row()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    match row {
      Some(r) => {
        let v: Option<&str> = r.get(0);
        match v {
          Some(s) => Ok(Some(serde_json::from_str(s)?)),
          None => Ok(None),
        }
      }
      None => Ok(None),
    }
  }

  async fn set(&self, key: &str, value: &Value) -> Result<()> {
    let pool = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
    let mut c = pool
      .get()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
    let json = serde_json::to_string(value)?;
    c.execute(UPSERT_SQL, &[&key, &json.as_str()])
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    Ok(())
  }

  async fn remove(&self, key: &str) -> Result<()> {
    let pool = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
    let mut c = pool
      .get()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
    c.execute(
      "DELETE FROM store WHERE k = @P1 AND DATALENGTH(k) = DATALENGTH(@P1)",
      &[&key],
    )
    .await
    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    Ok(())
  }

  async fn find_keys(&self, key: &str, not_key: Option<&str>) -> Result<Vec<String>> {
    let pool = self.pool.as_ref().ok_or(UeberError::NotInitialized)?;
    let mut c = pool
      .get()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e.to_string())))?;
    let pattern = key.replace('*', "%");
    let stream = if let Some(nk) = not_key {
      let np = nk.replace('*', "%");
      c.query(
        "SELECT k FROM store WHERE k LIKE @P1 AND k NOT LIKE @P2",
        &[&pattern.as_str(), &np.as_str()],
      )
      .await
    } else {
      c.query("SELECT k FROM store WHERE k LIKE @P1", &[&pattern.as_str()])
        .await
    }
    .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    let rows = stream
      .into_first_result()
      .await
      .map_err(|e| UeberError::Backend(anyhow::anyhow!(e)))?;
    Ok(
      rows
        .into_iter()
        .filter_map(|r| {
          let s: Option<&str> = r.get(0);
          s.map(String::from)
        })
        .collect(),
    )
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
