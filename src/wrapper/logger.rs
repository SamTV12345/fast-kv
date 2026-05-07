use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};

#[derive(Copy, Clone)]
pub enum Level {
  Debug,
  Info,
  Warn,
  Error,
}

impl Level {
  pub fn as_str(&self) -> &'static str {
    match self {
      Level::Debug => "debug",
      Level::Info => "info",
      Level::Warn => "warn",
      Level::Error => "error",
    }
  }
}

/// Pluggable logger backed by an optional JS callback.
///
/// `Logger::none()` discards all messages. `Logger::from_js(tsfn)` forwards
/// `(level, message)` to a JS function via `ThreadsafeFunction` in non-blocking
/// mode.
///
/// # napi-rs 3.x generic adjustment
///
/// The task spec used `ThreadsafeFunction<(String, String), ()>`, but in napi-rs 3.x
/// bare tuples do not implement `JsValuesTupleIntoVec` directly — only
/// `FnArgs<(A, B)>` does, and `FnArgs` is not re-exported from the public napi API.
///
/// To avoid depending on the private `bindgen_runtime` module, the TSFN is
/// erased behind a `Box<dyn Fn(String, String) + Send + Sync>` closure.
/// Phase 2 will call `from_js` with a `ThreadsafeFunction<String, ()>` that
/// receives a single JSON-encoded `level` while the message is passed as a
/// second positional parameter via a JS-side wrapper, or alternatively Phase 2
/// can store the TSFN directly via the concrete-type constructor below.
///
/// The concrete TSFN type stored inside the closure is:
///
///   `ThreadsafeFunction<String, ()>`  (single-arg: the level string)
///
/// with a separate `String` (the message) captured in the same call via a
/// pre-formatted `"level\t message"` string that the JS side can split, **or**
/// Phase 2 may supply a custom `Box<dyn Fn(String, String) + Send + Sync>`
/// directly through `Logger::from_fn`.
#[derive(Clone)]
pub struct Logger {
  inner: Option<std::sync::Arc<dyn Fn(String, String) + Send + Sync>>,
}

impl Logger {
  /// Returns a logger that silently discards every message.
  pub fn none() -> Self {
    Self { inner: None }
  }

  /// Builds a `Logger` from a `ThreadsafeFunction<(String, String), ()>`.
  ///
  /// Because `FnArgs<(String, String)>` is the napi-rs 3.x internal type
  /// needed for two-argument JS callbacks and it is not part of the public
  /// API, Phase 2 should supply a pre-built TSFN whose `T` is already
  /// `String` (level) and whose JS callback receives the message as the
  /// second argument through a combined payload.
  ///
  /// For now, this constructor accepts the combined-payload TSFN directly:
  /// the payload `String` is `"<level>\x1f<message>"` (unit-separator
  /// delimiter); the JS side splits on `\x1f`.  Phase 2 may also use
  /// `Logger::from_fn` with a richer closure.
  pub fn from_js(tsfn: ThreadsafeFunction<String, ()>) -> Self {
    let arc: std::sync::Arc<dyn Fn(String, String) + Send + Sync> =
      std::sync::Arc::new(move |level: String, msg: String| {
        let payload = format!("{}\x1f{}", level, msg);
        let _ = tsfn.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
      });
    Self { inner: Some(arc) }
  }

  /// Builds a `Logger` from any `Send + Sync` closure.
  ///
  /// Phase 2 can use this to wrap a fully-typed TSFN by capturing it inside
  /// the closure and calling it directly.
  pub fn from_fn<F>(f: F) -> Self
  where
    F: Fn(String, String) + Send + Sync + 'static,
  {
    Self {
      inner: Some(std::sync::Arc::new(f)),
    }
  }

  pub fn log(&self, level: Level, msg: impl Into<String>) {
    if let Some(f) = &self.inner {
      f(level.as_str().to_string(), msg.into());
    }
  }
}
