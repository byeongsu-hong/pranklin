use thiserror::Error;

/// State management errors
#[derive(Error, Debug)]
pub enum StateError {
    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Key not found")]
    KeyNotFound,

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Invalid version")]
    InvalidVersion,

    #[error("Other error: {0}")]
    Other(String),
}

impl From<serde_json::Error> for StateError {
    fn from(err: serde_json::Error) -> Self {
        StateError::SerializationError(err.to_string())
    }
}

impl From<std::io::Error> for StateError {
    fn from(err: std::io::Error) -> Self {
        StateError::StorageError(err.to_string())
    }
}
