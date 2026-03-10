use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use aiman_shared::{AutoRestart, EngineConfig, EngineInstance, EngineStatus, LogEntry, LogStream};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, watch, Mutex, RwLock},
    task::JoinHandle,
};

// Keep a bounded in-memory log buffer per engine for WS backfill.
const LOG_BUFFER_MAX: usize = 2000;

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("engine not found")]
    NotFound,
    #[error("engine already running")]
    AlreadyRunning,
    #[error("engine not running")]
    NotRunning,
    #[error("engine config already exists")]
    ConfigExists,
    #[error("engine config in use")]
    ConfigInUse,
    #[error("engine config invalid: {0}")]
    ConfigInvalid(String),
}

pub fn map_supervisor_error(err: SupervisorError) -> axum::http::StatusCode {
    match err {
        SupervisorError::NotFound => axum::http::StatusCode::NOT_FOUND,
        SupervisorError::AlreadyRunning => axum::http::StatusCode::CONFLICT,
        SupervisorError::NotRunning => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigExists => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigInUse => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigInvalid(_) => axum::http::StatusCode::BAD_REQUEST,
    }
}

#[derive(Clone)]
// Supervisor holds engine handles and mediates lifecycle control.
pub struct Supervisor {
    config_path: PathBuf,
    data_dir: PathBuf,
    configs: Arc<RwLock<HashMap<String, EngineConfig>>>,
    handles: Arc<RwLock<HashMap<String, Arc<EngineHandle>>>>,
}

impl Supervisor {
    pub async fn from_store(
        config_path: PathBuf,
        data_dir: PathBuf,
        seed_path: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        tracing::info!(
            config_path = %config_path.display(),
            data_dir = %data_dir.display(),
            seed_path = seed_path.as_ref().map(|path| path.display().to_string()),
            "loading engine config store"
        );
        // Ensure persistence directories exist.
        tokio::fs::create_dir_all(data_dir.join("logs")).await?;
        tokio::fs::create_dir_all(data_dir.join("status")).await?;
        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let configs_vec = load_config_store(&config_path, seed_path.as_ref()).await?;
        tracing::info!(count = configs_vec.len(), "loaded engine configs");
        let mut configs = HashMap::new();
        let mut handles = HashMap::new();

        for config in configs_vec {
            let id = config.id.clone();
            tracing::debug!(engine_id = %id, "registering engine config");
            configs.insert(id.clone(), config.clone());
            handles.insert(
                id.clone(),
                Arc::new(EngineHandle::new(
                    config,
                    data_dir.join("logs").join(format!("{id}.jsonl")),
                    data_dir.join("status").join(format!("{id}.jsonl")),
                )),
            );
        }

        Ok(Self {
            config_path,
            data_dir,
            configs: Arc::new(RwLock::new(configs)),
            handles: Arc::new(RwLock::new(handles)),
        })
    }

    pub async fn list_instances(&self) -> Vec<EngineInstance> {
        let handles = self.handles.read().await;
        let mut instances = Vec::with_capacity(handles.len());
        for handle in handles.values() {
            instances.push(handle.instance.read().await.clone());
        }
        instances
    }

    pub async fn get_instance(&self, id: &str) -> Option<EngineInstance> {
        let handles = self.handles.read().await;
        let handle = handles.get(id)?.clone();
        let instance = handle.instance.read().await.clone();
        Some(instance)
    }

    pub async fn get_handle(&self, id: &str) -> Option<Arc<EngineHandle>> {
        let handles = self.handles.read().await;
        handles.get(id).cloned()
    }

    pub async fn list_configs(&self) -> Vec<EngineConfig> {
        let configs = self.configs.read().await;
        let mut values: Vec<_> = configs.values().cloned().collect();
        values.sort_by(|a, b| a.id.cmp(&b.id));
        values
    }

    pub async fn add_config(&self, config: EngineConfig) -> Result<EngineConfig, SupervisorError> {
        validate_config(&config)?;
        let mut configs = self.configs.write().await;
        if configs.contains_key(&config.id) {
            return Err(SupervisorError::ConfigExists);
        }
        let mut handles = self.handles.write().await;

        let id = config.id.clone();
        configs.insert(id.clone(), config.clone());
        handles.insert(
            id.clone(),
            Arc::new(EngineHandle::new(
                config.clone(),
                self.data_dir.join("logs").join(format!("{id}.jsonl")),
                self.data_dir.join("status").join(format!("{id}.jsonl")),
            )),
        );

        persist_config_store(&self.config_path, &configs).await;
        tracing::info!(engine_id = %id, "added engine config");
        Ok(config)
    }

    pub async fn update_config(
        &self,
        id: &str,
        config: EngineConfig,
    ) -> Result<EngineConfig, SupervisorError> {
        validate_config(&config)?;
        if id != config.id {
            return Err(SupervisorError::ConfigInvalid(
                "config id mismatch".to_string(),
            ));
        }

        let mut configs = self.configs.write().await;
        let mut handles = self.handles.write().await;

        let handle = handles.get(id).cloned().ok_or(SupervisorError::NotFound)?;
        if handle_is_running(&handle).await {
            return Err(SupervisorError::ConfigInUse);
        }

        configs.insert(id.to_string(), config.clone());
        handles.insert(
            id.to_string(),
            Arc::new(EngineHandle::new(
                config.clone(),
                self.data_dir.join("logs").join(format!("{id}.jsonl")),
                self.data_dir.join("status").join(format!("{id}.jsonl")),
            )),
        );

        persist_config_store(&self.config_path, &configs).await;
        tracing::info!(engine_id = %id, "updated engine config");
        Ok(config)
    }

    pub async fn remove_config(&self, id: &str) -> Result<(), SupervisorError> {
        let mut configs = self.configs.write().await;
        let mut handles = self.handles.write().await;
        let handle = handles.get(id).cloned().ok_or(SupervisorError::NotFound)?;
        if handle_is_running(&handle).await {
            return Err(SupervisorError::ConfigInUse);
        }
        configs.remove(id);
        handles.remove(id);
        persist_config_store(&self.config_path, &configs).await;
        tracing::info!(engine_id = %id, "removed engine config");
        Ok(())
    }

    pub async fn start(&self, id: &str) -> Result<EngineInstance, SupervisorError> {
        tracing::info!(engine_id = %id, "start requested");
        let handle = self.get_handle(id).await.ok_or(SupervisorError::NotFound)?;
        handle.start().await?;
        let instance = handle.instance.read().await.clone();
        Ok(instance)
    }

    pub async fn stop(&self, id: &str) -> Result<EngineInstance, SupervisorError> {
        tracing::info!(engine_id = %id, "stop requested");
        let handle = self.get_handle(id).await.ok_or(SupervisorError::NotFound)?;
        handle.stop().await?;
        let instance = handle.instance.read().await.clone();
        Ok(instance)
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    engine: Vec<EngineConfig>,
}

async fn load_config_store(
    path: &PathBuf,
    seed_path: Option<&PathBuf>,
) -> anyhow::Result<Vec<EngineConfig>> {
    if let Ok(raw) = tokio::fs::read_to_string(path).await {
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let configs: Vec<EngineConfig> = serde_json::from_str(&raw)?;
        return Ok(configs);
    }

    if let Some(seed_path) = seed_path {
        if let Ok(raw) = tokio::fs::read_to_string(seed_path).await {
            let parsed: ConfigFile = toml::from_str(&raw)?;
            let configs = parsed.engine;
            persist_config_store(path, &configs_to_map(&configs)).await;
            return Ok(configs);
        }
    }

    Ok(Vec::new())
}

fn configs_to_map(configs: &[EngineConfig]) -> HashMap<String, EngineConfig> {
    configs
        .iter()
        .cloned()
        .map(|config| (config.id.clone(), config))
        .collect()
}

async fn persist_config_store(path: &PathBuf, configs: &HashMap<String, EngineConfig>) {
    let mut values: Vec<_> = configs.values().cloned().collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    if let Ok(serialized) = serde_json::to_string_pretty(&values) {
        let _ = tokio::fs::write(path, serialized).await;
    }
}

fn validate_config(config: &EngineConfig) -> Result<(), SupervisorError> {
    if config.id.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid("id is required".to_string()));
    }
    if config.command.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid(
            "command is required".to_string(),
        ));
    }
    if config.name.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid("name is required".to_string()));
    }
    Ok(())
}

async fn handle_is_running(handle: &EngineHandle) -> bool {
    matches!(
        handle.instance.read().await.status,
        EngineStatus::Running | EngineStatus::Starting
    )
}

// Per-engine state + control handles.
pub struct EngineHandle {
    config: EngineConfig,
    instance: Arc<RwLock<EngineInstance>>,
    pub log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    pub log_tx: broadcast::Sender<LogEntry>,
    control: Mutex<EngineControl>,
    pub log_path: PathBuf,
    pub status_path: PathBuf,
    log_write_lock: Arc<Mutex<()>>,
    status_write_lock: Arc<Mutex<()>>,
}

// Control plane state for stopping/restarting a running task.
struct EngineControl {
    stop_tx: Option<watch::Sender<bool>>,
    task: Option<JoinHandle<()>>,
}

impl EngineHandle {
    fn new(config: EngineConfig, log_path: PathBuf, status_path: PathBuf) -> Self {
        let instance = EngineInstance {
            id: config.id.clone(),
            config_id: config.id.clone(),
            status: EngineStatus::Stopped,
            pid: None,
            started_at: None,
            last_exit_at: None,
            last_exit_code: None,
            health: None,
        };

        let (log_tx, _) = broadcast::channel(256);

        Self {
            config,
            instance: Arc::new(RwLock::new(instance)),
            log_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_BUFFER_MAX))),
            log_tx,
            control: Mutex::new(EngineControl {
                stop_tx: None,
                task: None,
            }),
            log_path,
            status_path,
            log_write_lock: Arc::new(Mutex::new(())),
            status_write_lock: Arc::new(Mutex::new(())),
        }
    }

    // Start the engine task if not running.
    async fn start(&self) -> Result<(), SupervisorError> {
        let mut control = self.control.lock().await;
        if control
            .task
            .as_ref()
            .map(|task| !task.is_finished())
            .unwrap_or(false)
        {
            tracing::warn!(engine_id = %self.config.id, "start skipped; already running");
            return Err(SupervisorError::AlreadyRunning);
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        control.stop_tx = Some(stop_tx);

        let handle = Arc::new(self.clone_for_task());
        tracing::info!(engine_id = %self.config.id, "spawning engine task");
        control.task = Some(tokio::spawn(async move {
            run_engine(handle, stop_rx).await;
        }));
        Ok(())
    }

    // Signal the task to stop.
    async fn stop(&self) -> Result<(), SupervisorError> {
        let control = self.control.lock().await;
        let Some(stop_tx) = &control.stop_tx else {
            tracing::warn!(engine_id = %self.config.id, "stop skipped; engine not running");
            return Err(SupervisorError::NotRunning);
        };
        let _ = stop_tx.send(true);
        Ok(())
    }

    fn clone_for_task(&self) -> EngineTaskHandle {
        EngineTaskHandle {
            config: self.config.clone(),
            instance: self.instance.clone(),
            log_buffer: self.log_buffer.clone(),
            log_tx: self.log_tx.clone(),
            log_path: self.log_path.clone(),
            status_path: self.status_path.clone(),
            log_write_lock: self.log_write_lock.clone(),
            status_write_lock: self.status_write_lock.clone(),
        }
    }
}

#[derive(Clone)]
// Lightweight clone passed into the async task.
struct EngineTaskHandle {
    config: EngineConfig,
    instance: Arc<RwLock<EngineInstance>>,
    log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    log_tx: broadcast::Sender<LogEntry>,
    log_path: PathBuf,
    status_path: PathBuf,
    log_write_lock: Arc<Mutex<()>>,
    status_write_lock: Arc<Mutex<()>>,
}

// Spawn and supervise the process with optional auto-restart.
async fn run_engine(handle: Arc<EngineTaskHandle>, mut stop_rx: watch::Receiver<bool>) {
    let mut retries = 0;

    loop {
        if *stop_rx.borrow() {
            tracing::info!(engine_id = %handle.config.id, "stop signal received before start");
            set_status(&handle, EngineStatus::Stopped, None, None).await;
            break;
        }

        tracing::info!(engine_id = %handle.config.id, "starting engine process");
        set_status(&handle, EngineStatus::Starting, None, None).await;

        match spawn_process(&handle.config).await {
            Ok(mut child) => {
                let pid = child.id();
                tracing::info!(
                    engine_id = %handle.config.id,
                    pid = pid,
                    "engine process spawned"
                );
                set_status(&handle, EngineStatus::Running, pid, Some(now())).await;

                let mut stdout_task = None;
                let mut stderr_task = None;

                if let Some(stdout) = child.stdout.take() {
                    stdout_task = Some(tokio::spawn(stream_logs(
                        handle.clone(),
                        BufReader::new(stdout),
                        LogStream::Stdout,
                    )));
                }
                if let Some(stderr) = child.stderr.take() {
                    stderr_task = Some(tokio::spawn(stream_logs(
                        handle.clone(),
                        BufReader::new(stderr),
                        LogStream::Stderr,
                    )));
                }

                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            tracing::info!(
                                engine_id = %handle.config.id,
                                pid = pid,
                                "stop signal received; terminating engine process"
                            );
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            set_status(&handle, EngineStatus::Stopped, None, None).await;
                            break;
                        }
                    }
                    status = child.wait() => {
                        let code = status.ok().and_then(|s| s.code());
                        tracing::info!(
                            engine_id = %handle.config.id,
                            pid = pid,
                            exit_code = code,
                            "engine process exited"
                        );
                        set_exit_status(&handle, code).await;
                    }
                }

                if let Some(task) = stdout_task {
                    let _ = task.await;
                }
                if let Some(task) = stderr_task {
                    let _ = task.await;
                }
            }
            Err(err) => {
                set_status(&handle, EngineStatus::Error, None, None).await;
                tracing::warn!(error = %err, "failed to spawn engine");
            }
        }

        if should_restart(&handle.config.auto_restart, retries) {
            retries += 1;
            tracing::info!(
                engine_id = %handle.config.id,
                attempt = retries,
                backoff_secs = handle.config.auto_restart.backoff_secs,
                "auto-restart scheduled"
            );
            tokio::time::sleep(Duration::from_secs(handle.config.auto_restart.backoff_secs)).await;
            continue;
        }

        tracing::info!(engine_id = %handle.config.id, "engine supervision loop exiting");
        break;
    }
}

// Build and spawn the engine child process.
async fn spawn_process(config: &EngineConfig) -> anyhow::Result<tokio::process::Child> {
    tracing::debug!(
        engine_id = %config.id,
        command = %config.command,
        args = ?config.args,
        working_dir = config.working_dir.as_deref(),
        env_count = config.env.len(),
        "spawning engine process"
    );
    let mut command = Command::new(&config.command);
    command.args(&config.args);

    if let Some(dir) = &config.working_dir {
        command.current_dir(dir);
    }

    for env in &config.env {
        command.env(&env.key, &env.value);
    }

    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    Ok(child)
}

// Read log lines, store to buffer + JSONL, and broadcast.
async fn stream_logs<R: tokio::io::AsyncRead + Unpin>(
    handle: Arc<EngineTaskHandle>,
    reader: BufReader<R>,
    stream: LogStream,
) {
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let entry = LogEntry {
            ts: now(),
            stream: stream.clone(),
            line,
        };

        {
            let mut buffer = handle.log_buffer.lock().await;
            if buffer.len() >= LOG_BUFFER_MAX {
                buffer.pop_front();
            }
            buffer.push_back(entry.clone());
        }

        append_jsonl(&handle.log_path, &handle.log_write_lock, &entry).await;
        let _ = handle.log_tx.send(entry);
    }
    tracing::debug!(
        engine_id = %handle.config.id,
        stream = ?stream,
        "log stream ended"
    );
}

// Update in-memory status and persist snapshot to JSONL.
async fn set_status(
    handle: &EngineTaskHandle,
    status: EngineStatus,
    pid: Option<u32>,
    started_at: Option<String>,
) {
    let mut instance = handle.instance.write().await;
    instance.status = status;
    if let Some(pid) = pid {
        instance.pid = Some(pid);
    }
    if let Some(started_at) = started_at {
        instance.started_at = Some(started_at);
    }
    let snapshot = instance.clone();
    drop(instance);
    append_jsonl(&handle.status_path, &handle.status_write_lock, &snapshot).await;
    tracing::debug!(
        engine_id = %handle.config.id,
        status = ?snapshot.status,
        pid = pid,
        "engine status updated"
    );
}

// Record a stop event with exit code.
async fn set_exit_status(handle: &EngineTaskHandle, code: Option<i32>) {
    let mut instance = handle.instance.write().await;
    instance.status = EngineStatus::Stopped;
    instance.pid = None;
    instance.last_exit_code = code;
    instance.last_exit_at = Some(now());
    let snapshot = instance.clone();
    drop(instance);
    append_jsonl(&handle.status_path, &handle.status_write_lock, &snapshot).await;
    tracing::debug!(
        engine_id = %handle.config.id,
        exit_code = code,
        "engine exit status recorded"
    );
}

fn should_restart(policy: &AutoRestart, retries: u32) -> bool {
    policy.enabled && retries < policy.max_retries
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

// Append a JSONL line with a simple mutex to avoid interleaving.
async fn append_jsonl<T: Serialize>(path: &PathBuf, lock: &Arc<Mutex<()>>, value: &T) {
    let _guard = lock.lock().await;
    if let Ok(line) = serde_json::to_string(value) {
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            use tokio::io::AsyncWriteExt;
            let _ = file.write_all(line.as_bytes()).await;
            let _ = file.write_all(b"\n").await;
        }
    }
}

// Read JSONL with optional since + limit filtering.
pub async fn read_jsonl<T: for<'de> Deserialize<'de>>(
    path: &PathBuf,
    since: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<Vec<T>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries: VecDeque<T> = VecDeque::new();
    let max = limit.unwrap_or(500);

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(entry) = serde_json::from_str::<T>(&line) {
            if let Some(since_dt) = since_dt {
                if let Some(ts) = extract_ts(&line) {
                    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&ts) {
                        if parsed < since_dt {
                            continue;
                        }
                    }
                }
            }

            if entries.len() >= max {
                entries.pop_front();
            }
            entries.push_back(entry);
        }
    }

    Ok(entries.into_iter().collect())
}

// Best-effort timestamp extraction for since filtering.
fn extract_ts(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("ts")?.as_str().map(|s| s.to_string())
}
