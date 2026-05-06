use napi::{Error as NapiError, Status};

pub type Result<T> = std::result::Result<T, UeberError>;

#[derive(thiserror::Error, Debug)]
pub enum UeberError {
    #[error("the doBulk method must be implemented if write caching is enabled")]
    DoBulkNotImplemented,

    #[error("Cannot set property \"{prop}\" on non-object \"{value}\"")]
    SetSubOnNonObject { prop: String, value: String },

    #[error("backend init failed: {0}")]
    BackendInit(String),

    #[error("backend error: {0}")]
    Backend(#[from] anyhow::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("database not initialized")]
    NotInitialized,

    #[error("unknown backend type: {0}")]
    UnknownBackend(String),
}

impl UeberError {
    /// Lossy clone — preserves message string only. Used because anyhow::Error doesn't impl Clone
    /// but we need to broadcast errors to multiple awaiters of the write buffer flush.
    pub fn clone_for_broadcast(&self) -> UeberError {
        UeberError::Backend(anyhow::anyhow!(self.to_string()))
    }
}

impl From<UeberError> for NapiError {
    fn from(e: UeberError) -> NapiError {
        NapiError::new(Status::GenericFailure, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_sub_on_non_object_message_matches_ts() {
        let err = UeberError::SetSubOnNonObject {
            prop: "badProp".into(),
            value: "value".into(),
        };
        assert_eq!(
            err.to_string(),
            r#"Cannot set property "badProp" on non-object "value""#
        );
    }

    #[test]
    fn do_bulk_message_matches_ts() {
        assert_eq!(
            UeberError::DoBulkNotImplemented.to_string(),
            "the doBulk method must be implemented if write caching is enabled"
        );
    }

    #[test]
    fn clone_for_broadcast_preserves_message() {
        let err = UeberError::NotInitialized;
        let cloned = err.clone_for_broadcast();
        assert_eq!(cloned.to_string(), "backend error: database not initialized");
    }
}
