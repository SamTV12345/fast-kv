use napi_derive::napi;

#[napi(object)]
#[derive(Default, Clone)]
pub struct Settings {
  pub filename: Option<String>,
  pub host: Option<String>,
  pub port: Option<u32>,
  pub user: Option<String>,
  pub password: Option<String>,
  pub database: Option<String>,
  pub url: Option<String>,
  pub charset: Option<String>,
  pub engine: Option<String>,
  pub table: Option<String>,
  pub collection: Option<String>,
  pub db_name: Option<String>,
  pub connection_string: Option<String>,
  pub api: Option<String>,
  pub base_index: Option<String>,
  pub server: Option<String>,
  pub column_family: Option<String>,
  pub request_timeout: Option<u32>,
  pub query_timeout: Option<u32>,
  pub bulk_limit: Option<u32>,
  pub idle_timeout_millis: Option<u32>,
  pub min: Option<u32>,
  pub max: Option<u32>,
  pub migrate_to_newer_schema: Option<bool>,
  pub parse_json: Option<bool>,
  pub pool: Option<bool>,
  pub client_options: Option<serde_json::Value>,
}

#[napi(object)]
#[derive(Default, Clone)]
pub struct WrapperSettings {
  /// LRU read-cache capacity. Default 1000. 0 disables.
  pub cache: Option<u32>,
  /// Write buffer flush interval in ms. Default 100. 0 disables buffering.
  pub write_interval: Option<u32>,
  /// Maximum ops per bulk flush. Default 100.
  pub bulk_limit: Option<u32>,
  /// Encode values as JSON strings before passing to the backend. Defaults per-backend.
  pub json: Option<bool>,
}

impl WrapperSettings {
  pub fn cache_capacity(&self) -> u64 {
    self.cache.unwrap_or(1000) as u64
  }
  pub fn write_interval_ms(&self) -> u64 {
    self.write_interval.unwrap_or(100) as u64
  }
  pub fn bulk_limit(&self) -> usize {
    self.bulk_limit.unwrap_or(100) as usize
  }
}
