mod api;
mod auth;
mod benchmark;
mod hardware;
mod models;
mod state;
mod supervisor;

use std::path::PathBuf;
use std::sync::Arc;

use axum::{middleware, routing::get, routing::post, routing::put, Router};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::api::{
    benchmark_engine, create_config, delete_config, engine_logs, engine_logs_ws,
    engine_status_history, get_engine, hardware_info, health, list_benchmarks, list_configs,
    list_engines, scan_models, start_engine, stop_engine, update_config,
};
use crate::auth::auth_middleware;
use crate::hardware::HardwareCache;
use crate::state::AppState;
use crate::supervisor::Supervisor;

fn main() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let worker_threads = std::env::var("AIMAN_TOKIO_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0);

    let mut runtime_builder = tokio::runtime::Builder::new_multi_thread();
    runtime_builder.enable_all().thread_name("aiman-agent");
    if let Some(count) = worker_threads {
        runtime_builder.worker_threads(count);
    }

    let runtime = runtime_builder
        .build()
        .expect("build tokio runtime");

    runtime.block_on(async {
    // Data dir holds JSONL logs/status history + config store.
    let data_dir = std::env::var("AIMAN_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data"));

    // Config store defaults to data/configs.json; seed optional from TOML if provided.
    let config_path = std::env::var("AIMAN_CONFIG_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_dir.join("configs.json"));
    let seed_path = std::env::var("AIMAN_ENGINES_CONFIG").ok().map(PathBuf::from);

    tracing::info!(
        data_dir = %data_dir.display(),
        config_path = %config_path.display(),
        seed_path = seed_path.as_ref().map(|path| path.display().to_string()),
        "host paths configured"
    );

    let supervisor = Supervisor::from_store(config_path, data_dir, seed_path)
        .await
        .expect("load engine config store");

    let api_key = std::env::var("AIMAN_API_KEY").ok();
    if api_key.is_some() {
        tracing::info!("auth enabled (AIMAN_API_KEY set)");
    } else {
        tracing::info!("auth disabled (AIMAN_API_KEY unset)");
    }
    let state = AppState {
        supervisor: Arc::new(supervisor),
        api_key,
        hardware_cache: Arc::new(tokio::sync::Mutex::new(HardwareCache::from_env())),
    };

    // HTTP API surface for control + observability.
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/hardware", get(hardware_info))
        .route("/v1/configs", get(list_configs).post(create_config))
        .route("/v1/configs/{id}", put(update_config).delete(delete_config))
        .route("/v1/engines", get(list_engines))
        .route("/v1/engines/{id}", get(get_engine))
        .route("/v1/engines/{id}/start", post(start_engine))
        .route("/v1/engines/{id}/stop", post(stop_engine))
        .route("/v1/engines/{id}/logs", get(engine_logs))
        .route("/v1/engines/{id}/logs/ws", get(engine_logs_ws))
        .route("/v1/engines/{id}/status", get(engine_status_history))
        .route("/v1/engines/{id}/benchmark", post(benchmark_engine))
        .route("/v1/benchmarks", get(list_benchmarks))
        .route("/v1/models/scan", post(scan_models))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    let bind_addr = std::env::var("AIMAN_BIND").unwrap_or_else(|_| "0.0.0.0:4010".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("bind host listener");

    tracing::info!("aiman_agent listening on {bind_addr}");
    axum::serve(listener, app).await.expect("serve host");
    });
}
