//! AI Model Manager Dashboard Backend
//!
//! A Rust backend for the dashboard that proxies requests to host agents
//! and provides aggregation, benchmarking, and real-time streaming.

mod api;
mod error;
mod hosts;
mod state;
mod types;

use std::path::PathBuf;

use axum::routing::{get, post, put};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

use hosts::load_hosts;
use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap_or_else(|_| "info".parse().expect("valid directive"))),
        )
        .init();

    // Resolve paths from environment or defaults
    let repo_root = std::env::current_dir()?;
    let hosts_config_path = std::env::var("AIMAN_HOSTS_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("configs/hosts.toml"));
    let hosts_store_path = std::env::var("AIMAN_HOSTS_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("data/hosts.json"));
    let benchmarks_path = std::env::var("AIMAN_DASHBOARD_BENCHMARKS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("data/benchmarks-dashboard.jsonl"));
    let ui_dir = std::env::var("AIMAN_DASHBOARD_UI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("dashboard/dist/ui"));

    let port: u16 = std::env::var("AIMAN_DASHBOARD_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4020);
    let bind = std::env::var("AIMAN_DASHBOARD_BIND").unwrap_or_else(|_| "0.0.0.0".to_string());

    // Load initial hosts
    let initial_hosts = load_hosts(&hosts_store_path, &hosts_config_path).await?;
    tracing::info!(count = initial_hosts.len(), "loaded hosts");

    // Create app state
    let state = AppState::new(
        initial_hosts,
        hosts_store_path,
        hosts_config_path,
        benchmarks_path,
        ui_dir.clone(),
    );

    // Build router
    let app = Router::new()
        // Health check
        .route("/health", get(health))
        // Host management
        .route("/api/hosts", get(api::hosts::list_hosts).post(api::hosts::create_host))
        .route(
            "/api/hosts/{hostId}",
            put(api::hosts::update_host).delete(api::hosts::delete_host),
        )
        // Proxied per-host routes
        .route("/api/hosts/{hostId}/models", get(api::proxy::proxy_models))
        .route("/api/hosts/{hostId}/hardware", get(api::proxy::proxy_hardware))
        .route(
            "/api/hosts/{hostId}/configs",
            get(api::proxy::proxy_configs).post(api::proxy::create_config),
        )
        .route(
            "/api/hosts/{hostId}/configs/{configId}",
            put(api::proxy::update_config).delete(api::proxy::delete_config),
        )
        .route(
            "/api/hosts/{hostId}/images",
            get(api::proxy::proxy_images).post(api::proxy::create_image),
        )
        .route(
            "/api/hosts/{hostId}/images/{imageId}",
            put(api::proxy::update_image).delete(api::proxy::delete_image),
        )
        .route("/api/hosts/{hostId}/images/{imageId}/prepare", post(api::proxy::prepare_image))
        .route("/api/hosts/{hostId}/images/prune", post(api::proxy::prune_images))
        // Engine control
        .route("/api/hosts/{hostId}/engines/{engineId}/start", post(api::proxy::start_engine))
        .route("/api/hosts/{hostId}/engines/{engineId}/stop", post(api::proxy::stop_engine))
        .route("/api/hosts/{hostId}/engines/{engineId}/logs", get(api::proxy::proxy_logs))
        .route(
            "/api/hosts/{hostId}/engines/{engineId}/logs/sessions",
            get(api::proxy::proxy_log_sessions),
        )
        .route("/api/hosts/{hostId}/engines/{engineId}/status", get(api::proxy::proxy_status))
        // Streaming
        .route("/api/hosts/{host_id}/events", get(api::streaming::sse_bridge))
        .route("/api/hosts/{hostId}/engines/{engineId}/logs/ws", get(api::streaming::ws_bridge))
        // Aggregation
        .route("/api/engines", get(api::aggregation::aggregate_engines))
        .route("/api/benchmarks", get(api::aggregation::aggregate_benchmarks))
        // Benchmark execution
        .route(
            "/api/hosts/{hostId}/engines/{engineId}/benchmark",
            post(api::benchmark::run_benchmark),
        )
        // State
        .with_state(state)
        // Static UI files
        .fallback_service(
            ServeDir::new(&ui_dir).fallback(ServeFile::new(ui_dir.join("index.html"))),
        );

    // Start server
    let addr = format!("{bind}:{port}");
    tracing::info!(addr = %addr, "starting dashboard server");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}
