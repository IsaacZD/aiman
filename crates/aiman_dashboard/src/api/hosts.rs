//! Host management API handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::error::DashboardError;
use crate::hosts::{persist_hosts, validate_host};
use crate::state::AppState;
use crate::types::HostConfig;

/// List all configured hosts.
pub async fn list_hosts(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let hosts = state.hosts.read().await;
    Json(serde_json::json!({ "hosts": *hosts }))
}

/// Create a new host.
pub async fn create_host(
    State(state): State<AppState>,
    Json(payload): Json<HostConfig>,
) -> Result<(StatusCode, Json<serde_json::Value>), DashboardError> {
    validate_host(&payload)?;

    let mut hosts = state.hosts.write().await;
    if hosts.iter().any(|h| h.id == payload.id) {
        return Err(DashboardError::HostConflict);
    }

    hosts.push(payload.clone());
    persist_hosts(&state.hosts_store_path, &hosts).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "host": payload }))))
}

/// Update an existing host.
pub async fn update_host(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Json(payload): Json<HostConfig>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    validate_host(&payload)?;

    if payload.id != host_id {
        return Err(DashboardError::Validation("host id mismatch".to_string()));
    }

    let mut hosts = state.hosts.write().await;
    let Some(index) = hosts.iter().position(|h| h.id == host_id) else {
        return Err(DashboardError::HostNotFound);
    };

    hosts[index] = payload.clone();
    persist_hosts(&state.hosts_store_path, &hosts).await?;

    Ok(Json(serde_json::json!({ "host": payload })))
}

/// Delete a host.
pub async fn delete_host(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    let mut hosts = state.hosts.write().await;
    let original_len = hosts.len();
    hosts.retain(|h| h.id != host_id);

    if hosts.len() == original_len {
        return Err(DashboardError::HostNotFound);
    }

    persist_hosts(&state.hosts_store_path, &hosts).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
