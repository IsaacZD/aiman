use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse},
    Json,
};
use serde_json::json;
use futures_util::{stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio::sync::broadcast;

use aiman_shared::{
    DockerImage, EngineConfig, EngineInstance, EngineStatus, EngineType, LogEntry, LogSession,
};

use crate::benchmark::{run_benchmark, BenchmarkRecord, BenchmarkRequest};
use crate::hardware::HardwareInfo;
use crate::models::scan_model_libraries;
use crate::state::AppState;
use crate::supervisor::{map_supervisor_error, read_log_entries, read_log_sessions, read_jsonl};

pub async fn health() -> &'static str {
    "ok"
}

pub async fn hardware_info(State(state): State<AppState>) -> Json<HardwareResponse> {
    let hardware = state.hardware_cache.lock().await.get().await;
    Json(HardwareResponse { hardware })
}

/// SSE stream that pushes engine status changes and periodic hardware snapshots.
/// Clients subscribe once and receive push updates without polling.
pub async fn events_sse(State(state): State<AppState>) -> impl IntoResponse {
    let status_rx = state.supervisor.subscribe_status();
    let hardware_rx = state.supervisor.subscribe_hardware();

    let event_stream = stream::unfold(
        (status_rx, hardware_rx),
        |(mut srx, mut hrx)| async move {
            loop {
                tokio::select! {
                    result = srx.recv() => {
                        match result {
                            Ok(instance) => {
                                let data = json!({"type": "engine_status", "instance": instance}).to_string();
                                return Some((Ok::<Event, Infallible>(Event::default().data(data)), (srx, hrx)));
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                    result = hrx.recv() => {
                        match result {
                            Ok(hw) => {
                                let data = json!({"type": "hardware", "hardware": hw}).to_string();
                                return Some((Ok::<Event, Infallible>(Event::default().data(data)), (srx, hrx)));
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                }
            }
        },
    );

    Sse::new(event_stream).keep_alive(KeepAlive::default())
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
    session_id: Option<String>,
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
        session_id = query.session_id.as_deref(),
        "engine logs requested"
    );
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let entries = read_log_entries(
        &handle.log_path,
        query.since.as_deref(),
        query.limit,
        query.session_id.as_deref(),
    )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LogHistoryResponse { entries }))
}

pub async fn engine_log_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogQuery>,
) -> Result<Json<LogSessionsResponse>, StatusCode> {
    tracing::debug!(
        engine_id = %id,
        limit = query.limit,
        "engine log sessions requested"
    );
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let sessions = read_log_sessions(&handle.session_path, query.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LogSessionsResponse { sessions }))
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

    let BenchmarkRequest { mut settings, host } = request;
    if settings.api_base_url.is_none() && matches!(config.engine_type, EngineType::Docker) {
        if let Some(docker) = config.docker.as_ref() {
            if let Some(image) = state.supervisor.get_image(&docker.image_id).await {
                if let Some(api_base_url) = infer_docker_api_base(&config, &image) {
                    settings.api_base_url = Some(api_base_url);
                }
            }
        }
    }

    let hardware = state.hardware_cache.lock().await.get().await;
    let record = run_benchmark(config, instance, host, Some(hardware), settings)
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

fn infer_docker_api_base(config: &EngineConfig, image: &DockerImage) -> Option<String> {
    let docker_args = config
        .docker
        .as_ref()
        .map(|docker| docker.args.as_slice())
        .unwrap_or(&[]);
    let mut host = parse_arg_value(docker_args, "--host")
        .or_else(|| parse_arg_value(docker_args, "--bind"))
        .or_else(|| parse_arg_value(&config.args, "--host"))
        .or_else(|| parse_arg_value(&config.args, "--bind"))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let ports = match &config.docker {
        Some(docker) => {
            let mut ports = image.ports.clone();
            ports.extend(docker.extra_ports.clone());
            ports
        }
        None => image.ports.clone(),
    };
    let port = parse_arg_value(docker_args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| parse_docker_host_port(&ports))
        .unwrap_or(8000);
    if host == "0.0.0.0" || host == "::" {
        host = "127.0.0.1".to_string();
    }
    Some(format!("http://{host}:{port}"))
}

fn parse_arg_value(args: &[String], key: &str) -> Option<String> {
    for (idx, value) in args.iter().enumerate() {
        if value == key {
            return args.get(idx + 1).cloned();
        }
        if let Some(stripped) = value.strip_prefix(&format!("{key}=")) {
            return Some(stripped.to_string());
        }
    }
    None
}

fn parse_docker_host_port(ports: &[String]) -> Option<u16> {
    for mapping in ports {
        if let Some(port) = parse_docker_port_mapping(mapping) {
            return Some(port);
        }
    }
    None
}

fn parse_docker_port_mapping(mapping: &str) -> Option<u16> {
    let trimmed = mapping.trim();
    if trimmed.is_empty() {
        return None;
    }
    let no_proto = trimmed.split('/').next().unwrap_or(trimmed).trim();
    let parts: Vec<&str> = no_proto.split(':').collect();
    let host_port = if parts.len() >= 2 {
        parts.get(parts.len() - 2).copied()
    } else {
        parts.first().copied()
    }?;
    host_port.trim().parse::<u16>().ok()
}

pub async fn list_configs(State(state): State<AppState>) -> Json<ConfigsResponse> {
    tracing::debug!("list configs requested");
    Json(ConfigsResponse {
        configs: state.supervisor.list_configs().await,
    })
}

pub async fn list_images(State(state): State<AppState>) -> Json<ImagesResponse> {
    tracing::debug!("list images requested");
    Json(ImagesResponse {
        images: state.supervisor.list_images().await,
    })
}

pub async fn get_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ImageResponse>, StatusCode> {
    tracing::debug!(image_id = %id, "get image requested");
    let image = state
        .supervisor
        .get_image(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ImageResponse { image }))
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

pub async fn create_image(
    State(state): State<AppState>,
    Json(image): Json<DockerImage>,
) -> Result<Json<ImageResponse>, StatusCode> {
    tracing::info!(image_id = %image.id, "create image API called");
    let image = state
        .supervisor
        .add_image(image)
        .await
        .map_err(map_supervisor_error)?;
    Ok(Json(ImageResponse { image }))
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

pub async fn update_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(image): Json<DockerImage>,
) -> Result<Json<ImageResponse>, StatusCode> {
    tracing::info!(image_id = %id, "update image API called");
    let image = state
        .supervisor
        .update_image(&id, image)
        .await
        .map_err(map_supervisor_error)?;
    Ok(Json(ImageResponse { image }))
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

pub async fn prune_images(State(state): State<AppState>) -> impl IntoResponse {
    match state.supervisor.prune_images().await {
        Ok(removed) => Json(json!({ "removed": removed })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_image(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, StatusCode> {
    tracing::info!(image_id = %id, "delete image API called");
    state
        .supervisor
        .remove_image(&id)
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
pub(crate) struct ImagesResponse {
    images: Vec<DockerImage>,
}

#[derive(Serialize)]
pub(crate) struct ImageResponse {
    image: DockerImage,
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
pub(crate) struct LogSessionsResponse {
    sessions: Vec<LogSession>,
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
