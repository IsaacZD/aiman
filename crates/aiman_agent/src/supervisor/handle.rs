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
    pub(super) instance: Arc<RwLock<EngineInstance>>,
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
    pub(super) fn new(
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
