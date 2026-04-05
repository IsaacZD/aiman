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
use bollard::{
    container::LogOutput,
    models::{ContainerCreateBody, DeviceMapping, HostConfig, PortBinding},
    query_parameters::{
        CreateContainerOptions, LogsOptions, RemoveContainerOptions,
        StartContainerOptions, StopContainerOptions, WaitContainerOptions,
    },
    Docker,
};
use chrono::Utc;
use futures_util::StreamExt;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{broadcast, watch, Mutex, RwLock},
    task::JoinHandle,
};

use super::error::SupervisorError;
use super::store::{append_jsonl, append_session};

// Keep a bounded in-memory log buffer per engine for WS backfill.
const LOG_BUFFER_MAX: usize = 2000;

// Per-engine state + control handles.
pub struct EngineHandle {
    pub(super) config: EngineConfig,
    pub(super) images: Arc<RwLock<HashMap<String, DockerImage>>>,
    pub(super) docker_client: Arc<Docker>,
    pub(super) instance: Arc<RwLock<EngineInstance>>,
    pub log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    pub log_tx: broadcast::Sender<LogEntry>,
    pub(super) status_tx: broadcast::Sender<EngineInstance>,
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
    pub(super) fn new(
        config: EngineConfig,
        images: Arc<RwLock<HashMap<String, DockerImage>>>,
        docker_client: Arc<Docker>,
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
            docker_client,
            instance: Arc::new(RwLock::new(instance)),
            log_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_BUFFER_MAX))),
            log_tx,
            status_tx,
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
            docker_client: self.docker_client.clone(),
            instance: self.instance.clone(),
            log_buffer: self.log_buffer.clone(),
            log_tx: self.log_tx.clone(),
            status_tx: self.status_tx.clone(),
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
    docker_client: Arc<Docker>,
    instance: Arc<RwLock<EngineInstance>>,
    log_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    log_tx: broadcast::Sender<LogEntry>,
    // Broadcast engine status changes to SSE subscribers.
    status_tx: broadcast::Sender<EngineInstance>,
    log_path: PathBuf,
    session_path: PathBuf,
    status_path: PathBuf,
    log_write_lock: Arc<Mutex<()>>,
    session_write_lock: Arc<Mutex<()>>,
    status_write_lock: Arc<Mutex<()>>,
}

// Route to Docker API or process-based engine loop.
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

        let result = if matches!(handle.config.engine_type, EngineType::Docker) {
            run_docker_engine(&handle, &mut stop_rx).await
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

// ── Docker engine (bollard) ───────────────────────────────────────────────────

async fn run_docker_engine(
    handle: &Arc<EngineTaskHandle>,
    stop_rx: &mut watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let image = resolve_docker_image(&handle.config, &handle.images).await?;
    let resolved = resolve_docker_spec(&handle.config, &image);

    // Build/label phase: attach managed-by=aiman label so orphaned images can be pruned.
    if let Some(build) = &resolved.build {
        build_docker_image(build, &resolved.image, &image.id).await?;
    } else if resolved.pull {
        // No custom Dockerfile: build a one-line FROM wrapper with --pull so Docker
        // fetches the latest registry image and we can attach our labels in one step.
        label_docker_image(&resolved.image, &image.id).await?;
    }
    // else: pull=false — use the existing local image as-is; no labeling.

    // Container name: prefer explicit config, fall back to engine id.
    let container_name = handle
        .config
        .docker
        .as_ref()
        .map(|d| docker_container_name(&handle.config, d))
        .unwrap_or_else(|| handle.config.id.clone());

    // Remove any existing container with the same name to avoid 409 conflicts.
    remove_docker_container(&handle.docker_client, &container_name).await;

    // Create container.
    let container_id =
        create_docker_container(handle, &resolved, &container_name).await?;

    // Start container.
    handle
        .docker_client
        .start_container(&container_id, None::<StartContainerOptions>)
        .await
        .context("failed to start container")?;

    tracing::info!(
        engine_id = %handle.config.id,
        container_id = %container_id,
        "docker container started"
    );

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

    // Spawn log streaming task.
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
                    stop_docker_container_api(&handle.docker_client, &container_id).await;
                    if resolved.remove {
                        remove_docker_container(&handle.docker_client, &container_id).await;
                    }
                    set_status(handle, EngineStatus::Stopped, None, None).await;
                    session_stopped_at = Some(now());
                    finalize_session(handle, session_id, session_started_at, session_stopped_at, log_task, health_task).await;
                    return Ok(());
                }
            }
        }
    }

    // Wait for container exit or stop signal.
    let mut wait_stream = handle
        .docker_client
        .wait_container(&container_id, None::<WaitContainerOptions>);

    tokio::select! {
        _ = stop_rx.changed() => {
            if *stop_rx.borrow() {
                tracing::info!(
                    engine_id = %handle.config.id,
                    container_id = %container_id,
                    "stop signal received; stopping container"
                );
                stop_docker_container_api(&handle.docker_client, &container_id).await;
                if resolved.remove {
                    remove_docker_container(&handle.docker_client, &container_id).await;
                }
                set_status(handle, EngineStatus::Stopped, None, None).await;
                session_stopped_at = Some(now());
            }
        }
        result = wait_stream.next() => {
            let code = result
                .and_then(|r| r.ok())
                .map(|r| i32::try_from(r.status_code).unwrap_or(-1));
            tracing::info!(
                engine_id = %handle.config.id,
                container_id = %container_id,
                exit_code = code,
                "docker container exited"
            );
            if resolved.remove {
                remove_docker_container(&handle.docker_client, &container_id).await;
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
    append_session(
        &handle.session_path,
        &handle.session_write_lock,
        &LogSession {
            id: session_id,
            started_at: session_started_at,
            stopped_at: Some(stopped_at),
        },
    )
    .await;
    health_task.abort();
    let _ = log_task.await;
}

/// Pull-only path: build a one-line `FROM <image>` Dockerfile with --pull so
/// Docker fetches the latest registry image and we can attach aiman labels in
/// the same step.  The resulting local image keeps the original tag.
async fn label_docker_image(image: &str, image_config_id: &str) -> anyhow::Result<()> {
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("docker image is required for pull");
    }
    tracing::info!(image = %image, image_config_id = %image_config_id, "labeling docker image via build");

    let build_dir = std::env::temp_dir()
        .join(format!("aiman-label-{}", Utc::now().timestamp_millis()));
    tokio::fs::create_dir_all(&build_dir).await?;
    tokio::fs::write(build_dir.join("Dockerfile"), format!("FROM {image}")).await?;

    let output = Command::new("docker")
        .arg("build")
        .arg("-t").arg(image)
        .arg("--pull")
        .arg("--label").arg("managed-by=aiman")
        .arg("--label").arg(format!("aiman.image-id={image_config_id}"))
        .arg(&build_dir)
        .output()
        .await?;
    let _ = tokio::fs::remove_dir_all(&build_dir).await;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker label step failed: {}", stderr.trim());
    }
    Ok(())
}

/// Build the bollard container config and call create_container.
async fn create_docker_container(
    handle: &Arc<EngineTaskHandle>,
    resolved: &ResolvedDockerSpec,
    container_name: &str,
) -> anyhow::Result<String> {
    // Port bindings: "host:container" or "host:container/proto".
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    let mut exposed_port_list: Vec<String> = Vec::new();
    for port_spec in &resolved.ports {
        let spec = port_spec.trim();
        if spec.is_empty() {
            continue;
        }
        // Format: [host_ip:]host_port:container_port[/proto]
        let parts: Vec<&str> = spec.splitn(3, ':').collect();
        let (host_port, container_port_proto) = match parts.len() {
            2 => (parts[0], parts[1]),
            3 => (parts[1], parts[2]),
            _ => continue,
        };
        // container_port may include /proto; add default /tcp if not.
        let container_key = if container_port_proto.contains('/') {
            container_port_proto.to_string()
        } else {
            format!("{container_port_proto}/tcp")
        };
        exposed_port_list.push(container_key.clone());
        port_bindings.insert(
            container_key,
            Some(vec![PortBinding {
                host_ip: None,
                host_port: Some(host_port.to_string()),
            }]),
        );
    }

    // Volume bindings: "/host:/container[:options]"
    let binds: Vec<String> = resolved
        .volumes
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();

    // GPU devices: map to /dev/nvidia* device paths.
    let devices = resolved.gpus.as_deref().and_then(|gpus| {
        if gpus.is_empty() {
            return None;
        }

        let mut device_mappings = Vec::new();

        // Always include control devices
        device_mappings.push(DeviceMapping {
            path_on_host: Some("/dev/nvidiactl".into()),
            path_in_container: Some("/dev/nvidiactl".into()),
            cgroup_permissions: Some("rwm".into()),
        });
        device_mappings.push(DeviceMapping {
            path_on_host: Some("/dev/nvidia-uvm".into()),
            path_in_container: Some("/dev/nvidia-uvm".into()),
            cgroup_permissions: Some("rwm".into()),
        });
        device_mappings.push(DeviceMapping {
            path_on_host: Some("/dev/nvidia-uvm-tools".into()),
            path_in_container: Some("/dev/nvidia-uvm-tools".into()),
            cgroup_permissions: Some("rwm".into()),
        });

        // Add GPU devices based on the gpus value
        if gpus == "all" {
            // Add devices 0-15 (should cover most systems)
            for i in 0..16 {
                device_mappings.push(DeviceMapping {
                    path_on_host: Some(format!("/dev/nvidia{}", i)),
                    path_in_container: Some(format!("/dev/nvidia{}", i)),
                    cgroup_permissions: Some("rwm".into()),
                });
            }
        } else {
            // Parse specific GPU IDs (e.g., "0", "0,1", "1,3")
            for gpu_id in gpus.split(',') {
                let gpu_id = gpu_id.trim();
                if let Ok(id) = gpu_id.parse::<u32>() {
                    device_mappings.push(DeviceMapping {
                        path_on_host: Some(format!("/dev/nvidia{}", id)),
                        path_in_container: Some(format!("/dev/nvidia{}", id)),
                        cgroup_permissions: Some("rwm".into()),
                    });
                }
            }
        }

        Some(device_mappings)
    });

    let host_config = HostConfig {
        port_bindings: if port_bindings.is_empty() {
            None
        } else {
            Some(port_bindings)
        },
        binds: if binds.is_empty() { None } else { Some(binds) },
        devices,
        ..Default::default()
    };

    // Env: "KEY=VALUE"
    let env: Vec<String> = resolved
        .env
        .iter()
        .filter(|e| !e.key.trim().is_empty())
        .map(|e| format!("{}={}", e.key, e.value))
        .collect();

    // Command: entrypoint override + args.
    let cmd: Option<Vec<String>> = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(command) = resolved
            .command
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            parts.extend(split_command_input(command));
        }
        for arg in &resolved.args {
            parts.extend(split_command_input(arg));
        }
        if parts.is_empty() { None } else { Some(parts) }
    };

    let config = ContainerCreateBody {
        image: Some(resolved.image.trim().to_string()),
        cmd,
        env: if env.is_empty() { None } else { Some(env) },
        user: resolved
            .user
            .as_ref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        exposed_ports: if exposed_port_list.is_empty() {
            None
        } else {
            Some(exposed_port_list)
        },
        host_config: Some(host_config),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(container_name.to_string()),
        platform: String::new(),
    };

    let response = handle
        .docker_client
        .create_container(Some(options), config)
        .await
        .context("failed to create container")?;

    Ok(response.id)
}

/// Stream container stdout/stderr via bollard into the log buffer.
async fn stream_container_logs(
    handle: Arc<EngineTaskHandle>,
    container_id: String,
    session_id: String,
    ready_tx: Option<watch::Sender<bool>>,
    ready_marker: Option<String>,
) {
    let mut logs = handle.docker_client.logs(
        &container_id,
        Some(LogsOptions {
            follow: true,
            stdout: true,
            stderr: true,
            timestamps: false,
            ..Default::default()
        }),
    );

    while let Some(result) = logs.next().await {
        let (stream, bytes) = match result {
            Ok(LogOutput::StdOut { message }) => (LogStream::Stdout, message),
            Ok(LogOutput::StdErr { message }) => (LogStream::Stderr, message),
            Ok(_) => continue,
            Err(err) => {
                tracing::debug!(engine_id = %handle.config.id, error = %err, "log stream error");
                break;
            }
        };
        let line = String::from_utf8_lossy(&bytes).trim_end_matches('\n').to_string();
        if line.is_empty() {
            continue;
        }
        if let (Some(tx), Some(marker)) = (&ready_tx, &ready_marker) {
            if line.contains(marker) {
                let _ = tx.send(true);
            }
        }
        emit_log(&handle, stream, &session_id, line).await;
    }

    tracing::debug!(engine_id = %handle.config.id, "container log stream ended");
}

/// Poll container health status every 30 s and update EngineInstance.health.
async fn poll_container_health(handle: Arc<EngineTaskHandle>, container_id: String) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        match handle.docker_client.inspect_container(&container_id, None).await {
            Ok(info) => {
                let health_status = info
                    .state
                    .and_then(|s| s.health)
                    .and_then(|h| h.status)
                    .map(|s| s.to_string());
                if let Some(status) = health_status {
                    let mut instance = handle.instance.write().await;
                    instance.health = Some(status);
                }
            }
            Err(_) => break, // Container gone; task will be aborted anyway.
        }
    }
}

async fn stop_docker_container_api(docker: &Docker, container_id: &str) {
    let options = StopContainerOptions { t: Some(10), signal: None };
    if let Err(err) = docker.stop_container(container_id, Some(options)).await {
        tracing::warn!(container_id = %container_id, error = %err, "failed to stop container");
    }
}

async fn remove_docker_container(docker: &Docker, container_id: &str) {
    let options = RemoveContainerOptions { force: true, ..Default::default() };
    if let Err(err) = docker.remove_container(container_id, Some(options)).await {
        tracing::warn!(container_id = %container_id, error = %err, "failed to remove container");
    }
}

// ── Process engine (non-Docker) ──────────────────────────────────────────────

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
    append_session(
        &handle.session_path,
        &handle.session_write_lock,
        &LogSession {
            id: session_id,
            started_at: session_started_at,
            stopped_at: Some(stopped_at),
        },
    )
    .await;

    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }
    Ok(())
}

// Build and spawn the engine child process (non-Docker).
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

// ── Docker spec resolution ────────────────────────────────────────────────────

struct ResolvedDockerSpec {
    image: String,
    ports: Vec<String>,
    volumes: Vec<String>,
    env: Vec<aiman_shared::EnvVar>,
    user: Option<String>,
    command: Option<String>,
    args: Vec<String>,
    gpus: Option<String>,
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
        .map(|d| d.extra_ports.clone())
        .unwrap_or_default();
    let extra_volumes = docker
        .map(|d| d.extra_volumes.clone())
        .unwrap_or_default();
    let extra_env = docker
        .map(|d| d.extra_env.clone())
        .unwrap_or_default();

    let mut ports = image.ports.clone();
    ports.extend(extra_ports);
    let mut volumes = image.volumes.clone();
    volumes.extend(extra_volumes);
    let mut env = image.env.clone();
    env.extend(extra_env);
    env.extend(config.env.clone());

    let pull = docker
        .and_then(|d| d.pull)
        .unwrap_or(image.pull);
    let remove = docker
        .and_then(|d| d.remove)
        .unwrap_or(image.remove);

    let user = docker
        .and_then(|d| d.user.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| image.user.clone());
    let command = docker
        .and_then(|d| d.command.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| image.command.clone());

    let mut args = image.args.clone();
    if let Some(d) = docker {
        args.extend(d.args.iter().cloned());
    }

    // DockerConfig.gpus overrides image-level gpus when set.
    let gpus = docker
        .and_then(|d| d.gpus.clone())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| image.gpus.clone());

    ResolvedDockerSpec {
        image: image.image.clone(),
        ports,
        volumes,
        env,
        user,
        command,
        args,
        gpus,
        pull,
        remove,
        build: image.build.clone(),
    }
}

fn docker_container_name(config: &EngineConfig, docker: &DockerConfig) -> String {
    docker
        .container_name
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .unwrap_or_else(|| config.id.clone())
}

// ── Docker image build (CLI — kept for tar-context simplicity) ────────────────

async fn build_docker_image(
    build: &DockerBuild,
    image: &str,
    image_config_id: &str,
) -> anyhow::Result<()> {
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("docker image is required for build");
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

    let mut command = Command::new("docker");
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
        anyhow::bail!("docker build failed: {}", detail);
    }
    Ok(())
}

// ── Shared log helpers ────────────────────────────────────────────────────────

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
    append_jsonl(&handle.log_path, &handle.log_write_lock, &entry).await;
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

// ── Status helpers ────────────────────────────────────────────────────────────

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
    append_jsonl(&handle.status_path, &handle.status_write_lock, &snapshot).await;
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
