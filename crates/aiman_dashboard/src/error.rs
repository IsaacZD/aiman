//! Dashboard-specific error types.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

/// Dashboard API errors.
#[derive(Debug, Error)]
pub enum DashboardError {
    #[error("host not found")]
    HostNotFound,

    #[error("validation error: {0}")]
    Validation(String),

    #[error("host already exists")]
    HostConflict,

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("I/O error: {0}")]
    IoError(String),

    #[error("proxy error: {0}")]
    ProxyError(String),

    #[error("bad gateway")]
    BadGateway,

    #[error("benchmark failed: {0}")]
    BenchmarkFailed(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl DashboardError {
    /// Get the HTTP status code for this error.
    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::HostNotFound => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::HostConflict => StatusCode::CONFLICT,
            Self::ConfigError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::IoError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ProxyError(_) => StatusCode::BAD_GATEWAY,
            Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::BenchmarkFailed(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let message = self.to_string();
        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}

impl From<aiman_shared::http::ProxyError> for DashboardError {
    fn from(err: aiman_shared::http::ProxyError) -> Self {
        Self::ProxyError(err.to_string())
    }
}
