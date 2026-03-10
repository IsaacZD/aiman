use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use aiman_shared::{AutoRestart, EngineConfig, EngineInstance, EngineStatus, LogEntry, LogStream};
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, watch, Mutex, RwLock},
    task::JoinHandle,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const LOG_BUFFER_MAX: usize = 2000;

#[derive(Clone)]
struct AppState {
    supervisor: Arc<Supervisor>,
    api_key: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config_path = std::env::var("AIMAN_ENGINES_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("configs/engines.toml"));

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

    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/engines", get(list_engines))
        .route("/v1/engines/:id", get(get_engine))
        .route("/v1/engines/:id/start", post(start_engine))
        .route("/v1/engines/:id/stop", post(stop_engine))
        .route("/v1/engines/:id/logs", get(engine_logs))
        .route("/v1/engines/:id/logs/ws", get(engine_logs_ws))
        .route("/v1/engines/:id/status", get(engine_status_history))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    let bind_addr = std::env::var("AIMAN_BIND").unwrap_or_else(|_| "0.0.0.0:4010".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("bind host listener");

    tracing::info!("aiman-host listening on {bind_addr}");
    axum::serve(listener, app).await.expect("serve host");
}

async fn health() -> &'static str {
    "ok"
}

async fn auth_middleware(
    State(state): State<AppState>,
    request: axum::http::Request<axum::body::Body>,
    next: middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let Some(expected) = state.api_key else {
        return Ok(next.run(request).await);
    };

    let provided = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    if provided == Some(expected.as_str()) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn list_engines(State(state): State<AppState>) -> Json<EnginesResponse> {
    Json(EnginesResponse {
        engines: state.supervisor.list_instances().await,
    })
}

async fn get_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
    let instance = state
        .supervisor
        .get_instance(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(EngineResponse { instance }))
}

async fn start_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
    let instance = state
        .supervisor
        .start(&id)
        .await
        .map_err(map_supervisor_error)?;

    Ok(Json(EngineResponse { instance }))
}

async fn stop_engine(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EngineResponse>, StatusCode> {
    let instance = state
        .supervisor
        .stop(&id)
        .await
        .map_err(map_supervisor_error)?;

    Ok(Json(EngineResponse { instance }))
}

async fn engine_logs_ws(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(ws.on_upgrade(move |socket| async move {
        let mut rx = handle.log_tx.subscribe();
        let buffer = { handle.log_buffer.lock().await.clone() };

        let (mut sender, mut receiver) = socket.split();

        for entry in buffer {
            if let Ok(text) = serde_json::to_string(&entry) {
                if sender
                    .send(axum::extract::ws::Message::Text(text))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        loop {
            tokio::select! {
                Ok(entry) = rx.recv() => {
                    if let Ok(text) = serde_json::to_string(&entry) {
                        if sender.send(axum::extract::ws::Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                }
                msg = receiver.next() => {
                    if msg.is_none() {
                        break;
                    }
                }
                else => break,
            }
        }
    }))
}

#[derive(Deserialize)]
struct LogQuery {
    since: Option<String>,
    limit: Option<usize>,
}

async fn engine_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogQuery>,
) -> Result<Json<LogHistoryResponse>, StatusCode> {
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let entries = read_jsonl(&handle.log_path, query.since.as_deref(), query.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(LogHistoryResponse { entries }))
}

async fn engine_status_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<LogQuery>,
) -> Result<Json<StatusHistoryResponse>, StatusCode> {
    let handle = state
        .supervisor
        .get_handle(&id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    let entries = read_jsonl(&handle.status_path, query.since.as_deref(), query.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(StatusHistoryResponse { entries }))
}

#[derive(Serialize)]
struct EnginesResponse {
    engines: Vec<EngineInstance>,
}

#[derive(Serialize)]
struct EngineResponse {
    instance: EngineInstance,
}

#[derive(Serialize)]
struct LogHistoryResponse {
    entries: Vec<LogEntry>,
}

#[derive(Serialize)]
struct StatusHistoryResponse {
    entries: Vec<EngineInstance>,
}

#[derive(Debug, thiserror::Error)]
enum SupervisorError {
    #[error("engine not found")]
    NotFound,
    #[error("engine already running")]
    AlreadyRunning,
    #[error("engine not running")]
    NotRunning,
    #[error("engine failed to start: {0}")]
    StartFailed(String),
}

fn map_supervisor_error(err: SupervisorError) -> StatusCode {
    match err {
        SupervisorError::NotFound => StatusCode::NOT_FOUND,
        SupervisorError::AlreadyRunning => StatusCode::CONFLICT,
        SupervisorError::NotRunning => StatusCode::CONFLICT,
        SupervisorError::StartFailed(_) => StatusCode::BAD_REQUEST,
    }
}

#[derive(Clone)]
struct Supervisor {
    configs: Arc<HashMap<String, EngineConfig>>,
    handles: Arc<RwLock<HashMap<String, Arc<EngineHandle>>>>,
}

impl Supervisor {
    async fn from_file(path: PathBuf, data_dir: PathBuf) -> anyhow::Result<Self> {
        let raw = tokio::fs::read_to_string(path).await?;
        let parsed: ConfigFile = toml::from_str(&raw)?;
        let mut configs = HashMap::new();
        let mut handles = HashMap::new();

        tokio::fs::create_dir_all(data_dir.join("logs")).await?;
        tokio::fs::create_dir_all(data_dir.join("status")).await?;

        for config in parsed.engine {
            let id = config.id.clone();
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
            configs: Arc::new(configs),
            handles: Arc::new(RwLock::new(handles)),
        })
    }

    async fn list_instances(&self) -> Vec<EngineInstance> {
        let handles = self.handles.read().await;
        let mut instances = Vec::with_capacity(handles.len());
        for handle in handles.values() {
            instances.push(handle.instance.read().await.clone());
        }
        instances
    }

    async fn get_instance(&self, id: &str) -> Option<EngineInstance> {
        let handles = self.handles.read().await;
        let handle = handles.get(id)?.clone();
        let instance = handle.instance.read().await.clone();
        Some(instance)
    }

    async fn get_handle(&self, id: &str) -> Option<Arc<EngineHandle>> {
        let handles = self.handles.read().await;
        handles.get(id).cloned()
    }

    async fn start(&self, id: &str) -> Result<EngineInstance, SupervisorError> {
        let handle = self.get_handle(id).await.ok_or(SupervisorError::NotFound)?;
        handle.start().await?;
        let instance = handle.instance.read().await.clone();
        Ok(instance)
    }

    async fn stop(&self, id: &str) -> Result<EngineInstance, SupervisorError> {
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

struct EngineHandle {
    config: EngineConfig,
    instance: Arc<RwLock<EngineInstance>>,
    log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    log_tx: broadcast::Sender<LogEntry>,
    control: Mutex<EngineControl>,
    log_path: PathBuf,
    status_path: PathBuf,
    log_write_lock: Arc<Mutex<()>>,
    status_write_lock: Arc<Mutex<()>>,
}

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

    async fn start(&self) -> Result<(), SupervisorError> {
        let mut control = self.control.lock().await;
        if control
            .task
            .as_ref()
            .map(|task| !task.is_finished())
            .unwrap_or(false)
        {
            return Err(SupervisorError::AlreadyRunning);
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        control.stop_tx = Some(stop_tx);

        let handle = Arc::new(self.clone_for_task());
        control.task = Some(tokio::spawn(async move {
            run_engine(handle, stop_rx).await;
        }));
        Ok(())
    }

    async fn stop(&self) -> Result<(), SupervisorError> {
        let control = self.control.lock().await;
        let Some(stop_tx) = &control.stop_tx else {
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

async fn run_engine(handle: Arc<EngineTaskHandle>, mut stop_rx: watch::Receiver<bool>) {
    let mut retries = 0;

    loop {
        if *stop_rx.borrow() {
            set_status(&handle, EngineStatus::Stopped, None, None).await;
            break;
        }

        set_status(&handle, EngineStatus::Starting, None, None).await;

        match spawn_process(&handle.config).await {
            Ok(mut child) => {
                let pid = child.id();
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
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            set_status(&handle, EngineStatus::Stopped, None, None).await;
                            break;
                        }
                    }
                    status = child.wait() => {
                        let code = status.ok().and_then(|s| s.code());
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
            tokio::time::sleep(Duration::from_secs(handle.config.auto_restart.backoff_secs)).await;
            continue;
        }

        break;
    }
}

async fn spawn_process(config: &EngineConfig) -> anyhow::Result<tokio::process::Child> {
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
}

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
}

async fn set_exit_status(handle: &EngineTaskHandle, code: Option<i32>) {
    let mut instance = handle.instance.write().await;
    instance.status = EngineStatus::Stopped;
    instance.pid = None;
    instance.last_exit_code = code;
    instance.last_exit_at = Some(now());
    let snapshot = instance.clone();
    drop(instance);
    append_jsonl(&handle.status_path, &handle.status_write_lock, &snapshot).await;
}

fn should_restart(policy: &AutoRestart, retries: u32) -> bool {
    policy.enabled && retries < policy.max_retries
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

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

async fn read_jsonl<T: for<'de> Deserialize<'de>>(
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

fn extract_ts(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.get("ts")?.as_str().map(|s| s.to_string())
}
