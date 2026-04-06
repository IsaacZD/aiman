//! Dashboard-specific data types.

use serde::{Deserialize, Serialize};

/// Host configuration for a remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_libraries: Option<Vec<String>>,
}

/// TOML hosts file format.
#[derive(Debug, Deserialize)]
pub struct HostsFile {
    #[serde(default)]
    pub host: Option<Vec<HostConfig>>,
}

/// Benchmark request payload from the UI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct BenchmarkPayload {
    pub model: Option<String>,
    pub api_base_url: Option<String>,
    #[serde(default)]
    pub pp: Vec<i32>,
    #[serde(default)]
    pub tg: Vec<i32>,
    #[serde(default)]
    pub depth: Vec<i32>,
    pub runs: Option<i32>,
    #[serde(default)]
    pub concurrency: Vec<i32>,
    pub prefix_caching: Option<bool>,
    pub latency_mode: Option<String>,
}

/// Benchmark settings stored in the record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSettings {
    pub model: String,
    pub api_base_url: String,
    pub pp: Vec<i32>,
    pub tg: Vec<i32>,
    pub depth: Vec<i32>,
    pub runs: i32,
    pub concurrency: Vec<i32>,
    pub prefix_caching: bool,
    pub latency_mode: String,
}

/// Host snapshot included in benchmark records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSnapshot {
    pub id: String,
    pub name: String,
    pub base_url: String,
}

/// A benchmark record stored in JSONL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    pub id: String,
    pub ts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<HostSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_hardware: Option<aiman_shared::HardwareInfo>,
    pub engine_config: aiman_shared::EngineConfig,
    pub engine_status: String,
    pub settings: BenchmarkSettings,
    /// Raw llama-benchy stdout (markdown table).
    pub output: String,
}
