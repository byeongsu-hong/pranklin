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
