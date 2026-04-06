//! Proxied route handlers to host agents.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use reqwest::Method;
use serde::Deserialize;

use crate::error::DashboardError;
use crate::hosts::find_host;
use crate::state::AppState;

/// Query parameters for log/status endpoints.
#[derive(Debug, Deserialize, Default)]
pub struct LogQuery {
    pub since: Option<String>,
    pub limit: Option<usize>,
    pub session_id: Option<String>,
}

/// Helper to get a host or return 404.
async fn get_host(state: &AppState, host_id: &str) -> Result<crate::types::HostConfig, DashboardError> {
    let hosts = state.hosts.read().await;
    find_host(&hosts, host_id)
        .cloned()
        .ok_or(DashboardError::HostNotFound)
}

/// Proxy model scan request.
pub async fn proxy_models(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let libraries = host.model_libraries.clone().unwrap_or_default();
    if libraries.is_empty() {
        return Ok((StatusCode::OK, Json(serde_json::json!({ "artifacts": [] }))));
    }

    let body = serde_json::json!({ "libraries": libraries });
    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request(Method::POST, &host.base_url, "/v1/models/scan", host.api_key.as_deref(), Some(&body))
        .await?;

    Ok((status, Json(response)))
}

/// Proxy hardware info.
pub async fn proxy_hardware(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::GET, &host.base_url, "/v1/hardware", host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Proxy config list.
pub async fn proxy_configs(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::GET, &host.base_url, "/v1/configs", host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Create config on host.
pub async fn create_config(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request(Method::POST, &host.base_url, "/v1/configs", host.api_key.as_deref(), Some(&body))
        .await?;

    Ok((status, Json(response)))
}

/// Update config on host.
pub async fn update_config(
    State(state): State<AppState>,
    Path((host_id, config_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;
    let path = format!("/v1/configs/{config_id}");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request(Method::PUT, &host.base_url, &path, host.api_key.as_deref(), Some(&body))
        .await?;

    Ok((status, Json(response)))
}

/// Delete config on host.
pub async fn delete_config(
    State(state): State<AppState>,
    Path((host_id, config_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;
    let path = format!("/v1/configs/{config_id}");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::DELETE, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Proxy image list.
pub async fn proxy_images(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::GET, &host.base_url, "/v1/images", host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Create image on host.
pub async fn create_image(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request(Method::POST, &host.base_url, "/v1/images", host.api_key.as_deref(), Some(&body))
        .await?;

    Ok((status, Json(response)))
}

/// Update image on host.
pub async fn update_image(
    State(state): State<AppState>,
    Path((host_id, image_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;
    let path = format!("/v1/images/{}", urlencoding::encode(&image_id));

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request(Method::PUT, &host.base_url, &path, host.api_key.as_deref(), Some(&body))
        .await?;

    Ok((status, Json(response)))
}

/// Delete image on host.
pub async fn delete_image(
    State(state): State<AppState>,
    Path((host_id, image_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;
    let path = format!("/v1/images/{}", urlencoding::encode(&image_id));

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::DELETE, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Prune images on host.
pub async fn prune_images(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::POST, &host.base_url, "/v1/images/prune", host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Start engine on host.
pub async fn start_engine(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;
    let path = format!("/v1/engines/{engine_id}/start");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::POST, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Stop engine on host.
pub async fn stop_engine(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;
    let path = format!("/v1/engines/{engine_id}/stop");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::POST, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Proxy engine logs.
pub async fn proxy_logs(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
    Query(query): Query<LogQuery>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let mut params = Vec::new();
    if let Some(since) = &query.since {
        params.push(format!("since={}", urlencoding::encode(since)));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(session_id) = &query.session_id {
        params.push(format!("session_id={}", urlencoding::encode(session_id)));
    }

    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let path = format!("/v1/engines/{engine_id}/logs{qs}");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::GET, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Proxy engine log sessions.
pub async fn proxy_log_sessions(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
    Query(query): Query<LogQuery>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let mut params = Vec::new();
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }

    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let path = format!("/v1/engines/{engine_id}/logs/sessions{qs}");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::GET, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}

/// Proxy engine status history.
pub async fn proxy_status(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
    Query(query): Query<LogQuery>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    let host = get_host(&state, &host_id).await?;

    let mut params = Vec::new();
    if let Some(since) = &query.since {
        params.push(format!("since={}", urlencoding::encode(since)));
    }
    if let Some(limit) = query.limit {
        params.push(format!("limit={limit}"));
    }

    let qs = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let path = format!("/v1/engines/{engine_id}/status{qs}");

    let (status, response): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(Method::GET, &host.base_url, &path, host.api_key.as_deref(), None::<&()>)
        .await?;

    Ok((status, Json(response)))
}
