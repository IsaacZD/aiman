//! Benchmark execution via llama-benchy subprocess.

use std::time::Duration;

use axum::extract::{Path, State};
use axum::Json;
use chrono::Utc;
use reqwest::Method;
use tokio::process::Command;

use crate::error::DashboardError;
use crate::hosts::find_host;
use crate::state::AppState;
use crate::types::{BenchmarkPayload, BenchmarkRecord, BenchmarkSettings, HostSnapshot};

/// Run a benchmark via llama-benchy on the dashboard machine.
pub async fn run_benchmark(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
    Json(payload): Json<BenchmarkPayload>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    let host = {
        let hosts = state.hosts.read().await;
        find_host(&hosts, &host_id)
            .cloned()
            .ok_or(DashboardError::HostNotFound)?
    };

    // Fetch engine list to find instance status and config_id
    let (_, engines_body): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(
            Method::GET,
            &host.base_url,
            "/v1/engines",
            host.api_key.as_deref(),
            None::<&()>,
        )
        .await
        .map_err(|e| DashboardError::BenchmarkFailed(format!("failed to fetch engines: {e}")))?;

    let engines = engines_body.get("engines").and_then(|e| e.as_array());
    let instance = engines
        .and_then(|arr| arr.iter().find(|e| e.get("id").and_then(|v| v.as_str()) == Some(&engine_id)));

    let instance = instance
        .ok_or_else(|| DashboardError::BenchmarkFailed(format!("engine {engine_id} not found")))?;

    let status = instance
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("Unknown");

    if status != "Running" {
        return Err(DashboardError::BenchmarkFailed(format!(
            "engine is not running (status: {status})"
        )));
    }

    let config_id = instance
        .get("config_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&engine_id);

    // Fetch all configs and find the one matching this engine
    let (_, configs_body): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(
            Method::GET,
            &host.base_url,
            "/v1/configs",
            host.api_key.as_deref(),
            None::<&()>,
        )
        .await
        .map_err(|e| DashboardError::BenchmarkFailed(format!("failed to fetch configs: {e}")))?;

    let config_value = configs_body
        .get("configs")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.iter().find(|c| c.get("id").and_then(|v| v.as_str()) == Some(config_id)))
        .cloned()
        .ok_or_else(|| DashboardError::BenchmarkFailed(format!("config {config_id} not found")))?;

    let config: aiman_shared::EngineConfig = serde_json::from_value(config_value)
        .map_err(|e| DashboardError::BenchmarkFailed(format!("invalid config: {e}")))?;

    // Fetch hardware info
    let (_, hardware_body): (_, serde_json::Value) = state
        .proxy_client
        .request::<serde_json::Value>(
            Method::GET,
            &host.base_url,
            "/v1/hardware",
            host.api_key.as_deref(),
            None::<&()>,
        )
        .await
        .unwrap_or_default();

    let hardware: Option<aiman_shared::HardwareInfo> =
        serde_json::from_value(hardware_body).ok();

    // Determine API base URL for the engine
    let api_base_url = payload.api_base_url.clone().unwrap_or_else(|| {
        // Try to infer from container config
        if let Some(ref container) = config.container {
            // Fetch image to get port mapping
            let image_id = &container.image_id;
            // For now, assume default port 8000
            format!("{}/v1", host.base_url.trim_end_matches('/'))
        } else {
            format!("{}/v1", host.base_url.trim_end_matches('/'))
        }
    });

    // Build llama-benchy arguments
    let mut args = vec![
        "--base-url".to_string(),
        api_base_url.clone(),
    ];

    if let Some(ref model) = payload.model {
        args.push("--model".to_string());
        args.push(model.clone());
    }

    if !payload.pp.is_empty() {
        args.push("--pp".to_string());
        for n in &payload.pp {
            args.push(n.to_string());
        }
    }

    if !payload.tg.is_empty() {
        args.push("--tg".to_string());
        for n in &payload.tg {
            args.push(n.to_string());
        }
    }

    if !payload.depth.is_empty() {
        args.push("--depth".to_string());
        for n in &payload.depth {
            args.push(n.to_string());
        }
    }

    if let Some(runs) = payload.runs {
        args.push("--runs".to_string());
        args.push(runs.to_string());
    }

    if !payload.concurrency.is_empty() {
        args.push("--concurrency".to_string());
        for n in &payload.concurrency {
            args.push(n.to_string());
        }
    }

    if payload.prefix_caching == Some(true) {
        args.push("--enable-prefix-caching".to_string());
    }

    if let Some(ref mode) = payload.latency_mode {
        args.push("--latency-mode".to_string());
        args.push(mode.clone());
    }

    if payload.no_warmup == Some(true) {
        args.push("--no-warmup".to_string());
    }

    // Run llama-benchy
    tracing::info!(args = ?args, "running llama-benchy");

    let output = run_subprocess("llama-benchy", &args, Duration::from_secs(600))
        .await
        .map_err(|e| DashboardError::BenchmarkFailed(e.to_string()))?;

    // Build record
    let record = BenchmarkRecord {
        id: format!("bench-{}", Utc::now().timestamp_millis()),
        ts: Utc::now().to_rfc3339(),
        host: Some(HostSnapshot {
            id: host.id.clone(),
            name: host.name.clone(),
            base_url: host.base_url.clone(),
        }),
        host_hardware: hardware,
        engine_config: config,
        engine_status: status.to_string(),
        settings: BenchmarkSettings {
            model: payload.model.unwrap_or_default(),
            api_base_url,
            pp: payload.pp,
            tg: payload.tg,
            depth: payload.depth,
            runs: payload.runs.unwrap_or(1),
            concurrency: payload.concurrency,
            prefix_caching: payload.prefix_caching.unwrap_or(false),
            latency_mode: payload.latency_mode.unwrap_or_else(|| "api".to_string()),
            no_warmup: payload.no_warmup.unwrap_or(false),
        },
        output,
    };

    // Append to JSONL
    let _ = state.benchmark_writer.append(&record).await;

    Ok(Json(serde_json::json!({ "record": record })))
}

async fn run_subprocess(cmd: &str, args: &[String], timeout: Duration) -> anyhow::Result<String> {
    let mut command = Command::new(cmd);
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("{cmd} not found in PATH - install via: pip install llama-benchy")
        } else {
            anyhow::anyhow!("failed to spawn {cmd}: {e}")
        }
    })?;

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("{cmd} timed out after {} seconds", timeout.as_secs()))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{cmd} exited with code {:?}: {}",
            output.status.code(),
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
