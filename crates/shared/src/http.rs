//! HTTP proxy client utilities.
//!
//! Provides a client for proxying requests to remote services with
//! optional bearer token authentication.

use axum::http::StatusCode;
use bytes::Bytes;
use futures_util::Stream;
use reqwest::Method;
use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

/// Errors that can occur during proxy operations.
#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("upstream returned {status}: {message}")]
    UpstreamError { status: u16, message: String },

    #[error("response deserialization failed: {0}")]
    DeserializationFailed(#[from] serde_json::Error),

    #[error("upstream unavailable")]
    Unavailable,
}

impl ProxyError {
    /// Convert this error to an HTTP status code.
    #[must_use]
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::RequestFailed(_) | Self::Unavailable => StatusCode::BAD_GATEWAY,
            Self::UpstreamError { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            Self::DeserializationFailed(_) => StatusCode::BAD_GATEWAY,
        }
    }
}

/// HTTP client for proxying requests to remote services.
///
/// Handles bearer token authentication and provides typed request/response handling.
#[derive(Clone)]
pub struct ProxyClient {
    client: reqwest::Client,
}

impl Default for ProxyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyClient {
    /// Create a new proxy client with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Create a proxy client with a custom reqwest client.
    #[must_use]
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Make a JSON request and deserialize the response.
    ///
    /// # Errors
    ///
    /// Returns `ProxyError` if the request fails or the response cannot be deserialized.
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
        body: Option<&impl Serialize>,
    ) -> Result<(StatusCode, T), ProxyError> {
        let url = format!("{}{}", base_url.trim_end_matches('/'), path);

        let mut req = self.client.request(method, &url);

        if let Some(key) = api_key {
            req = req.bearer_auth(key);
        }

        if let Some(body) = body {
            req = req.json(body);
        }

        let res = req.send().await?;
        let status = StatusCode::from_u16(res.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

        if !res.status().is_success() {
            let message = res.text().await.unwrap_or_default();
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message,
            });
        }

        let body: T = res.json().await?;
        Ok((status, body))
    }

    /// Make a request and return the raw JSON value.
    ///
    /// Useful when you don't know the exact response shape or need to forward it as-is.
    ///
    /// # Errors
    ///
    /// Returns `ProxyError` if the request fails.
    pub async fn request_json(
        &self,
        method: Method,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
        body: Option<&impl Serialize>,
    ) -> Result<(StatusCode, serde_json::Value), ProxyError> {
        self.request(method, base_url, path, api_key, body).await
    }

    /// Make a request without expecting a response body.
    ///
    /// # Errors
    ///
    /// Returns `ProxyError` if the request fails.
    pub async fn request_no_body(
        &self,
        method: Method,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
        body: Option<&impl Serialize>,
    ) -> Result<StatusCode, ProxyError> {
        let url = format!("{}{}", base_url.trim_end_matches('/'), path);

        let mut req = self.client.request(method, &url);

        if let Some(key) = api_key {
            req = req.bearer_auth(key);
        }

        if let Some(body) = body {
            req = req.json(body);
        }

        let res = req.send().await?;
        let status = StatusCode::from_u16(res.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

        if !res.status().is_success() {
            let message = res.text().await.unwrap_or_default();
            return Err(ProxyError::UpstreamError {
                status: status.as_u16(),
                message,
            });
        }

        Ok(status)
    }

    /// Start an SSE stream from the upstream service.
    ///
    /// Returns a stream of raw bytes that can be forwarded to clients.
    ///
    /// # Errors
    ///
    /// Returns `ProxyError` if the connection fails.
    pub async fn stream_sse(
        &self,
        base_url: &str,
        path: &str,
        api_key: Option<&str>,
    ) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, ProxyError> {
        let url = format!("{}{}", base_url.trim_end_matches('/'), path);

        let mut req = self.client.get(&url).header("Accept", "text/event-stream");

        if let Some(key) = api_key {
            req = req.bearer_auth(key);
        }

        let res = req.send().await?;

        if !res.status().is_success() {
            return Err(ProxyError::Unavailable);
        }

        Ok(res.bytes_stream())
    }

    /// Get the underlying reqwest client for custom operations.
    #[must_use]
    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }
}
