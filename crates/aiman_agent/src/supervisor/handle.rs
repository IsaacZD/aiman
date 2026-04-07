//! Engine handle implementation with process lifecycle management.
//!
//! This module uses unsafe code for POSIX process group management
//! (setpgid, killpg) which is necessary for proper subprocess cleanup.
#![allow(unsafe_code)]

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use aiman_shared::{
    AutoRestart, ContainerBuild, ContainerConfig, ContainerImage, EngineConfig, EngineInstance,
    EngineStatus, EngineType, ImageStatus, LogEntry, LogSession, LogStream,
};
use chrono::Utc;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, watch, Mutex, RwLock},
    task::JoinHandle,
};

use super::error::SupervisorError;
use super::store::LogWriter;

/// Run the pull or build step for a container image.
/// Called from a spawned task in Supervisor::prepare_image.
pub async fn prepare_image_task(image: &ContainerImage) -> anyhow::Result<()> {
    let has_dockerfile = image
        .build
        .as_ref()
        .and_then(|b| b.dockerfile_content.as_deref())
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    if has_dockerfile {
        let build = image.build.as_ref().unwrap();
        // For Dockerfile-based images, we need a tag. Use the image id as the local tag.
        let tag = format!("aiman-build:{}", image.id);
        build_container_image(build, &tag, &image.id).await
    } else {
        label_container_image(&image.image, &image.id).await
    }
}

/// Normalize a container image name for Podman.
///
/// Unlike Docker, Podman doesn't automatically assume `docker.io` for short names.
/// This function ensures images without an explicit registry get the `docker.io/` prefix.
///
/// Examples:
/// - `nginx` → `docker.io/library/nginx`
/// - `vllm/vllm-openai:latest` → `docker.io/vllm/vllm-openai:latest`
/// - `ghcr.io/foo/bar:v1` → `ghcr.io/foo/bar:v1` (unchanged)
fn normalize_image_name(image: &str) -> String {
    let image = image.trim();
    // Split off the tag/digest to analyze the name part.
    let (name, suffix) = if let Some(at) = image.rfind('@') {
        (&image[..at], &image[at..])
    } else if let Some(colon) = image.rfind(':') {
        // Make sure it's a tag colon, not a port in the registry.
        let before_colon = &image[..colon];
        if before_colon.contains('/') || !before_colon.contains('.') {
            (&image[..colon], &image[colon..])
        } else {
            (image, "")
        }
    } else {
        (image, "")
    };

    let parts: Vec<&str> = name.split('/').collect();
    match parts.len() {
        1 => {
            // Library image: nginx → docker.io/library/nginx
            format!("docker.io/library/{name}{suffix}")
        }
        2 => {
            // Check if first part looks like a registry (has a dot or colon).
            if parts[0].contains('.') || parts[0].contains(':') {
                // Already has registry: localhost/foo, myregistry.com/bar
                image.to_string()
            } else {
                // User image: vllm/vllm-openai → docker.io/vllm/vllm-openai
                format!("docker.io/{name}{suffix}")
            }
        }
        _ => {
            // 3+ parts: likely already fully qualified (e.g., ghcr.io/owner/repo)
            image.to_string()
        }
    }
}

// Keep a bounded in-memory log buffer per engine for WS backfill.
const LOG_BUFFER_MAX: usize = 2000;

// Per-engine state + control handles.
pub struct EngineHandle {
    pub(super) config: EngineConfig,
    pub(super) images: Arc<RwLock<HashMap<String, ContainerImage>>>,
    pub(super) instance: Arc<RwLock<EngineInstance>>,
    pub log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    pub log_tx: broadcast::Sender<LogEntry>,
    pub(super) status_tx: broadcast::Sender<EngineInstance>,
    control: Mutex<EngineControl>,
    pub log_path: PathBuf,
    pub session_path: PathBuf,
    pub status_path: PathBuf,
    log_writer: LogWriter,
    session_writer: LogWriter,
    status_writer: LogWriter,
}

// Control plane state for stopping/restarting a running task.
struct EngineControl {
    stop_tx: Option<watch::Sender<bool>>,
    task: Option<JoinHandle<()>>,
}

impl EngineHandle {
    pub(super) fn new(
        config: EngineConfig,
        images: Arc<RwLock<HashMap<String, ContainerImage>>>,
        log_path: PathBuf,
        session_path: PathBuf,
        status_path: PathBuf,
        status_tx: broadcast::Sender<EngineInstance>,
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
            status_tx,
            control: Mutex::new(EngineControl {
                stop_tx: None,
                task: None,
            }),
            log_writer: LogWriter::new(log_path.clone()),
            session_writer: LogWriter::new(session_path.clone()),
            status_writer: LogWriter::new(status_path.clone()),
            log_path,
            session_path,
            status_path,
        }
    }

    // Start the engine task if not running.
    pub(super) async fn start(&self) -> Result<(), SupervisorError> {
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
    pub(super) async fn stop(&self) -> Result<(), SupervisorError> {
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
            status_tx: self.status_tx.clone(),
            log_writer: self.log_writer.clone(),
            session_writer: self.session_writer.clone(),
            status_writer: self.status_writer.clone(),
        }
    }
}

#[derive(Clone)]
// Lightweight clone passed into the async task.
struct EngineTaskHandle {
    config: EngineConfig,
    images: Arc<RwLock<HashMap<String, ContainerImage>>>,
    instance: Arc<RwLock<EngineInstance>>,
    log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    log_tx: broadcast::Sender<LogEntry>,
    // Broadcast engine status changes to SSE subscribers.
    status_tx: broadcast::Sender<EngineInstance>,
    log_writer: LogWriter,
    session_writer: LogWriter,
    status_writer: LogWriter,
}

// Route to container (podman) or process-based engine loop.
async fn run_engine(handle: Arc<EngineTaskHandle>, mut stop_rx: watch::Receiver<bool>) {
    let mut retries = 0;

    loop {
        if *stop_rx.borrow() {
            tracing::info!(engine_id = %handle.config.id, "stop signal received before start");
            set_status(&handle, EngineStatus::Stopped, None, None).await;
            break;
        }

        tracing::info!(engine_id = %handle.config.id, "starting engine");
        set_status(&handle, EngineStatus::Starting, None, None).await;

        let result = if matches!(handle.config.engine_type, EngineType::Container) {
            run_container_engine(&handle, &mut stop_rx).await
        } else {
            run_process_engine(&handle, &mut stop_rx).await
        };

        if let Err(err) = result {
            set_status(&handle, EngineStatus::Error, None, None).await;
            tracing::warn!(error = ?err, engine_id = %handle.config.id, "engine error");
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

// ── Container engine (podman CLI) ────────────────────────────────────────────

async fn run_container_engine(
    handle: &Arc<EngineTaskHandle>,
    stop_rx: &mut watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let image = resolve_container_image(&handle.config, &handle.images).await?;
    if image.status != ImageStatus::Ready {
        anyhow::bail!("container image '{}' is not ready — run Prepare first", image.name);
    }
    let resolved = resolve_container_spec(&handle.config, &image);

    // Container name: prefer explicit config, fall back to engine id.
    let container_name = handle
        .config
        .container
        .as_ref()
        .map(|c| container_name_from_config(&handle.config, c))
        .unwrap_or_else(|| handle.config.id.clone());

    // Remove any existing container with the same name to avoid conflicts.
    remove_container(&container_name).await;

    // Create container.
    let container_id = create_container(&resolved, &container_name).await?;

    // Start container.
    let output = Command::new("podman")
        .args(["start", &container_id])
        .output()
        .await
        .context("failed to start container")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman start failed: {}", stderr.trim());
    }

    tracing::info!(
        engine_id = %handle.config.id,
        container_id = %container_id,
        "container started"
    );

    let session_id = new_session_id();
    let session_started_at = now();
    let _ = handle.session_writer.append(&LogSession {
        id: session_id.clone(),
        started_at: session_started_at.clone(),
        stopped_at: None,
    }).await;

    // Determine whether we wait for a ready marker before setting Running.
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
    let ready_tx_opt = if wait_for_ready { Some(ready_tx) } else { None };

    if wait_for_ready {
        set_status(handle, EngineStatus::Starting, None, None).await;
    } else {
        set_status(handle, EngineStatus::Running, None, Some(now())).await;
    }

    // Spawn log streaming task via `podman logs -f`.
    let log_task = {
        let handle = handle.clone();
        let container_id = container_id.clone();
        let session_id = session_id.clone();
        tokio::spawn(async move {
            stream_container_logs(handle, container_id, session_id, ready_tx_opt, ready_marker)
                .await;
        })
    };

    // Spawn health polling task.
    let health_task = {
        let handle = handle.clone();
        let container_id = container_id.clone();
        tokio::spawn(async move {
            poll_container_health(handle, container_id).await;
        })
    };

    let mut session_stopped_at: Option<String> = None;

    // Wait for ready marker if needed.
    if wait_for_ready {
        let wait_ready = async {
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
            ready = wait_ready => {
                if ready {
                    set_status(handle, EngineStatus::Running, None, Some(now())).await;
                }
            }
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    stop_container(&container_id).await;
                    if resolved.remove {
                        remove_container(&container_id).await;
                    }
                    set_status(handle, EngineStatus::Stopped, None, None).await;
                    session_stopped_at = Some(now());
                    finalize_session(handle, session_id, session_started_at, session_stopped_at, log_task, health_task).await;
                    return Ok(());
                }
            }
        }
    }

    // Wait for container exit or stop signal via `podman wait`.
    let wait_container_id = container_id.clone();
    let mut wait_child = Command::new("podman")
        .args(["wait", &wait_container_id])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn podman wait")?;

    tokio::select! {
        _ = stop_rx.changed() => {
            if *stop_rx.borrow() {
                tracing::info!(
                    engine_id = %handle.config.id,
                    container_id = %container_id,
                    "stop signal received; stopping container"
                );
                // Kill the wait process first.
                let _ = wait_child.kill().await;
                stop_container(&container_id).await;
                if resolved.remove {
                    remove_container(&container_id).await;
                }
                set_status(handle, EngineStatus::Stopped, None, None).await;
                session_stopped_at = Some(now());
            }
        }
        result = wait_child.wait() => {
            // `podman wait` prints the exit code to stdout.
            let code = if let Ok(wait_output) = wait_child.wait_with_output().await {
                String::from_utf8_lossy(&wait_output.stdout)
                    .trim()
                    .parse::<i32>()
                    .ok()
            } else {
                result.ok().and_then(|s| s.code())
            };
            tracing::info!(
                engine_id = %handle.config.id,
                container_id = %container_id,
                exit_code = code,
                "container exited"
            );
            if resolved.remove {
                remove_container(&container_id).await;
            }
            set_exit_status(handle, code).await;
            session_stopped_at = Some(now());
        }
    }

    finalize_session(
        handle,
        session_id,
        session_started_at,
        session_stopped_at,
        log_task,
        health_task,
    )
    .await;
    Ok(())
}

async fn finalize_session(
    handle: &Arc<EngineTaskHandle>,
    session_id: String,
    session_started_at: String,
    session_stopped_at: Option<String>,
    log_task: JoinHandle<()>,
    health_task: JoinHandle<()>,
) {
    let stopped_at = session_stopped_at.unwrap_or_else(now);
    let _ = handle.session_writer.append(&LogSession {
        id: session_id,
        started_at: session_started_at,
        stopped_at: Some(stopped_at),
    }).await;
    health_task.abort();
    let _ = log_task.await;
}

/// Pull-only path: build a one-line `FROM <image>` Dockerfile with --pull so
/// podman fetches the latest registry image and we can attach aiman labels in
/// the same step.  The resulting local image keeps the original tag.
async fn label_container_image(image: &str, image_config_id: &str) -> anyhow::Result<()> {
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("container image is required for pull");
    }
    // Normalize image name for Podman (e.g., vllm/vllm-openai → docker.io/vllm/vllm-openai).
    let normalized = normalize_image_name(image);
    tracing::info!(image = %image, normalized = %normalized, image_config_id = %image_config_id, "labeling container image via build");

    let build_dir = std::env::temp_dir()
        .join(format!("aiman-label-{}", Utc::now().timestamp_millis()));
    tokio::fs::create_dir_all(&build_dir).await?;
    tokio::fs::write(build_dir.join("Dockerfile"), format!("FROM {normalized}")).await?;

    let output = Command::new("podman")
        .arg("build")
        .arg("-t").arg(&normalized)
        .arg("--pull")
        .arg("--label").arg("managed-by=aiman")
        .arg("--label").arg(format!("aiman.image-id={image_config_id}"))
        .arg(&build_dir)
        .output()
        .await?;
    let _ = tokio::fs::remove_dir_all(&build_dir).await;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman label step failed: {}", stderr.trim());
    }
    Ok(())
}

/// Build the `podman create` command and return the container ID.
async fn create_container(
    resolved: &ResolvedContainerSpec,
    container_name: &str,
) -> anyhow::Result<String> {
    let mut cmd = Command::new("podman");
    cmd.arg("create");

    // Container name.
    cmd.arg("--name").arg(container_name);

    // Labels.
    cmd.arg("--label").arg("managed-by=aiman");

    // Port bindings: "host:container[/proto]".
    for port_spec in &resolved.ports {
        let spec = port_spec.trim();
        if !spec.is_empty() {
            cmd.arg("-p").arg(spec);
        }
    }

    // Volume bindings: "/host:/container[:options]".
    for vol in &resolved.volumes {
        let vol = vol.trim();
        if !vol.is_empty() {
            cmd.arg("-v").arg(vol);
        }
    }

    // Environment variables.
    for env in &resolved.env {
        if !env.key.trim().is_empty() {
            cmd.arg("-e").arg(format!("{}={}", env.key, env.value));
        }
    }

    // GPU devices via CDI.
    if let Some(gpus) = &resolved.gpus {
        let gpus = gpus.trim();
        if !gpus.is_empty() {
            if gpus == "all" {
                cmd.arg("--device").arg("nvidia.com/gpu=all");
            } else {
                // Specific GPU IDs (e.g., "0", "0,1", "1,3").
                for gpu_id in gpus.split(',') {
                    let gpu_id = gpu_id.trim();
                    if !gpu_id.is_empty() {
                        cmd.arg("--device").arg(format!("nvidia.com/gpu={gpu_id}"));
                    }
                }
            }
        }
    }

    // User override.
    if let Some(user) = resolved.user.as_ref().map(|v| v.trim()).filter(|v| !v.is_empty()) {
        cmd.arg("--user").arg(user);
    }

    // Image.
    cmd.arg(resolved.image.trim());

    // Command + args (entrypoint override).
    if let Some(command) = resolved
        .command
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    {
        for part in split_command_input(command) {
            cmd.arg(part);
        }
    }
    for arg in &resolved.args {
        for part in split_command_input(arg) {
            cmd.arg(part);
        }
    }

    let output = cmd.output().await.context("failed to run podman create")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman create failed: {}", stderr.trim());
    }

    let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(container_id)
}

/// Stream container stdout/stderr via `podman logs -f` into the log buffer.
async fn stream_container_logs(
    handle: Arc<EngineTaskHandle>,
    container_id: String,
    session_id: String,
    ready_tx: Option<watch::Sender<bool>>,
    ready_marker: Option<String>,
) {
    let mut child = match Command::new("podman")
        .args(["logs", "-f", &container_id])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            tracing::debug!(engine_id = %handle.config.id, error = %err, "failed to spawn podman logs");
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_task = stdout.map(|out| {
        let handle = handle.clone();
        let session_id = session_id.clone();
        let ready_tx = ready_tx.clone();
        let ready_marker = ready_marker.clone();
        tokio::spawn(async move {
            stream_logs(handle, BufReader::new(out), LogStream::Stdout, session_id, ready_tx, ready_marker).await;
        })
    });

    let stderr_task = stderr.map(|err| {
        let handle = handle.clone();
        let session_id = session_id.clone();
        let ready_tx = ready_tx.clone();
        let ready_marker = ready_marker.clone();
        tokio::spawn(async move {
            stream_logs(handle, BufReader::new(err), LogStream::Stderr, session_id, ready_tx, ready_marker).await;
        })
    });

    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }

    // Ensure the child process is cleaned up.
    let _ = child.wait().await;

    tracing::debug!(engine_id = %handle.config.id, "container log stream ended");
}

/// Poll container health status every 30 s and update EngineInstance.health.
async fn poll_container_health(handle: Arc<EngineTaskHandle>, container_id: String) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let output = Command::new("podman")
            .args(["inspect", "--format", "{{.State.Health.Status}}", &container_id])
            .output()
            .await;
        match output {
            Ok(out) if out.status.success() => {
                let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !status.is_empty() {
                    let mut instance = handle.instance.write().await;
                    instance.health = Some(status);
                }
            }
            _ => break, // Container gone; task will be aborted anyway.
        }
    }
}

async fn stop_container(container_id: &str) {
    let result = Command::new("podman")
        .args(["stop", "-t", "10", container_id])
        .output()
        .await;
    if let Err(err) = result {
        tracing::warn!(container_id = %container_id, error = %err, "failed to stop container");
    } else if let Ok(out) = result {
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!(container_id = %container_id, error = %stderr.trim(), "failed to stop container");
        }
    }
}

async fn remove_container(container_id: &str) {
    let result = Command::new("podman")
        .args(["rm", "-f", container_id])
        .output()
        .await;
    if let Err(err) = result {
        tracing::warn!(container_id = %container_id, error = %err, "failed to remove container");
    }
}

// ── Process engine (non-container) ──────────────────────────────────────────

async fn run_process_engine(
    handle: &Arc<EngineTaskHandle>,
    stop_rx: &mut watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut child = spawn_process(&handle.config).await?;
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
    let _ = handle.session_writer.append(&LogSession {
        id: session_id.clone(),
        started_at: session_started_at.clone(),
        stopped_at: None,
    }).await;
    let mut session_stopped_at: Option<String> = None;
    tracing::info!(
        engine_id = %handle.config.id,
        pid = pid,
        "engine process spawned"
    );
    if wait_for_ready {
        set_status(handle, EngineStatus::Starting, pid, None).await;
    } else {
        set_status(handle, EngineStatus::Running, pid, Some(now())).await;
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
        let wait_ready = async {
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
            ready = wait_ready => {
                if ready {
                    set_status(handle, EngineStatus::Running, pid, Some(now())).await;
                }
            }
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    tracing::info!(engine_id = %handle.config.id, pid = pid, "stop signal received");
                    terminate_process_tree(&mut child).await;
                    set_status(handle, EngineStatus::Stopped, None, None).await;
                    should_monitor = false;
                    session_stopped_at = Some(now());
                }
            }
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code());
                tracing::info!(engine_id = %handle.config.id, pid = pid, exit_code = code, "engine process exited");
                set_exit_status(handle, code).await;
                should_monitor = false;
                session_stopped_at = Some(now());
            }
        }
    }

    if should_monitor {
        tokio::select! {
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    tracing::info!(engine_id = %handle.config.id, pid = pid, "stop signal received");
                    terminate_process_tree(&mut child).await;
                    set_status(handle, EngineStatus::Stopped, None, None).await;
                    session_stopped_at = Some(now());
                }
            }
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code());
                tracing::info!(engine_id = %handle.config.id, pid = pid, exit_code = code, "engine process exited");
                set_exit_status(handle, code).await;
                session_stopped_at = Some(now());
            }
        }
    }

    let stopped_at = session_stopped_at.unwrap_or_else(now);
    let _ = handle.session_writer.append(&LogSession {
        id: session_id,
        started_at: session_started_at,
        stopped_at: Some(stopped_at),
    }).await;

    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }
    Ok(())
}

// Build and spawn the engine child process (non-container).
async fn spawn_process(config: &EngineConfig) -> anyhow::Result<tokio::process::Child> {
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

// ── Container spec resolution ────────────────────────────────────────────────

struct ResolvedContainerSpec {
    image: String,
    ports: Vec<String>,
    volumes: Vec<String>,
    env: Vec<aiman_shared::EnvVar>,
    user: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    gpus: Option<String>,
    remove: bool,
}

async fn resolve_container_image(
    config: &EngineConfig,
    images: &Arc<RwLock<HashMap<String, ContainerImage>>>,
) -> anyhow::Result<ContainerImage> {
    let container = config
        .container
        .as_ref()
        .context("container config missing")?;
    let images = images.read().await;
    images
        .get(&container.image_id)
        .cloned()
        .context("container image not found")
}

fn resolve_container_spec(config: &EngineConfig, image: &ContainerImage) -> ResolvedContainerSpec {
    let container = config.container.as_ref();
    let extra_ports = container
        .map(|c| c.extra_ports.clone())
        .unwrap_or_default();
    let extra_volumes = container
        .map(|c| c.extra_volumes.clone())
        .unwrap_or_default();
    let extra_env = container
        .map(|c| c.extra_env.clone())
        .unwrap_or_default();

    let mut ports = image.ports.clone();
    ports.extend(extra_ports);
    let mut volumes = image.volumes.clone();
    volumes.extend(extra_volumes);
    let mut env = image.env.clone();
    env.extend(extra_env);
    env.extend(config.env.clone());

    let remove = container
        .and_then(|c| c.remove)
        .unwrap_or(image.remove);

    let user = container
        .and_then(|c| c.user.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| image.user.clone());
    let command = container
        .and_then(|c| c.command.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| image.command.clone());

    let mut args = image.args.clone();
    if let Some(c) = container {
        args.extend(c.args.iter().cloned());
    }

    // ContainerConfig.gpus overrides image-level gpus when set.
    let gpus = container
        .and_then(|c| c.gpus.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| image.gpus.clone());

    // For Dockerfile-based images (no image reference), use the local build tag.
    let resolved_image = if image.image.trim().is_empty() {
        format!("aiman-build:{}", image.id)
    } else {
        normalize_image_name(&image.image)
    };

    ResolvedContainerSpec {
        image: resolved_image,
        ports,
        volumes,
        env,
        user,
        command,
        args,
        gpus,
        remove,
    }
}

fn container_name_from_config(config: &EngineConfig, container: &ContainerConfig) -> String {
    container
        .container_name
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .unwrap_or_else(|| config.id.clone())
}

// ── Container image build (podman CLI) ──────────────────────────────────────

async fn build_container_image(
    build: &ContainerBuild,
    image: &str,
    image_config_id: &str,
) -> anyhow::Result<()> {
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("container image is required for build");
    }
    let content = build
        .dockerfile_content
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("dockerfile_content is required for build"))?;

    // Write Dockerfile into a temp dir and use that dir as the build context.
    let build_dir = std::env::temp_dir().join(format!("aiman-build-{}", Utc::now().timestamp_millis()));
    tokio::fs::create_dir_all(&build_dir).await?;
    tokio::fs::write(build_dir.join("Dockerfile"), content).await?;

    let mut command = Command::new("podman");
    command.arg("build").arg("-t").arg(image);
    command
        .arg("--label").arg("managed-by=aiman")
        .arg("--label").arg(format!("aiman.image-id={image_config_id}"));
    if build.pull {
        command.arg("--pull");
    }
    if build.no_cache {
        command.arg("--no-cache");
    }
    for entry in &build.build_args {
        if !entry.key.trim().is_empty() {
            command
                .arg("--build-arg")
                .arg(format!("{}={}", entry.key, entry.value));
        }
    }
    command.arg(&build_dir);

    let output = command.output().await?;
    let _ = tokio::fs::remove_dir_all(&build_dir).await;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        anyhow::bail!("podman build failed: {}", detail);
    }
    Ok(())
}

// ── Shared log helpers ──────────────────────────────────────────────────────

async fn emit_log(
    handle: &Arc<EngineTaskHandle>,
    stream: LogStream,
    session_id: &str,
    line: String,
) {
    let entry = LogEntry {
        ts: now(),
        session_id: session_id.to_string(),
        stream,
        line,
    };
    {
        let mut buffer = handle.log_buffer.lock().await;
        if buffer.len() >= LOG_BUFFER_MAX {
            buffer.pop_front();
        }
        buffer.push_back(entry.clone());
    }
    let _ = handle.log_writer.append(&entry).await;
    let _ = handle.log_tx.send(entry);
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
        emit_log(&handle, stream.clone(), &session_id, line).await;
    }
    tracing::debug!(
        engine_id = %handle.config.id,
        stream = ?stream,
        "log stream ended"
    );
}

// ── Status helpers ──────────────────────────────────────────────────────────

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
    let _ = handle.status_writer.append(&snapshot).await;
    // Push status change to SSE subscribers; ignore if no receivers.
    let _ = handle.status_tx.send(snapshot.clone());
    tracing::debug!(
        engine_id = %handle.config.id,
        status = ?snapshot.status,
        pid = pid,
        "engine status updated"
    );
}

async fn set_exit_status(handle: &EngineTaskHandle, code: Option<i32>) {
    let mut instance = handle.instance.write().await;
    instance.status = EngineStatus::Stopped;
    instance.pid = None;
    instance.last_exit_code = code;
    instance.last_exit_at = Some(now());
    let snapshot = instance.clone();
    drop(instance);
    let _ = handle.status_writer.append(&snapshot).await;
    // Push exit status to SSE subscribers; ignore if no receivers.
    let _ = handle.status_tx.send(snapshot.clone());
    tracing::debug!(
        engine_id = %handle.config.id,
        exit_code = code,
        "engine exit status recorded"
    );
}

fn should_restart(policy: &AutoRestart, retries: u32) -> bool {
    policy.enabled && retries < policy.max_retries
}

async fn terminate_process_tree(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            let pgid = pid as i32;
            unsafe {
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
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn new_session_id() -> String {
    Utc::now().timestamp_millis().to_string()
}
