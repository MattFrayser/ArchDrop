use axum::{
    http::StatusCode,
    response::{IntoResponse, Response, Json as AxumJson},
};
use serde_json::json;
use thiserror::Error;

/// Structured error types with proper HTTP status code mapping
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Insufficient storage: {0}")]
    InsufficientStorage(String),

    /// Catch-all for unexpected errors - logs full context internally
    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match self {
            AppError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthorized", msg)
            }
            AppError::NotFound(msg) => {
                (StatusCode::NOT_FOUND, "not_found", msg)
            }
            AppError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, "bad_request", msg)
            }
            AppError::InsufficientStorage(msg) => {
                (StatusCode::INSUFFICIENT_STORAGE, "insufficient_storage", msg)
            }
            AppError::Internal(ref err) => {
                // Log full error with backtrace server-side
                tracing::error!(
                    error = ?err,
                    backtrace = ?err.backtrace(),
                    "Internal server error"
                );
                // Return generic message to client (security best practice)
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                )
            }
        };

        let body = AxumJson(json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}
