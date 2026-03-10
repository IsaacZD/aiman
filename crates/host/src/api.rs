use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use aiman_shared::{EngineInstance, LogEntry};

use crate::state::AppState;
use crate::supervisor::{map_supervisor_error, read_jsonl};

pub async fn health() -> &'static str {
    "ok"
}

pub async fn list_engines(State(state): State<AppState>) -> Json<EnginesResponse> {
    Json(EnginesResponse {
        engines: state.supervisor.list_instances().await,
    })
}

pub async fn get_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
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
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(ws.on_upgrade(move |socket| async move {
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

#[derive(Serialize)]
pub(crate) struct EnginesResponse {
    engines: Vec<EngineInstance>,
}

#[derive(Serialize)]
pub(crate) struct EngineResponse {
    instance: EngineInstance,
}

#[derive(Serialize)]
pub(crate) struct LogHistoryResponse {
    entries: Vec<LogEntry>,
}

#[derive(Serialize)]
pub(crate) struct StatusHistoryResponse {
    entries: Vec<EngineInstance>,
}
