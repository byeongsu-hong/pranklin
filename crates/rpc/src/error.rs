use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// RPC errors
#[derive(Error, Debug)]
pub enum RpcError {
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Mempool error: {0}")]
    MempoolError(String),

    #[error("State error: {0}")]
    StateError(String),

    #[error("Engine error: {0}")]
    EngineError(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Not found")]
    NotFound,

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Error response
#[derive(Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for RpcError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            Self::InvalidRequest(msg) | Self::MempoolError(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::AuthError(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            Self::StateError(msg) 
            | Self::EngineError(msg) 
            | Self::ServerError(msg) 
            | Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        (status, Json(ErrorResponse { error: error_message })).into_response()
    }
}
