mod api;
mod auth;
mod state;
mod supervisor;

use std::path::PathBuf;
use std::sync::Arc;

use axum::{middleware, routing::get, routing::post, Router};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::api::{
    engine_logs, engine_logs_ws, engine_status_history, get_engine, health, list_engines,
    start_engine, stop_engine,
};
use crate::auth::auth_middleware;
use crate::state::AppState;
use crate::supervisor::Supervisor;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load engine configs (defaults to repo-relative path).
    let config_path = std::env::var("AIMAN_ENGINES_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("configs/engines.toml"));

    // Data dir holds JSONL logs/status history.
    let data_dir = std::env::var("AIMAN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data"));

    let supervisor = Supervisor::from_file(config_path, data_dir)
        .await
        .expect("load engine config");

    let api_key = std::env::var("AIMAN_API_KEY").ok();
    let state = AppState {
        supervisor: Arc::new(supervisor),
        api_key,
    };

    // HTTP API surface for control + observability.
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/engines", get(list_engines))
        .route("/v1/engines/{id}", get(get_engine))
        .route("/v1/engines/{id}/start", post(start_engine))
        .route("/v1/engines/{id}/stop", post(stop_engine))
        .route("/v1/engines/{id}/logs", get(engine_logs))
        .route("/v1/engines/{id}/logs/ws", get(engine_logs_ws))
        .route("/v1/engines/{id}/status", get(engine_status_history))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    let bind_addr = std::env::var("AIMAN_BIND").unwrap_or_else(|_| "0.0.0.0:4010".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("bind host listener");

    tracing::info!("aiman-host listening on {bind_addr}");
    axum::serve(listener, app).await.expect("serve host");
}
