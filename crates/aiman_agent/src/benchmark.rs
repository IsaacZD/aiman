use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use aiman_shared::{EngineConfig, EngineInstance, EngineStatus, EngineType};

use crate::hardware::HardwareInfo;

#[derive(Debug, Deserialize)]
pub struct BenchmarkRequest {
    pub settings: BenchmarkRunSettings,
    pub host: Option<BenchmarkHostSnapshot>,
}

#[derive(Debug, Deserialize)]
pub struct BenchmarkRunSettings {
    pub concurrency: Option<Vec<usize>>,
    pub requests_per_concurrency: Option<usize>,
    pub prompt: Option<String>,
    pub prompt_words: Option<usize>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub model: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key: Option<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BenchmarkSettings {
    pub concurrency: Vec<usize>,
    pub requests_per_concurrency: usize,
    pub prompt: String,
    pub prompt_words: usize,
    pub max_tokens: u32,
    pub temperature: f32,
    pub model: String,
    pub api_base_url: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BenchmarkHostSnapshot {
    pub id: String,
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkRecord {
    pub id: String,
    pub ts: String,
    pub host: Option<BenchmarkHostSnapshot>,
    pub host_hardware: Option<HardwareInfo>,
    pub engine_config: EngineConfig,
    pub engine_status: EngineStatus,
    pub settings: BenchmarkSettings,
    pub results: Vec<BenchmarkResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub concurrency: usize,
    pub requests: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub duration_ms: u64,
    pub avg_latency_ms: u64,
    pub min_latency_ms: u64,
    pub max_latency_ms: u64,
    pub p50_latency_ms: u64,
    pub p90_latency_ms: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub prompt_tps: f64,
    pub completion_tps: f64,
    pub requests_per_sec: f64,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct RequestOutcome {
    latency_ms: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
struct UsageInfo {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

pub async fn run_benchmark(
    config: EngineConfig,
    instance: EngineInstance,
    host: Option<BenchmarkHostSnapshot>,
    host_hardware: Option<HardwareInfo>,
    request: BenchmarkRunSettings,
) -> anyhow::Result<BenchmarkRecord> {
    let defaults = NormalizedSettings::from_request(&config, request).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(defaults.timeout_seconds))
        .build()
        .context("create benchmark HTTP client")?;

    let mut results = Vec::new();
    for &concurrency in &defaults.concurrency {
        let outcome = run_concurrency(
            &client,
            &defaults,
            concurrency,
            defaults.requests_per_concurrency,
        )
        .await;
        results.push(outcome);
    }

    let record = BenchmarkRecord {
        id: format!("bench-{}", Utc::now().timestamp_millis()),
        ts: Utc::now().to_rfc3339(),
        host,
        host_hardware,
        engine_config: config,
        engine_status: instance.status,
        settings: BenchmarkSettings {
            concurrency: defaults.concurrency.clone(),
            requests_per_concurrency: defaults.requests_per_concurrency,
            prompt: defaults.prompt.clone(),
            prompt_words: defaults.prompt_words,
            max_tokens: defaults.max_tokens,
            temperature: defaults.temperature,
            model: defaults.model.clone(),
            api_base_url: defaults.api_base_url.clone(),
            timeout_seconds: defaults.timeout_seconds,
        },
        results,
    };

    Ok(record)
}

async fn run_concurrency(
    client: &reqwest::Client,
    settings: &NormalizedSettings,
    concurrency: usize,
    total_requests: usize,
) -> BenchmarkResult {
    let total_requests = total_requests.max(1);
    let concurrency = concurrency.max(1);
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let start = tokio::time::Instant::now();
    let mut handles = Vec::with_capacity(total_requests);

    for _ in 0..total_requests {
        let semaphore = semaphore.clone();
        let client = client.clone();
        let settings = settings.clone();
        handles.push(tokio::spawn(async move {
            let permit = match semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => {
                    return RequestOutcome {
                        latency_ms: 0,
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        total_tokens: 0,
                        error: Some("semaphore closed".to_string()),
                    }
                }
            };
            let _permit = permit;
            run_request(&client, &settings).await
        }));
    }

    let mut outcomes = Vec::with_capacity(total_requests);
    for handle in handles {
        match handle.await {
            Ok(outcome) => outcomes.push(outcome),
            Err(err) => outcomes.push(RequestOutcome {
                latency_ms: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                error: Some(format!("worker join error: {err}")),
            }),
        }
    }

    let duration = start.elapsed();
    summarize_results(concurrency, total_requests, duration, outcomes)
}

async fn run_request(
    client: &reqwest::Client,
    settings: &NormalizedSettings,
) -> RequestOutcome {
    let start = tokio::time::Instant::now();
    let req = client
        .post(format!("{}/v1/chat/completions", settings.api_base_url))
        .json(&serde_json::json!({
            "model": &settings.model,
            "messages": [
                { "role": "user", "content": &settings.prompt }
            ],
            "max_tokens": settings.max_tokens,
            "temperature": settings.temperature
        }));

    let req = if let Some(api_key) = settings.api_key.as_deref() {
        req.bearer_auth(api_key)
    } else {
        req
    };

    let response = match req.send().await {
        Ok(resp) => resp,
        Err(err) => {
            return RequestOutcome {
                latency_ms: start.elapsed().as_millis() as u64,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                error: Some(format!("request failed: {err}")),
            }
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return RequestOutcome {
            latency_ms: start.elapsed().as_millis() as u64,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            error: Some(format!("HTTP {status}: {body}")),
        };
    }

    let body = match response.json::<ChatCompletionResponse>().await {
        Ok(body) => body,
        Err(err) => {
            return RequestOutcome {
                latency_ms: start.elapsed().as_millis() as u64,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                error: Some(format!("decode failed: {err}")),
            }
        }
    };

    let usage = body.usage.unwrap_or(UsageInfo {
        prompt_tokens: Some(0),
        completion_tokens: Some(0),
        total_tokens: Some(0),
    });

    let prompt_tokens = usage.prompt_tokens.unwrap_or(0);
    let completion_tokens = usage.completion_tokens.unwrap_or(0);
    let total_tokens = usage
        .total_tokens
        .unwrap_or(prompt_tokens + completion_tokens);

    RequestOutcome {
        latency_ms: start.elapsed().as_millis() as u64,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        error: None,
    }
}

fn summarize_results(
    concurrency: usize,
    requests: usize,
    duration: Duration,
    outcomes: Vec<RequestOutcome>,
) -> BenchmarkResult {
    let duration_ms = duration.as_millis() as u64;
    let duration_secs = duration.as_secs_f64().max(0.001);
    let mut latencies = Vec::new();
    let mut prompt_tokens = 0u64;
    let mut completion_tokens = 0u64;
    let mut total_tokens = 0u64;
    let mut errors = Vec::new();

    for outcome in outcomes.iter() {
        if let Some(err) = outcome.error.as_ref() {
            if errors.len() < 6 {
                errors.push(err.clone());
            }
            continue;
        }
        latencies.push(outcome.latency_ms);
        prompt_tokens += outcome.prompt_tokens;
        completion_tokens += outcome.completion_tokens;
        total_tokens += outcome.total_tokens;
    }

    let success_count = latencies.len();
    let error_count = requests.saturating_sub(success_count);
    latencies.sort_unstable();
    let avg_latency_ms = if success_count > 0 {
        latencies.iter().sum::<u64>() / success_count as u64
    } else {
        0
    };
    let min_latency_ms = latencies.first().copied().unwrap_or(0);
    let max_latency_ms = latencies.last().copied().unwrap_or(0);
    let p50_latency_ms = percentile(&latencies, 0.5);
    let p90_latency_ms = percentile(&latencies, 0.9);

    BenchmarkResult {
        concurrency,
        requests,
        success_count,
        error_count,
        duration_ms,
        avg_latency_ms,
        min_latency_ms,
        max_latency_ms,
        p50_latency_ms,
        p90_latency_ms,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        prompt_tps: prompt_tokens as f64 / duration_secs,
        completion_tps: completion_tokens as f64 / duration_secs,
        requests_per_sec: success_count as f64 / duration_secs,
        errors,
    }
}

fn percentile(values: &[u64], pct: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let rank = ((values.len() as f64 - 1.0) * pct).round() as usize;
    values[rank.min(values.len() - 1)]
}

#[derive(Clone)]
struct NormalizedSettings {
    concurrency: Vec<usize>,
    requests_per_concurrency: usize,
    prompt: String,
    prompt_words: usize,
    max_tokens: u32,
    temperature: f32,
    model: String,
    api_base_url: String,
    api_key: Option<String>,
    timeout_seconds: u64,
}

impl NormalizedSettings {
    async fn from_request(
        config: &EngineConfig,
        request: BenchmarkRunSettings,
    ) -> anyhow::Result<Self> {
        let concurrency = request
            .concurrency
            .unwrap_or_else(|| vec![1, 2, 4, 8])
            .into_iter()
            .filter(|value| *value > 0)
            .collect::<Vec<_>>();
        let concurrency = if concurrency.is_empty() {
            vec![1]
        } else {
            concurrency
        };
        let requests_per_concurrency = request.requests_per_concurrency.unwrap_or(8).max(1);
        let requested_words = request.prompt_words.unwrap_or(120).max(1);
        let prompt = request
            .prompt
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| generate_prompt(requested_words));
        let prompt_words = count_words(&prompt).max(1);
        let max_tokens = request.max_tokens.unwrap_or(256).max(1);
        let temperature = request.temperature.unwrap_or(0.2).clamp(0.0, 2.0);
        let api_base_url = request
            .api_base_url
            .and_then(|value| normalize_base_url(&value))
            .or_else(|| infer_api_base(config))
            .context("unable to infer engine API base URL")?;

        let model = match request.model {
            Some(model) if !model.trim().is_empty() => model,
            _ => fetch_default_model(&api_base_url, request.api_key.as_deref()).await?,
        };

        Ok(Self {
            concurrency,
            requests_per_concurrency,
            prompt,
            prompt_words,
            max_tokens,
            temperature,
            model,
            api_base_url,
            api_key: request.api_key,
            timeout_seconds: request.timeout_seconds.unwrap_or(90).max(10),
        })
    }
}

async fn fetch_default_model(api_base_url: &str, api_key: Option<&str>) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("create model discovery client")?;
    let req = client.get(format!("{}/v1/models", api_base_url));
    let req = if let Some(api_key) = api_key {
        req.bearer_auth(api_key)
    } else {
        req
    };
    let response = req.send().await.context("fetch model list")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("model list request failed (HTTP {}): {}", status, body);
    }
    let body = response.json::<ModelListResponse>().await?;
    body.data
        .first()
        .map(|model| model.id.clone())
        .context("model list returned no models")
}

fn normalize_base_url(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn infer_api_base(config: &EngineConfig) -> Option<String> {
    let mut host = parse_arg_value(&config.args, "--host")
        .or_else(|| parse_arg_value(&config.args, "--bind"))
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = parse_arg_value(&config.args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| default_port(&config.engine_type));
    if host == "0.0.0.0" || host == "::" {
        host = "127.0.0.1".to_string();
    }
    Some(format!("http://{host}:{port}"))
}

fn parse_arg_value(args: &[String], key: &str) -> Option<String> {
    for (idx, value) in args.iter().enumerate() {
        if value == key {
            return args.get(idx + 1).cloned();
        }
        if let Some(stripped) = value.strip_prefix(&format!("{key}=")) {
            return Some(stripped.to_string());
        }
    }
    None
}

fn default_port(engine_type: &EngineType) -> u16 {
    match engine_type {
        // Llama.cpp-derived servers typically default to 8080.
        EngineType::LlamaCpp | EngineType::IkLlamaCpp => 8080,
        // FastLLM uses 8080 in its default server example.
        EngineType::Fastllm => 8080,
        // vLLM + most others default to 8000.
        _ => 8000,
    }
}

fn generate_prompt(words: usize) -> String {
    const WORDS: &[&str] = &[
        "ocean", "signal", "ember", "circuit", "memory", "harbor", "silent", "gravity", "silver",
        "atlas", "garden", "vector", "timber", "echo", "planet", "canvas", "mirror", "thread",
        "story", "nebula", "glacier", "pixel", "horizon", "compass", "lattice", "whisper", "orchid",
        "shadow", "river", "lantern",
    ];
    let mut prompt = String::new();
    for idx in 0..words {
        if idx > 0 {
            prompt.push(' ');
        }
        prompt.push_str(WORDS[idx % WORDS.len()]);
    }
    prompt
}

fn count_words(prompt: &str) -> usize {
    prompt.split_whitespace().filter(|word| !word.is_empty()).count()
}
