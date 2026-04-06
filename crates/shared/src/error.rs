//! Common error handling utilities for API services.
//!
//! Provides traits and helpers for consistent error responses across
//! both the agent and dashboard services.

#[cfg(feature = "http")]
use axum::http::StatusCode;
#[cfg(feature = "http")]
use axum::response::{IntoResponse, Response};

/// Trait for errors that can be converted to HTTP responses.
///
/// Implement this trait on your domain-specific error types to get
/// consistent JSON error responses.
#[cfg(feature = "http")]
pub trait ApiError: std::error::Error {
    /// The HTTP status code for this error.
    fn status_code(&self) -> StatusCode;

    /// A user-friendly error message.
    fn error_message(&self) -> &str {
        // Default to the Display impl
        // Note: This returns a &str, so implementations should return
        // a static string or store the message in the error type
        "internal error"
    }
}

/// Convert an `ApiError` into an Axum response.
///
/// Returns a JSON response with the format: `{"error": "message"}`
#[cfg(feature = "http")]
pub fn error_response<E: ApiError>(err: &E) -> Response {
    let status = err.status_code();
    let body = serde_json::json!({ "error": err.error_message() });
    (status, axum::Json(body)).into_response()
}

/// Common API error variants that can be reused across services.
#[derive(Debug, thiserror::Error)]
pub enum CommonError {
    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("bad gateway: {0}")]
    BadGateway(String),

    #[error("service unavailable")]
    ServiceUnavailable,
}

#[cfg(feature = "http")]
impl ApiError for CommonError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadGateway(_) => StatusCode::BAD_GATEWAY,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn error_message(&self) -> &str {
        match self {
            Self::NotFound => "not found",
            Self::BadRequest(msg) | Self::Conflict(msg) | Self::Internal(msg) | Self::BadGateway(msg) => msg,
            Self::ServiceUnavailable => "service unavailable",
        }
    }
}

#[cfg(feature = "http")]
impl IntoResponse for CommonError {
    fn into_response(self) -> Response {
        error_response(&self)
    }
}
