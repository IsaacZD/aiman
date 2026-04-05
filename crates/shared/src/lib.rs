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
    #[serde(default)]
    pub container: Option<ContainerConfig>,
}

/// Environment variable injection for a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

/// Supported engine types (extend as needed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    Container,
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
    pub session_id: String,
    pub stream: LogStream,
    pub line: String,
}

/// Log stream origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// A log session bounded by engine start/stop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSession {
    pub id: String,
    pub started_at: String,
    pub stopped_at: Option<String>,
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

/// Container build settings for containerized engines.
/// The agent creates a temporary directory, writes `dockerfile_content` as
/// `Dockerfile` there, and uses that directory as the build context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ContainerBuild {
    pub dockerfile_content: Option<String>,
    pub build_args: Vec<EnvVar>,
    pub pull: bool,
    pub no_cache: bool,
}

/// Stored image template shared across container-backed engine configs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ContainerImage {
    pub id: String,
    pub name: String,
    pub image: String,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
    pub env: Vec<EnvVar>,
    /// Deprecated — GPU access is configured via `gpus` instead.
    pub run_args: Vec<String>,
    /// GPU device access via CDI: "all", "0", "0,1".
    /// Maps to podman `--device nvidia.com/gpu=<value>`.
    pub gpus: Option<String>,
    pub user: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub pull: bool,
    pub remove: bool,
    pub build: Option<ContainerBuild>,
}

/// Container runtime settings for containerized engines.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContainerConfig {
    pub container_name: Option<String>,
    pub image_id: String,
    pub extra_ports: Vec<String>,
    pub extra_volumes: Vec<String>,
    pub extra_env: Vec<EnvVar>,
    /// Deprecated — use `gpus` on the image instead.
    pub extra_run_args: Vec<String>,
    /// Override image-level GPU setting for this engine instance.
    pub gpus: Option<String>,
    pub user: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub pull: Option<bool>,
    pub remove: Option<bool>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            container_name: None,
            image_id: String::new(),
            extra_ports: Vec::new(),
            extra_volumes: Vec::new(),
            extra_env: Vec::new(),
            extra_run_args: Vec::new(),
            gpus: None,
            user: None,
            command: None,
            args: Vec::new(),
            pull: None,
            remove: None,
        }
    }
}
