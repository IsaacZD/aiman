import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import type {
  BenchmarkRecord,
  BenchmarkResult,
  DockerImage,
  EngineConfig,
  EngineInstance,
  HardwareInfo,
  HostConfig,
  NormalizedBenchmarkSettings,
  RequestOutcome
} from "./types";

export type { BenchmarkRecord, BenchmarkResult };

// Path is injected at module init to keep this module testable.
let dashboardBenchmarksPath: string;

export function initBenchmarkPath(opts: { dashboardBenchmarksPath: string }) {
  dashboardBenchmarksPath = opts.dashboardBenchmarksPath;
}

export async function appendDashboardBenchmark(record: BenchmarkRecord) {
  const dir = path.dirname(dashboardBenchmarksPath);
  await mkdir(dir, { recursive: true });
  const line = `${JSON.stringify(record)}\n`;
  await writeFile(dashboardBenchmarksPath, line, { flag: "a" });
}

export async function readDashboardBenchmarks(): Promise<BenchmarkRecord[]> {
  try {
    const raw = await readFile(dashboardBenchmarksPath, "utf8");
    if (!raw.trim()) {
      return [];
    }
    const records: BenchmarkRecord[] = [];
    for (const line of raw.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) {
        continue;
      }
      try {
        records.push(JSON.parse(trimmed) as BenchmarkRecord);
      } catch {
        continue;
      }
    }
    return records;
  } catch {
    return [];
  }
}

export async function runDashboardBenchmark(
  host: HostConfig,
  engineId: string,
  payload: Record<string, unknown>
): Promise<BenchmarkRecord> {
  const settingsPayload =
    "settings" in payload && typeof payload.settings === "object"
      ? (payload.settings as Record<string, unknown>)
      : payload;

  const [config, instance, hardware] = await Promise.all([
    fetchConfig(host, engineId),
    fetchInstance(host, engineId),
    fetchHardware(host)
  ]);

  const image =
    config.engine_type === "Docker" && config.docker?.image_id
      ? await fetchImage(host, config.docker.image_id)
      : null;

  if (instance.status !== "Running") {
    throw new Error(`engine is not running (status ${instance.status})`);
  }

  const resolved = await normalizeBenchmarkSettings(config, host, settingsPayload, image ?? undefined);
  const results: BenchmarkResult[] = [];
  for (const concurrency of resolved.concurrency) {
    const result = await runBenchmarkConcurrency(resolved, concurrency);
    results.push(result);
  }

  return {
    id: `bench-${Date.now()}`,
    ts: new Date().toISOString(),
    origin: "dashboard",
    host: { id: host.id, name: host.name, base_url: host.base_url },
    host_hardware: hardware,
    engine_config: config,
    engine_status: instance.status,
    settings: {
      concurrency: resolved.concurrency,
      requests_per_concurrency: resolved.requestsPerConcurrency,
      prompt: resolved.prompt,
      prompt_words: resolved.promptWords,
      max_tokens: resolved.maxTokens,
      temperature: resolved.temperature,
      model: resolved.model,
      api_base_url: resolved.apiBaseUrl,
      timeout_seconds: resolved.timeoutSeconds
    },
    results
  };
}

async function fetchConfig(host: HostConfig, engineId: string): Promise<EngineConfig> {
  const res = await fetch(`${host.base_url}/v1/configs`, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  if (!res.ok) {
    throw new Error(`failed to load configs (HTTP ${res.status})`);
  }
  const body = (await res.json()) as { configs?: EngineConfig[] };
  const config = (body.configs ?? []).find((item) => item.id === engineId);
  if (!config) {
    throw new Error("engine config not found");
  }
  return config;
}

async function fetchImage(host: HostConfig, imageId: string): Promise<DockerImage | null> {
  const res = await fetch(`${host.base_url}/v1/images/${encodeURIComponent(imageId)}`, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  if (!res.ok) {
    return null;
  }
  const body = (await res.json()) as { image?: DockerImage };
  return body.image ?? null;
}

async function fetchInstance(host: HostConfig, engineId: string): Promise<EngineInstance> {
  const res = await fetch(`${host.base_url}/v1/engines/${engineId}`, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  if (!res.ok) {
    throw new Error(`failed to load engine (HTTP ${res.status})`);
  }
  const body = (await res.json()) as { instance: EngineInstance };
  return body.instance;
}

async function fetchHardware(host: HostConfig): Promise<HardwareInfo | null> {
  const res = await fetch(`${host.base_url}/v1/hardware`, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  if (!res.ok) {
    return null;
  }
  const body = (await res.json()) as { hardware?: HardwareInfo };
  return body.hardware ?? null;
}

async function normalizeBenchmarkSettings(
  config: EngineConfig,
  host: HostConfig,
  payload: Record<string, unknown>,
  image?: DockerImage
): Promise<NormalizedBenchmarkSettings> {
  const concurrency = parseConcurrency(payload.concurrency).filter((value) => value > 0);
  const resolvedConcurrency = concurrency.length ? concurrency : [1, 2, 4, 8];
  const requestsPerConcurrency = clampNumber(payload.requests_per_concurrency, 8, 1);
  const promptWords = clampNumber(payload.prompt_words, 120, 1);
  const prompt =
    typeof payload.prompt === "string" && payload.prompt.trim().length
      ? payload.prompt.trim()
      : generatePrompt(promptWords);
  const promptWordCount = countWords(prompt);
  const maxTokens = clampNumber(payload.max_tokens, 256, 1);
  const temperature = clampNumber(payload.temperature, 0.2, 0);
  const apiBaseUrl =
    typeof payload.api_base_url === "string" && payload.api_base_url.trim().length
      ? normalizeBaseUrl(payload.api_base_url)
      : inferApiBase(config, host, image);
  if (!apiBaseUrl) {
    throw new Error("unable to infer engine API base URL");
  }

  const apiKey =
    typeof payload.api_key === "string" && payload.api_key.trim().length
      ? payload.api_key.trim()
      : undefined;
  const model =
    typeof payload.model === "string" && payload.model.trim().length
      ? payload.model.trim()
      : await fetchDefaultModel(apiBaseUrl, apiKey);
  const timeoutSeconds = clampNumber(payload.timeout_seconds, 90, 10);

  return {
    concurrency: resolvedConcurrency,
    requestsPerConcurrency,
    prompt,
    promptWords: promptWordCount,
    maxTokens,
    temperature,
    model,
    apiBaseUrl,
    apiKey,
    timeoutSeconds
  };
}

async function runBenchmarkConcurrency(
  settings: NormalizedBenchmarkSettings,
  concurrency: number
): Promise<BenchmarkResult> {
  const totalRequests = Math.max(1, settings.requestsPerConcurrency);
  const limiter = createLimiter(concurrency);
  const start = Date.now();
  const tasks = Array.from({ length: totalRequests }, () =>
    limiter(() => runBenchmarkRequest(settings))
  );
  const outcomes = await Promise.all(tasks);
  const durationMs = Math.max(1, Date.now() - start);
  const latencies: number[] = [];
  let promptTokens = 0;
  let completionTokens = 0;
  let totalTokens = 0;
  const errors: string[] = [];

  outcomes.forEach((outcome) => {
    if (outcome.error) {
      if (errors.length < 6) {
        errors.push(outcome.error);
      }
      return;
    }
    latencies.push(outcome.latency_ms);
    promptTokens += outcome.prompt_tokens;
    completionTokens += outcome.completion_tokens;
    totalTokens += outcome.total_tokens;
  });

  latencies.sort((a, b) => a - b);
  const successCount = latencies.length;
  const durationSecs = durationMs / 1000;
  const avgLatency =
    successCount > 0 ? Math.round(latencies.reduce((sum, value) => sum + value, 0) / successCount) : 0;

  return {
    concurrency,
    requests: totalRequests,
    success_count: successCount,
    error_count: totalRequests - successCount,
    duration_ms: durationMs,
    avg_latency_ms: avgLatency,
    min_latency_ms: latencies[0] ?? 0,
    max_latency_ms: latencies[latencies.length - 1] ?? 0,
    p50_latency_ms: percentile(latencies, 0.5),
    p90_latency_ms: percentile(latencies, 0.9),
    prompt_tokens: promptTokens,
    completion_tokens: completionTokens,
    total_tokens: totalTokens,
    prompt_tps: durationSecs > 0 ? promptTokens / durationSecs : 0,
    completion_tps: durationSecs > 0 ? completionTokens / durationSecs : 0,
    requests_per_sec: durationSecs > 0 ? successCount / durationSecs : 0,
    errors
  };
}

async function runBenchmarkRequest(settings: NormalizedBenchmarkSettings): Promise<RequestOutcome> {
  const start = Date.now();
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), settings.timeoutSeconds * 1000);
  try {
    const res = await fetch(`${settings.apiBaseUrl}/v1/chat/completions`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        ...(settings.apiKey ? { Authorization: `Bearer ${settings.apiKey}` } : {})
      },
      body: JSON.stringify({
        model: settings.model,
        messages: [{ role: "user", content: settings.prompt }],
        max_tokens: settings.maxTokens,
        temperature: settings.temperature
      }),
      signal: controller.signal
    });

    if (!res.ok) {
      const text = await res.text().catch(() => "");
      return {
        latency_ms: Date.now() - start,
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        error: `HTTP ${res.status}: ${text}`
      };
    }

    const body = (await res.json()) as {
      usage?: { prompt_tokens?: number; completion_tokens?: number; total_tokens?: number };
    };
    const usage = body.usage ?? {};
    const promptTokens = usage.prompt_tokens ?? 0;
    const completionTokens = usage.completion_tokens ?? 0;
    const totalTokens = usage.total_tokens ?? promptTokens + completionTokens;

    return {
      latency_ms: Date.now() - start,
      prompt_tokens: promptTokens,
      completion_tokens: completionTokens,
      total_tokens: totalTokens
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : "request failed";
    return {
      latency_ms: Date.now() - start,
      prompt_tokens: 0,
      completion_tokens: 0,
      total_tokens: 0,
      error: message
    };
  } finally {
    clearTimeout(timeout);
  }
}

function createLimiter(limit: number) {
  let active = 0;
  const queue: Array<() => void> = [];
  const runNext = () => {
    if (active >= limit) {
      return;
    }
    const next = queue.shift();
    if (!next) {
      return;
    }
    active += 1;
    next();
  };

  return <T>(task: () => Promise<T>): Promise<T> =>
    new Promise((resolve, reject) => {
      const run = () => {
        task()
          .then(resolve, reject)
          .finally(() => {
            active = Math.max(0, active - 1);
            runNext();
          });
      };
      queue.push(run);
      runNext();
    });
}

function percentile(values: number[], pct: number) {
  if (!values.length) {
    return 0;
  }
  const index = Math.min(values.length - 1, Math.round((values.length - 1) * pct));
  return values[index];
}

function clampNumber(value: unknown, fallback: number, minValue?: number) {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(parsed)) {
    return fallback;
  }
  if (minValue !== undefined && parsed < minValue) {
    return minValue;
  }
  return parsed;
}

function parseConcurrency(value: unknown): number[] {
  if (Array.isArray(value)) {
    return value.map((item) => Number(item)).filter((item) => Number.isFinite(item));
  }
  if (typeof value === "string") {
    return value
      .split(",")
      .map((item) => Number(item.trim()))
      .filter((item) => Number.isFinite(item));
  }
  return [];
}

function normalizeBaseUrl(value: string) {
  const trimmed = value.trim().replace(/\/$/, "");
  return trimmed;
}

function inferApiBase(config: EngineConfig, host: HostConfig, image?: DockerImage) {
  const dockerArgs = config.docker?.args ?? [];
  const dockerPorts = [
    ...(image?.ports ?? []),
    ...(config.docker?.extra_ports ?? [])
  ];
  const hostValue =
    parseArgValue(dockerArgs, "--host") ??
    parseArgValue(dockerArgs, "--bind") ??
    parseArgValue(config.args, "--host") ??
    parseArgValue(config.args, "--bind") ??
    "127.0.0.1";
  const portValue =
    parseArgValue(dockerArgs, "--port") ??
    parseArgValue(config.args, "--port") ??
    parseDockerHostPort(dockerPorts);
  const port = portValue ? Number(portValue) : defaultPort(config.engine_type);
  if (!Number.isFinite(port)) {
    return null;
  }
  const url = new URL(host.base_url);
  const isLocal =
    hostValue === "127.0.0.1" ||
    hostValue === "0.0.0.0" ||
    hostValue === "::" ||
    hostValue === "localhost";
  const resolvedHost = isLocal ? url.hostname : hostValue;
  return `${url.protocol}//${resolvedHost}:${port}`;
}

function parseArgValue(args: string[], key: string) {
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (value === key) {
      return args[index + 1];
    }
    if (value.startsWith(`${key}=`)) {
      return value.slice(key.length + 1);
    }
  }
  return null;
}

function parseDockerHostPort(ports: string[]) {
  for (const mapping of ports) {
    const port = parseDockerPortMapping(mapping);
    if (port) {
      return String(port);
    }
  }
  return null;
}

function parseDockerPortMapping(mapping: string) {
  const trimmed = mapping.trim();
  if (!trimmed) {
    return null;
  }
  const noProto = trimmed.split("/")[0] ?? trimmed;
  const parts = noProto.split(":");
  const hostPort = parts.length >= 2 ? parts[parts.length - 2] : parts[0];
  const parsed = Number(hostPort);
  if (!Number.isFinite(parsed)) {
    return null;
  }
  return parsed;
}

function defaultPort(engineType: EngineConfig["engine_type"]) {
  // Defaults mirror engine vendors to make benchmark inference sensible.
  if (engineType === "LlamaCpp" || engineType === "ik_llamacpp") {
    return 8080;
  }
  if (engineType === "fastllm") {
    return 8080;
  }
  return 8000;
}

function generatePrompt(words: number) {
  const pool = [
    "ocean",
    "signal",
    "ember",
    "circuit",
    "memory",
    "harbor",
    "silent",
    "gravity",
    "silver",
    "atlas",
    "garden",
    "vector",
    "timber",
    "echo",
    "planet",
    "canvas",
    "mirror",
    "thread",
    "story",
    "nebula",
    "glacier",
    "pixel",
    "horizon",
    "compass",
    "lattice",
    "whisper",
    "orchid",
    "shadow",
    "river",
    "lantern"
  ];
  const parts = Array.from({ length: words }, (_, idx) => pool[idx % pool.length]);
  return parts.join(" ");
}

function countWords(value: string) {
  return value.split(/\s+/).filter(Boolean).length;
}

async function fetchDefaultModel(apiBaseUrl: string, apiKey?: string) {
  const res = await fetch(`${apiBaseUrl}/v1/models`, {
    headers: apiKey ? { Authorization: `Bearer ${apiKey}` } : undefined
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`model list request failed (HTTP ${res.status}): ${text}`);
  }
  const body = (await res.json()) as { data?: Array<{ id: string }> };
  const model = body.data?.[0]?.id;
  if (!model) {
    throw new Error("model list returned no models");
  }
  return model;
}
