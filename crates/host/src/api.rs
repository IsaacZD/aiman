use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use aiman_shared::{EngineConfig, EngineInstance, EngineStatus, LogEntry};

use crate::benchmark::{run_benchmark, BenchmarkRecord, BenchmarkRequest};
use crate::hardware::{collect_hardware_info, HardwareInfo};
use crate::models::scan_model_libraries;
use crate::state::AppState;
use crate::supervisor::{map_supervisor_error, read_jsonl};

pub async fn health() -> &'static str {
    "ok"
}

pub async fn hardware_info() -> Json<HardwareResponse> {
    let hardware = collect_hardware_info().await;
    Json(HardwareResponse { hardware })
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

pub async fn benchmark_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<BenchmarkRequest>,
) -> Result<Json<BenchmarkResponse>, StatusCode> {
    tracing::info!(engine_id = %id, "benchmark requested");
    let instance = state
        .supervisor
        .get_instance(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    if instance.status != EngineStatus::Running {
        return Err(StatusCode::CONFLICT);
    }

    let config = state
        .supervisor
        .get_config(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let hardware = collect_hardware_info().await;
    let record = run_benchmark(config, instance, request.host, Some(hardware), request.settings)
        .await
        .map_err(|err| {
            tracing::error!(engine_id = %id, error = %err, "benchmark failed");
            StatusCode::BAD_REQUEST
        })?;

    state.supervisor.append_benchmark(&record).await;
    Ok(Json(BenchmarkResponse { record }))
}

#[derive(Deserialize)]
pub(crate) struct BenchmarkQuery {
    since: Option<String>,
    limit: Option<usize>,
}

pub async fn list_benchmarks(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<BenchmarkQuery>,
) -> Result<Json<BenchmarksResponse>, StatusCode> {
    let entries =
        read_jsonl(state.supervisor.benchmark_path(), query.since.as_deref(), query.limit)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(BenchmarksResponse { records: entries }))
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
pub(crate) struct BenchmarkResponse {
    record: BenchmarkRecord,
}

#[derive(Serialize)]
pub(crate) struct BenchmarksResponse {
    records: Vec<BenchmarkRecord>,
}
