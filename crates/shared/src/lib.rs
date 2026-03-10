use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineType {
    Vllm,
    LlamaCpp,
    KTransformers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineStatus {
    Starting,
    Running,
    Stopped,
    Error,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub ts: String,
    pub stream: LogStream,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogStream {
    Stdout,
    Stderr,
}

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
