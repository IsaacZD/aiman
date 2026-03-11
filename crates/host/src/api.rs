use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tokio::task;
use sysinfo::System;

use aiman_shared::{EngineConfig, EngineInstance, LogEntry};

use crate::models::scan_model_libraries;
use crate::state::AppState;
use crate::supervisor::{map_supervisor_error, read_jsonl};

pub async fn health() -> &'static str {
    "ok"
}

pub async fn hardware_info() -> Json<HardwareResponse> {
    let mut system = System::new_all();
    system.refresh_all();
    let gpus = task::spawn_blocking(collect_gpus).await.unwrap_or_default();

    let cpu = system.cpus().first();
    let cpu_brand = cpu
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|brand| !brand.is_empty());
    let cpu_frequency_mhz = cpu
        .map(|cpu| cpu.frequency())
        .filter(|freq| *freq > 0)
        .map(u64::from);

    Json(HardwareResponse {
        hardware: HardwareInfo {
            hostname: System::host_name(),
            os_name: System::name(),
            os_version: System::os_version(),
            kernel_version: System::kernel_version(),
            cpu_brand,
            cpu_cores_logical: system.cpus().len(),
            cpu_cores_physical: system.physical_core_count(),
            cpu_frequency_mhz,
            memory_total_kb: system.total_memory(),
            memory_available_kb: system.available_memory(),
            swap_total_kb: system.total_swap(),
            swap_free_kb: system.free_swap(),
            uptime_seconds: System::uptime(),
            gpus,
        },
    })
}

pub async fn list_engines(State(state): State<AppState>) -> Json<EnginesResponse> {
    tracing::debug!("list engines requested");
    Json(EnginesResponse {
        engines: state.supervisor.list_instances().await,
    })
}

pub async fn get_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
    tracing::debug!(engine_id = %id, "get engine requested");
    let instance = state
        .supervisor
        .get_instance(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(EngineResponse { instance }))
}

pub async fn start_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
    tracing::info!(engine_id = %id, "start engine API called");
    let instance = state
        .supervisor
        .start(&id)
        .await
        .map_err(map_supervisor_error)?;

    Ok(Json(EngineResponse { instance }))
}

pub async fn stop_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
    tracing::info!(engine_id = %id, "stop engine API called");
    let instance = state
        .supervisor
        .stop(&id)
        .await
        .map_err(map_supervisor_error)?;

    Ok(Json(EngineResponse { instance }))
}

// WebSocket log stream (JSON per line) with initial buffer replay.
pub async fn engine_logs_ws(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    tracing::debug!(engine_id = %id, "engine logs websocket upgrade requested");
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(ws.on_upgrade(move |socket| async move {
        tracing::info!(engine_id = %id, "engine logs websocket connected");
        let mut rx = handle.log_tx.subscribe();
        let buffer = { handle.log_buffer.lock().await.clone() };

        let (mut sender, mut receiver) = socket.split();

        for entry in buffer {
            if let Ok(text) = serde_json::to_string(&entry) {
                if sender
                    .send(axum::extract::ws::Message::Text(text.into()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        // Fan out new log entries to the client.
        loop {
            tokio::select! {
                Ok(entry) = rx.recv() => {
                    if let Ok(text) = serde_json::to_string(&entry) {
                        if sender
                            .send(axum::extract::ws::Message::Text(text.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                msg = receiver.next() => {
                    if msg.is_none() {
                        break;
                    }
                }
                else => break,
            }
        }
        tracing::info!(engine_id = %id, "engine logs websocket disconnected");
    }))
}

#[derive(Deserialize)]
pub(crate) struct LogQuery {
    since: Option<String>,
    limit: Option<usize>,
}

pub async fn engine_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogQuery>,
) -> Result<Json<LogHistoryResponse>, StatusCode> {
    tracing::debug!(
        engine_id = %id,
        since = query.since.as_deref(),
        limit = query.limit,
        "engine logs requested"
    );
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let entries = read_jsonl(&handle.log_path, query.since.as_deref(), query.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LogHistoryResponse { entries }))
}

pub async fn engine_status_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogQuery>,
) -> Result<Json<StatusHistoryResponse>, StatusCode> {
    tracing::debug!(
        engine_id = %id,
        since = query.since.as_deref(),
        limit = query.limit,
        "engine status history requested"
    );
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let entries = read_jsonl(&handle.status_path, query.since.as_deref(), query.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(StatusHistoryResponse { entries }))
}

pub async fn list_configs(State(state): State<AppState>) -> Json<ConfigsResponse> {
    tracing::debug!("list configs requested");
    Json(ConfigsResponse {
        configs: state.supervisor.list_configs().await,
    })
}

pub async fn create_config(
    State(state): State<AppState>,
    Json(config): Json<EngineConfig>,
) -> Result<Json<ConfigResponse>, StatusCode> {
    tracing::info!(engine_id = %config.id, "create config API called");
    let config = state
        .supervisor
        .add_config(config)
        .await
        .map_err(map_supervisor_error)?;
    Ok(Json(ConfigResponse { config }))
}

pub async fn update_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(config): Json<EngineConfig>,
) -> Result<Json<ConfigResponse>, StatusCode> {
    tracing::info!(engine_id = %id, "update config API called");
    let config = state
        .supervisor
        .update_config(&id, config)
        .await
        .map_err(map_supervisor_error)?;
    Ok(Json(ConfigResponse { config }))
}

pub async fn delete_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, StatusCode> {
    tracing::info!(engine_id = %id, "delete config API called");
    state
        .supervisor
        .remove_config(&id)
        .await
        .map_err(map_supervisor_error)?;
    Ok(Json(DeleteResponse { ok: true }))
}

#[derive(Deserialize)]
pub(crate) struct ModelScanRequest {
    libraries: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct ModelScanResponse {
    artifacts: Vec<crate::models::ModelArtifact>,
}

pub async fn scan_models(
    State(_state): State<AppState>,
    Json(request): Json<ModelScanRequest>,
) -> Result<Json<ModelScanResponse>, StatusCode> {
    tracing::info!(count = request.libraries.len(), "model scan requested");
    let libraries = request.libraries.clone();
    let artifacts = tokio::task::spawn_blocking(move || scan_model_libraries(&libraries))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracing::info!(count = artifacts.len(), "model scan completed");
    Ok(Json(ModelScanResponse { artifacts }))
}

#[derive(Serialize)]
pub(crate) struct EnginesResponse {
    engines: Vec<EngineInstance>,
}

#[derive(Serialize)]
pub(crate) struct EngineResponse {
    instance: EngineInstance,
}

#[derive(Serialize)]
pub(crate) struct ConfigsResponse {
    configs: Vec<EngineConfig>,
}

#[derive(Serialize)]
pub(crate) struct ConfigResponse {
    config: EngineConfig,
}

#[derive(Serialize)]
pub(crate) struct DeleteResponse {
    ok: bool,
}

#[derive(Serialize)]
pub(crate) struct LogHistoryResponse {
    entries: Vec<LogEntry>,
}

#[derive(Serialize)]
pub(crate) struct StatusHistoryResponse {
    entries: Vec<EngineInstance>,
}

#[derive(Serialize)]
pub(crate) struct HardwareResponse {
    hardware: HardwareInfo,
}

#[derive(Serialize)]
pub(crate) struct HardwareInfo {
    hostname: Option<String>,
    os_name: Option<String>,
    os_version: Option<String>,
    kernel_version: Option<String>,
    cpu_brand: Option<String>,
    cpu_cores_logical: usize,
    cpu_cores_physical: Option<usize>,
    cpu_frequency_mhz: Option<u64>,
    memory_total_kb: u64,
    memory_available_kb: u64,
    swap_total_kb: u64,
    swap_free_kb: u64,
    uptime_seconds: u64,
    gpus: Vec<GpuInfo>,
}

#[derive(Serialize)]
pub(crate) struct GpuInfo {
    name: Option<String>,
    vendor: Option<String>,
    memory_total_mb: Option<u64>,
    driver_version: Option<String>,
}

fn collect_gpus() -> Vec<GpuInfo> {
    if let Some(gpus) = collect_nvidia_gpus() {
        return gpus;
    }

    if let Some(gpus) = collect_lspci_gpus() {
        return gpus;
    }

    Vec::new()
}

fn collect_nvidia_gpus() -> Option<Vec<GpuInfo>> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();
    for line in text.lines().map(|line| line.trim()).filter(|line| !line.is_empty()) {
        let mut parts = line.split(',').map(|part| part.trim());
        let name = parts.next().map(|value| value.to_string()).filter(|v| !v.is_empty());
        let memory_total_mb = parts
            .next()
            .and_then(|value| value.parse::<u64>().ok());
        let driver_version =
            parts.next().map(|value| value.to_string()).filter(|v| !v.is_empty());

        gpus.push(GpuInfo {
            name,
            vendor: Some("NVIDIA".to_string()),
            memory_total_mb,
            driver_version,
        });
    }

    Some(gpus)
}

fn collect_lspci_gpus() -> Option<Vec<GpuInfo>> {
    let output = Command::new("lspci").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();
    for line in text.lines() {
        if !(line.contains("VGA compatible controller")
            || line.contains("3D controller")
            || line.contains("Display controller"))
        {
            continue;
        }

        let name = line
            .splitn(2, ':')
            .nth(1)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if name.is_none() {
            continue;
        }

        gpus.push(GpuInfo {
            name,
            vendor: None,
            memory_total_mb: None,
            driver_version: None,
        });
    }

    Some(gpus)
}
