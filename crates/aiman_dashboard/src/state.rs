//! Application state for the dashboard.

use std::path::PathBuf;
use std::sync::Arc;

use aiman_shared::http::ProxyClient;
use aiman_shared::storage::LogWriter;
use tokio::sync::RwLock;

use crate::types::HostConfig;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Host configurations (cached in memory, persisted to disk).
    pub hosts: Arc<RwLock<Vec<HostConfig>>>,
    /// HTTP client for proxying requests.
    pub proxy_client: ProxyClient,
    /// Path to the JSON hosts store.
    pub hosts_store_path: PathBuf,
    /// Path to the TOML hosts config (fallback).
    pub hosts_config_path: PathBuf,
    /// Path to the benchmarks JSONL file.
    pub benchmarks_path: PathBuf,
    /// Writer for benchmark records.
    pub benchmark_writer: LogWriter,
    /// Path to the built UI directory.
    pub ui_dir: PathBuf,
}

impl AppState {
    /// Create a new application state.
    #[must_use]
    pub fn new(
        hosts: Vec<HostConfig>,
        hosts_store_path: PathBuf,
        hosts_config_path: PathBuf,
        benchmarks_path: PathBuf,
        ui_dir: PathBuf,
    ) -> Self {
        let benchmark_writer = LogWriter::new(benchmarks_path.clone());
        Self {
            hosts: Arc::new(RwLock::new(hosts)),
            proxy_client: ProxyClient::new(),
            hosts_store_path,
            hosts_config_path,
            benchmarks_path,
            benchmark_writer,
            ui_dir,
        }
    }
}
