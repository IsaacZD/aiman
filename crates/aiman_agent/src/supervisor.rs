use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use aiman_shared::{
    AutoRestart, DockerBuild, DockerConfig, DockerImage, EngineConfig, EngineInstance, EngineStatus,
    EngineType, LogEntry, LogSession, LogStream,
};
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
    #[error("docker image already exists")]
    ImageExists,
    #[error("docker image in use")]
    ImageInUse,
    #[error("docker image not found")]
    ImageNotFound,
    #[error("docker image invalid: {0}")]
    ImageInvalid(String),
}

pub fn map_supervisor_error(err: SupervisorError) -> axum::http::StatusCode {
    match err {
        SupervisorError::NotFound => axum::http::StatusCode::NOT_FOUND,
        SupervisorError::AlreadyRunning => axum::http::StatusCode::CONFLICT,
        SupervisorError::NotRunning => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigExists => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigInUse => axum::http::StatusCode::CONFLICT,
        SupervisorError::ConfigInvalid(_) => axum::http::StatusCode::BAD_REQUEST,
        SupervisorError::ImageExists => axum::http::StatusCode::CONFLICT,
        SupervisorError::ImageInUse => axum::http::StatusCode::CONFLICT,
        SupervisorError::ImageNotFound => axum::http::StatusCode::NOT_FOUND,
        SupervisorError::ImageInvalid(_) => axum::http::StatusCode::BAD_REQUEST,
    }
}

#[derive(Clone)]
// Supervisor holds engine handles and mediates lifecycle control.
pub struct Supervisor {
    config_path: PathBuf,
    data_dir: PathBuf,
    configs: Arc<RwLock<HashMap<String, EngineConfig>>>,
    handles: Arc<RwLock<HashMap<String, Arc<EngineHandle>>>>,
    images: Arc<RwLock<HashMap<String, DockerImage>>>,
    images_path: PathBuf,
    benchmark_path: PathBuf,
    benchmark_write_lock: Arc<Mutex<()>>,
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
        let images_path = data_dir.join("docker-images.json");
        let images_vec = load_image_store(&images_path).await?;
        tracing::info!(count = images_vec.len(), "loaded docker images");
        let mut configs = HashMap::new();
        let mut handles = HashMap::new();
        let images = Arc::new(RwLock::new(images_to_map(&images_vec)));
        let benchmark_path = data_dir.join("benchmarks.jsonl");

        for config in configs_vec {
            let id = config.id.clone();
            tracing::debug!(engine_id = %id, "registering engine config");
            configs.insert(id.clone(), config.clone());
            handles.insert(
                id.clone(),
                Arc::new(EngineHandle::new(
                    config,
                    images.clone(),
                    data_dir.join("logs").join(format!("{id}.jsonl")),
                    data_dir.join("logs").join(format!("{id}-sessions.jsonl")),
                    data_dir.join("status").join(format!("{id}.jsonl")),
                )),
            );
        }

        Ok(Self {
            config_path,
            data_dir,
            configs: Arc::new(RwLock::new(configs)),
            handles: Arc::new(RwLock::new(handles)),
            images,
            images_path,
            benchmark_path,
            benchmark_write_lock: Arc::new(Mutex::new(())),
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

    pub async fn get_config(&self, id: &str) -> Option<EngineConfig> {
        let configs = self.configs.read().await;
        configs.get(id).cloned()
    }

    pub async fn list_images(&self) -> Vec<DockerImage> {
        let images = self.images.read().await;
        let mut values: Vec<_> = images.values().cloned().collect();
        values.sort_by(|a, b| a.id.cmp(&b.id));
        values
    }

    pub async fn get_image(&self, id: &str) -> Option<DockerImage> {
        let images = self.images.read().await;
        images.get(id).cloned()
    }

    pub async fn add_image(
        &self,
        image: DockerImage,
    ) -> Result<DockerImage, SupervisorError> {
        validate_image(&image)?;
        let mut images = self.images.write().await;
        if images.contains_key(&image.id) {
            return Err(SupervisorError::ImageExists);
        }
        let id = image.id.clone();
        images.insert(id.clone(), image.clone());
        persist_image_store(&self.images_path, &images).await;
        tracing::info!(image_id = %id, "added docker image");
        Ok(image)
    }

    pub async fn update_image(
        &self,
        id: &str,
        image: DockerImage,
    ) -> Result<DockerImage, SupervisorError> {
        validate_image(&image)?;
        if id != image.id {
            return Err(SupervisorError::ImageInvalid(
                "image id mismatch".to_string(),
            ));
        }
        let mut images = self.images.write().await;
        if !images.contains_key(id) {
            return Err(SupervisorError::ImageNotFound);
        }
        images.insert(id.to_string(), image.clone());
        persist_image_store(&self.images_path, &images).await;
        tracing::info!(image_id = %id, "updated docker image");
        Ok(image)
    }

    pub async fn remove_image(&self, id: &str) -> Result<(), SupervisorError> {
        let configs = self.configs.read().await;
        if configs.values().any(|config| {
            config.engine_type == EngineType::Docker
                && config
                    .docker
                    .as_ref()
                    .map(|docker| docker.image_id == id)
                    .unwrap_or(false)
        }) {
            return Err(SupervisorError::ImageInUse);
        }
        drop(configs);
        let mut images = self.images.write().await;
        if images.remove(id).is_none() {
            return Err(SupervisorError::ImageNotFound);
        }
        persist_image_store(&self.images_path, &images).await;
        tracing::info!(image_id = %id, "removed docker image");
        Ok(())
    }

    pub async fn add_config(&self, config: EngineConfig) -> Result<EngineConfig, SupervisorError> {
        validate_config(&config)?;
        if matches!(config.engine_type, EngineType::Docker) {
            let images = self.images.read().await;
            validate_docker_config(&config, &images)?;
        }
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
                self.images.clone(),
                self.data_dir.join("logs").join(format!("{id}.jsonl")),
                self.data_dir.join("logs").join(format!("{id}-sessions.jsonl")),
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
        if matches!(config.engine_type, EngineType::Docker) {
            let images = self.images.read().await;
            validate_docker_config(&config, &images)?;
        }
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
                self.images.clone(),
                self.data_dir.join("logs").join(format!("{id}.jsonl")),
                self.data_dir.join("logs").join(format!("{id}-sessions.jsonl")),
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

    pub fn benchmark_path(&self) -> &PathBuf {
        &self.benchmark_path
    }

    pub async fn append_benchmark<T: Serialize>(&self, record: &T) {
        append_jsonl(&self.benchmark_path, &self.benchmark_write_lock, record).await;
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

async fn load_image_store(path: &PathBuf) -> anyhow::Result<Vec<DockerImage>> {
    if let Ok(raw) = tokio::fs::read_to_string(path).await {
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let images: Vec<DockerImage> = serde_json::from_str(&raw)?;
        return Ok(images);
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

fn images_to_map(images: &[DockerImage]) -> HashMap<String, DockerImage> {
    images
        .iter()
        .cloned()
        .map(|image| (image.id.clone(), image))
        .collect()
}

async fn persist_config_store(path: &PathBuf, configs: &HashMap<String, EngineConfig>) {
    let mut values: Vec<_> = configs.values().cloned().collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    if let Ok(serialized) = serde_json::to_string_pretty(&values) {
        let _ = tokio::fs::write(path, serialized).await;
    }
}

async fn persist_image_store(path: &PathBuf, images: &HashMap<String, DockerImage>) {
    let mut values: Vec<_> = images.values().cloned().collect();
    values.sort_by(|a, b| a.id.cmp(&b.id));
    if let Ok(serialized) = serde_json::to_string_pretty(&values) {
        let _ = tokio::fs::write(path, serialized).await;
    }
}

fn validate_config(config: &EngineConfig) -> Result<(), SupervisorError> {
    if config.id.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid("id is required".to_string()));
    }
    if config.name.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid("name is required".to_string()));
    }
    if matches!(config.engine_type, EngineType::Docker) {
        let docker = config
            .docker
            .as_ref()
            .ok_or_else(|| SupervisorError::ConfigInvalid("docker config is required".to_string()))?;
        if docker.image_id.trim().is_empty() {
            return Err(SupervisorError::ConfigInvalid(
                "docker image id is required".to_string(),
            ));
        }
    } else if config.command.trim().is_empty() {
        return Err(SupervisorError::ConfigInvalid(
            "command is required".to_string(),
        ));
    }
    Ok(())
}

fn validate_docker_config(
    config: &EngineConfig,
    images: &HashMap<String, DockerImage>,
) -> Result<(), SupervisorError> {
    let docker = config
        .docker
        .as_ref()
        .ok_or_else(|| SupervisorError::ConfigInvalid("docker config is required".to_string()))?;
    if !images.contains_key(&docker.image_id) {
        return Err(SupervisorError::ImageNotFound);
    }
    Ok(())
}

fn validate_image(image: &DockerImage) -> Result<(), SupervisorError> {
    if image.id.trim().is_empty() {
        return Err(SupervisorError::ImageInvalid("id is required".to_string()));
    }
    if image.name.trim().is_empty() {
        return Err(SupervisorError::ImageInvalid("name is required".to_string()));
    }
    if image.image.trim().is_empty() {
        return Err(SupervisorError::ImageInvalid("image is required".to_string()));
    }
    if let Some(build) = &image.build {
        if build
            .context
            .as_ref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(SupervisorError::ImageInvalid(
                "build context is required".to_string(),
            ));
        }
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
    images: Arc<RwLock<HashMap<String, DockerImage>>>,
    instance: Arc<RwLock<EngineInstance>>,
    pub log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    pub log_tx: broadcast::Sender<LogEntry>,
    control: Mutex<EngineControl>,
    pub log_path: PathBuf,
    pub session_path: PathBuf,
    pub status_path: PathBuf,
    log_write_lock: Arc<Mutex<()>>,
    session_write_lock: Arc<Mutex<()>>,
    status_write_lock: Arc<Mutex<()>>,
}

// Control plane state for stopping/restarting a running task.
struct EngineControl {
    stop_tx: Option<watch::Sender<bool>>,
    task: Option<JoinHandle<()>>,
}

impl EngineHandle {
    fn new(
        config: EngineConfig,
        images: Arc<RwLock<HashMap<String, DockerImage>>>,
        log_path: PathBuf,
        session_path: PathBuf,
        status_path: PathBuf,
    ) -> Self {
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
            images,
            instance: Arc::new(RwLock::new(instance)),
            log_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_BUFFER_MAX))),
            log_tx,
            control: Mutex::new(EngineControl {
                stop_tx: None,
                task: None,
            }),
            log_path,
            session_path,
            status_path,
            log_write_lock: Arc::new(Mutex::new(())),
            session_write_lock: Arc::new(Mutex::new(())),
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
            images: self.images.clone(),
            instance: self.instance.clone(),
            log_buffer: self.log_buffer.clone(),
            log_tx: self.log_tx.clone(),
            log_path: self.log_path.clone(),
            session_path: self.session_path.clone(),
            status_path: self.status_path.clone(),
            log_write_lock: self.log_write_lock.clone(),
            session_write_lock: self.session_write_lock.clone(),
            status_write_lock: self.status_write_lock.clone(),
        }
    }
}

#[derive(Clone)]
// Lightweight clone passed into the async task.
struct EngineTaskHandle {
    config: EngineConfig,
    images: Arc<RwLock<HashMap<String, DockerImage>>>,
    instance: Arc<RwLock<EngineInstance>>,
    log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    log_tx: broadcast::Sender<LogEntry>,
    log_path: PathBuf,
    session_path: PathBuf,
    status_path: PathBuf,
    log_write_lock: Arc<Mutex<()>>,
    session_write_lock: Arc<Mutex<()>>,
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

        match spawn_process(&handle.config, &handle.images).await {
            Ok(mut child) => {
                let pid = child.id();
                let wait_for_ready = matches!(
                    handle.config.engine_type,
                    EngineType::Vllm | EngineType::Lvllm
                );
                let ready_marker = if wait_for_ready {
                    Some("Application startup complete.".to_string())
                } else {
                    None
                };
                let (ready_tx, mut ready_rx) = watch::channel(false);
                let session_id = new_session_id();
                let session_started_at = now();
                append_session(
                    &handle.session_path,
                    &handle.session_write_lock,
                    &LogSession {
                        id: session_id.clone(),
                        started_at: session_started_at.clone(),
                        stopped_at: None,
                    },
                )
                .await;
                let mut session_stopped_at: Option<String> = None;
                tracing::info!(
                    engine_id = %handle.config.id,
                    pid = pid,
                    "engine process spawned"
                );
                if wait_for_ready {
                    set_status(&handle, EngineStatus::Starting, pid, None).await;
                } else {
                    set_status(&handle, EngineStatus::Running, pid, Some(now())).await;
                }

                let mut stdout_task = None;
                let mut stderr_task = None;
                let ready_tx = if wait_for_ready { Some(ready_tx) } else { None };

                if let Some(stdout) = child.stdout.take() {
                    stdout_task = Some(tokio::spawn(stream_logs(
                        handle.clone(),
                        BufReader::new(stdout),
                        LogStream::Stdout,
                        session_id.clone(),
                        ready_tx.clone(),
                        ready_marker.clone(),
                    )));
                }
                if let Some(stderr) = child.stderr.take() {
                    stderr_task = Some(tokio::spawn(stream_logs(
                        handle.clone(),
                        BufReader::new(stderr),
                        LogStream::Stderr,
                        session_id.clone(),
                        ready_tx.clone(),
                        ready_marker.clone(),
                    )));
                }

                let mut should_monitor = true;
                if wait_for_ready {
                    let wait_for_ready = async {
                        loop {
                            if *ready_rx.borrow() {
                                return true;
                            }
                            if ready_rx.changed().await.is_err() {
                                return false;
                            }
                        }
                    };

                    tokio::select! {
                        ready = wait_for_ready => {
                            if ready {
                                set_status(&handle, EngineStatus::Running, pid, Some(now())).await;
                            }
                        }
                        _ = stop_rx.changed() => {
                            if *stop_rx.borrow() {
                                tracing::info!(
                                    engine_id = %handle.config.id,
                                    pid = pid,
                                    "stop signal received; terminating engine process"
                                );
                                stop_engine_process(&handle, &mut child).await;
                                set_status(&handle, EngineStatus::Stopped, None, None).await;
                                should_monitor = false;
                                session_stopped_at = Some(now());
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
                            should_monitor = false;
                            session_stopped_at = Some(now());
                        }
                    }
                }

                if should_monitor {
                    tokio::select! {
                        _ = stop_rx.changed() => {
                            if *stop_rx.borrow() {
                                tracing::info!(
                                    engine_id = %handle.config.id,
                                    pid = pid,
                                    "stop signal received; terminating engine process"
                                );
                                stop_engine_process(&handle, &mut child).await;
                                set_status(&handle, EngineStatus::Stopped, None, None).await;
                                session_stopped_at = Some(now());
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
                            session_stopped_at = Some(now());
                        }
                    }
                }

                let session_stopped_at =
                    session_stopped_at.unwrap_or_else(|| now());
                append_session(
                    &handle.session_path,
                    &handle.session_write_lock,
                    &LogSession {
                        id: session_id,
                        started_at: session_started_at,
                        stopped_at: Some(session_stopped_at),
                    },
                )
                .await;

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
async fn spawn_process(
    config: &EngineConfig,
    images: &Arc<RwLock<HashMap<String, DockerImage>>>,
) -> anyhow::Result<tokio::process::Child> {
    if matches!(config.engine_type, EngineType::Docker) {
        let image = resolve_docker_image(config, images).await?;
        let resolved = resolve_docker_spec(config, &image);
        return spawn_docker_process(config, &resolved).await;
    }

    tracing::debug!(
        engine_id = %config.id,
        command = %config.command,
        args = ?config.args,
        working_dir = config.working_dir.as_deref(),
        env_count = config.env.len(),
        "spawning engine process"
    );
    let mut parts = split_command_input(&config.command);
    let command_name = parts
        .first()
        .cloned()
        .unwrap_or_else(|| config.command.clone());
    if !parts.is_empty() {
        parts.remove(0);
    }
    let mut command = Command::new(&command_name);
    if !parts.is_empty() {
        command.args(parts);
    }
    let mut expanded_args = Vec::new();
    for arg in &config.args {
        expanded_args.extend(split_command_input(arg));
    }
    command.args(expanded_args);

    #[cfg(unix)]
    {
        unsafe {
            command.pre_exec(|| {
                // Start the child in its own process group so we can terminate the whole tree.
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }

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

fn split_command_input(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    for ch in input.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escape = true;
            continue;
        }
        if ch == '\'' || ch == '"' {
            if quote == Some(ch) {
                quote = None;
                continue;
            }
            if quote.is_none() {
                quote = Some(ch);
                continue;
            }
        }
        if quote.is_none() && ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if escape {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

struct ResolvedDockerSpec {
    image: String,
    ports: Vec<String>,
    volumes: Vec<String>,
    env: Vec<aiman_shared::EnvVar>,
    run_args: Vec<String>,
    workdir: Option<String>,
    user: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    pull: bool,
    remove: bool,
    build: Option<DockerBuild>,
}

async fn resolve_docker_image(
    config: &EngineConfig,
    images: &Arc<RwLock<HashMap<String, DockerImage>>>,
) -> anyhow::Result<DockerImage> {
    let docker = config
        .docker
        .as_ref()
        .context("docker config missing")?;
    let images = images.read().await;
    images
        .get(&docker.image_id)
        .cloned()
        .context("docker image not found")
}

fn resolve_docker_spec(config: &EngineConfig, image: &DockerImage) -> ResolvedDockerSpec {
    let docker = config.docker.as_ref();
    let extra_ports = docker
        .map(|docker| docker.extra_ports.clone())
        .unwrap_or_default();
    let extra_volumes = docker
        .map(|docker| docker.extra_volumes.clone())
        .unwrap_or_default();
    let extra_env = docker
        .map(|docker| docker.extra_env.clone())
        .unwrap_or_default();
    let extra_run_args = docker
        .map(|docker| docker.extra_run_args.clone())
        .unwrap_or_default();

    let mut ports = image.ports.clone();
    ports.extend(extra_ports);
    let mut volumes = image.volumes.clone();
    volumes.extend(extra_volumes);
    let mut env = image.env.clone();
    env.extend(extra_env);
    env.extend(config.env.clone());
    let mut run_args = image.run_args.clone();
    run_args.extend(extra_run_args);

    let pull = docker
        .and_then(|docker| docker.pull)
        .unwrap_or(image.pull);
    let remove = docker
        .and_then(|docker| docker.remove)
        .unwrap_or(image.remove);

    let workdir = docker
        .and_then(|docker| docker.workdir.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| image.workdir.clone());
    let user = docker
        .and_then(|docker| docker.user.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| image.user.clone());
    let command = docker
        .and_then(|docker| docker.command.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| image.command.clone());

    let mut args = image.args.clone();
    if let Some(docker) = docker {
        args.extend(docker.args.iter().cloned());
    }

    ResolvedDockerSpec {
        image: image.image.clone(),
        ports,
        volumes,
        env,
        run_args,
        workdir,
        user,
        command,
        args,
        pull,
        remove,
        build: image.build.clone(),
    }
}

async fn spawn_docker_process(
    config: &EngineConfig,
    docker: &ResolvedDockerSpec,
) -> anyhow::Result<tokio::process::Child> {
    let runtime = docker_runtime_command(config);

    if let Some(build) = &docker.build {
        build_docker_image(&runtime, config, build, &docker.image).await?;
    } else if docker.pull {
        pull_docker_image(&runtime, config, &docker.image).await?;
    }

    tracing::debug!(
        engine_id = %config.id,
        runtime = %runtime,
        image = %docker.image,
        "spawning docker engine process"
    );

    let mut args: Vec<String> = Vec::new();
    args.push("run".to_string());
    if docker.remove {
        args.push("--rm".to_string());
    }

    let container_name = config
        .docker
        .as_ref()
        .map(|docker| docker_container_name(config, docker))
        .unwrap_or_else(|| config.id.clone());
    if !container_name.is_empty() {
        args.push("--name".to_string());
        args.push(container_name);
    }

    for port in docker.ports.iter().map(|value| value.trim()).filter(|value| !value.is_empty()) {
        args.push("-p".to_string());
        args.push(port.to_string());
    }

    for volume in docker
        .volumes
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        args.push("-v".to_string());
        args.push(volume.to_string());
    }

    if let Some(workdir) = docker
        .workdir
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        args.push("-w".to_string());
        args.push(workdir.to_string());
    }

    if let Some(user) = docker
        .user
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        args.push("-u".to_string());
        args.push(user.to_string());
    }

    for env in &docker.env {
        if !env.key.trim().is_empty() {
            args.push("-e".to_string());
            args.push(format!("{}={}", env.key, env.value));
        }
    }

    for arg in &docker.run_args {
        args.extend(split_command_input(arg));
    }

    let image = docker.image.trim();
    if image.is_empty() {
        anyhow::bail!("docker image is required");
    }
    args.push(image.to_string());

    if let Some(command) = docker
        .command
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        args.extend(split_command_input(command));
    }

    for arg in &docker.args {
        args.extend(split_command_input(arg));
    }

    let mut command = docker_command(&runtime);
    command.args(args);

    if let Some(dir) = &config.working_dir {
        command.current_dir(dir);
    }

    #[cfg(unix)]
    {
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }

    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    Ok(child)
}

fn docker_runtime_command(config: &EngineConfig) -> String {
    let trimmed = config.command.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if let Ok(value) = std::env::var("AIMAN_DOCKER_RUNTIME") {
        if !value.trim().is_empty() {
            return value;
        }
    }
    "docker".to_string()
}

fn docker_container_name(config: &EngineConfig, docker: &DockerConfig) -> String {
    docker
        .container_name
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| config.id.clone())
}

async fn build_docker_image(
    runtime: &str,
    config: &EngineConfig,
    build: &DockerBuild,
    image: &str,
) -> anyhow::Result<()> {
    let context = build.context.as_deref().unwrap_or("");
    if context.trim().is_empty() {
        anyhow::bail!("docker build context is required");
    }
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("docker image is required for build");
    }

    let mut command = docker_command(runtime);
    command.arg("build").arg("-t").arg(image);
    if build.pull {
        command.arg("--pull");
    }
    if build.no_cache {
        command.arg("--no-cache");
    }
    if let Some(target) = build
        .target
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        command.arg("--target").arg(target);
    }
    let mut temp_dockerfile: Option<PathBuf> = None;
    if let Some(content) = build
        .dockerfile_content
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let filename = format!(
            "aiman-dockerfile-{}-{}.Dockerfile",
            config.id,
            Utc::now().timestamp_millis()
        );
        let path = std::env::temp_dir().join(filename);
        tokio::fs::write(&path, content).await?;
        temp_dockerfile = Some(path);
    }

    if let Some(dockerfile) = temp_dockerfile.as_ref() {
        command.arg("-f").arg(dockerfile);
    } else if let Some(dockerfile) = build
        .dockerfile
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        command.arg("-f").arg(dockerfile);
    }
    for entry in &build.build_args {
        if !entry.key.trim().is_empty() {
            command
                .arg("--build-arg")
                .arg(format!("{}={}", entry.key, entry.value));
        }
    }
    command.arg(context.trim());

    if let Some(dir) = &config.working_dir {
        command.current_dir(dir);
    }

    let output = command.output().await?;
    if let Some(path) = temp_dockerfile {
        let _ = tokio::fs::remove_file(path).await;
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        anyhow::bail!("docker build failed: {}", detail);
    }
    Ok(())
}

async fn pull_docker_image(
    runtime: &str,
    config: &EngineConfig,
    image: &str,
) -> anyhow::Result<()> {
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("docker image is required for pull");
    }
    let mut command = docker_command(runtime);
    command.arg("pull").arg(image);
    if let Some(dir) = &config.working_dir {
        command.current_dir(dir);
    }
    let output = command.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        anyhow::bail!("docker pull failed: {}", detail);
    }
    Ok(())
}

async fn stop_engine_process(handle: &EngineTaskHandle, child: &mut tokio::process::Child) {
    if matches!(handle.config.engine_type, EngineType::Docker) {
        if let Err(err) = stop_docker_container(&handle.config).await {
            tracing::warn!(
                engine_id = %handle.config.id,
                error = %err,
                "failed to stop docker container"
            );
        }
    }
    terminate_process_tree(child).await;
}

async fn stop_docker_container(config: &EngineConfig) -> anyhow::Result<()> {
    let docker = match &config.docker {
        Some(docker) => docker,
        None => return Ok(()),
    };
    let container_name = docker_container_name(config, docker);
    if container_name.trim().is_empty() {
        return Ok(());
    }
    let runtime = docker_runtime_command(config);
    let mut command = docker_command(&runtime);
    command.arg("stop").arg(container_name);
    if let Some(dir) = &config.working_dir {
        command.current_dir(dir);
    }
    let _ = command.output().await;
    Ok(())
}

fn docker_command(runtime: &str) -> Command {
    let mut parts = split_command_input(runtime);
    let command_name = parts
        .first()
        .cloned()
        .unwrap_or_else(|| runtime.to_string());
    if !parts.is_empty() {
        parts.remove(0);
    }
    let mut command = Command::new(command_name);
    if !parts.is_empty() {
        command.args(parts);
    }
    command
}

async fn terminate_process_tree(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            let pgid = pid as i32;
            unsafe {
                // SIGTERM process group first.
                libc::killpg(pgid, libc::SIGTERM);
            }
            let wait_result =
                tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
            if wait_result.is_err() {
                unsafe {
                    libc::killpg(pgid, libc::SIGKILL);
                }
                let _ = child.wait().await;
            }
            return;
        }
    }

    // Fallback for non-unix or missing pid.
    let _ = child.kill().await;
    let _ = child.wait().await;
}

// Read log lines, store to buffer + JSONL, and broadcast.
async fn stream_logs<R: tokio::io::AsyncRead + Unpin>(
    handle: Arc<EngineTaskHandle>,
    reader: BufReader<R>,
    stream: LogStream,
    session_id: String,
    ready_tx: Option<watch::Sender<bool>>,
    ready_marker: Option<String>,
) {
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let (Some(tx), Some(marker)) = (&ready_tx, &ready_marker) {
            if line.contains(marker) {
                let _ = tx.send(true);
            }
        }
        let entry = LogEntry {
            ts: now(),
            session_id: session_id.clone(),
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

fn new_session_id() -> String {
    Utc::now().timestamp_millis().to_string()
}

async fn append_session(path: &PathBuf, lock: &Arc<Mutex<()>>, session: &LogSession) {
    append_jsonl(path, lock, session).await;
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

// Read log JSONL with optional since + session filtering.
pub async fn read_log_entries(
    path: &PathBuf,
    since: Option<&str>,
    limit: Option<usize>,
    session_id: Option<&str>,
) -> anyhow::Result<Vec<LogEntry>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let since_dt = since.and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok());
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries: VecDeque<LogEntry> = VecDeque::new();
    let max = limit.unwrap_or(500);

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
            if let Some(since_dt) = since_dt {
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&entry.ts) {
                    if parsed < since_dt {
                        continue;
                    }
                }
            }
            if let Some(session_id) = session_id {
                if entry.session_id != session_id {
                    continue;
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

// Read log sessions and collapse start/stop records into a single entry per session.
pub async fn read_log_sessions(
    path: &PathBuf,
    limit: Option<usize>,
) -> anyhow::Result<Vec<LogSession>> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut sessions: HashMap<String, LogSession> = HashMap::new();

    while let Ok(Some(line)) = lines.next_line().await {
        if let Ok(entry) = serde_json::from_str::<LogSession>(&line) {
            let session = sessions.entry(entry.id.clone()).or_insert(LogSession {
                id: entry.id.clone(),
                started_at: entry.started_at.clone(),
                stopped_at: None,
            });
            if entry.started_at < session.started_at {
                session.started_at = entry.started_at.clone();
            }
            if entry.stopped_at.is_some() {
                session.stopped_at = entry.stopped_at.clone();
            }
        }
    }

    let mut values: Vec<_> = sessions.into_values().collect();
    values.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    if let Some(limit) = limit {
        values.truncate(limit);
    }

    Ok(values)
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
