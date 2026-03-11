//! Shared data contracts between agent and dashboard.
//! Keep this crate dependency-light so both sides can reuse it for schemas.
//! Design note: engine_type strings are consumed by both the Rust agent and the
//! dashboard UI, so renames should be additive and serialized names should be
//! explicit when the on-wire value differs from the Rust enum variant.

use serde::{Deserialize, Serialize};

/// Engine configuration preset (one config = one runnable instance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub id: String,
    pub name: String,
    pub engine_type: EngineType,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<EnvVar>,
    pub working_dir: Option<String>,
    pub auto_restart: AutoRestart,
}

/// Environment variable injection for a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

/// Supported engine types (extend as needed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineType {
    Vllm,
    LlamaCpp,
    #[serde(rename = "ik_llamacpp")]
    IkLlamaCpp,
    Lvllm,
    #[serde(rename = "fastllm")]
    Fastllm,
    KTransformers,
    Custom,
}

/// Runtime status of an engine instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EngineStatus {
    Starting,
    Running,
    Stopped,
    Error,
}

/// Snapshot of a running or stopped engine instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInstance {
    pub id: String,
    pub config_id: String,
    pub status: EngineStatus,
    pub pid: Option<u32>,
    pub started_at: Option<String>,
    pub last_exit_at: Option<String>,
    pub last_exit_code: Option<i32>,
    pub health: Option<String>,
}

/// One log line emitted by an engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub ts: String,
    pub stream: LogStream,
    pub line: String,
}

/// Log stream origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// Optional restart policy for crashed processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoRestart {
    pub enabled: bool,
    pub max_retries: u32,
    pub backoff_secs: u64,
}

impl Default for AutoRestart {
    fn default() -> Self {
        Self {
            enabled: false,
            max_retries: 0,
            backoff_secs: 5,
        }
    }
}
