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
            RpcError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            RpcError::AuthError(msg) => (StatusCode::UNAUTHORIZED, msg),
            RpcError::MempoolError(msg) => (StatusCode::BAD_REQUEST, msg),
            RpcError::StateError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            RpcError::EngineError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            RpcError::ServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            RpcError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            RpcError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: error_message,
        });

        (status, body).into_response()
    }
}
