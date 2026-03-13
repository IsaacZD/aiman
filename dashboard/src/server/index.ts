import Fastify from "fastify";
import fastifyStatic from "@fastify/static";
import websocketPlugin from "@fastify/websocket";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import toml from "toml";
import WebSocket from "ws";

type HostConfig = {
  id: string;
  name: string;
  base_url: string;
  api_key?: string;
  model_libraries?: string[];
};

type HostsFile = {
  host?: HostConfig[];
};

type EngineConfig = {
  id: string;
  name: string;
  // Keep in sync with crates/shared EngineType (string values serialized over the wire).
  engine_type: "Vllm" | "LlamaCpp" | "ik_llamacpp" | "Lvllm" | "fastllm" | "KTransformers" | "Custom";
  command: string;
  args: string[];
  env: { key: string; value: string }[];
  working_dir?: string | null;
  auto_restart: {
    enabled: boolean;
    max_retries: number;
    backoff_secs: number;
  };
};

type EngineInstance = {
  id: string;
  config_id: string;
  status: string;
  pid?: number | null;
  ts?: string;
};

type HardwareInfo = {
  hostname?: string | null;
  os_name?: string | null;
  os_version?: string | null;
  kernel_version?: string | null;
  cpu_brand?: string | null;
  cpu_cores_logical?: number | null;
  cpu_cores_physical?: number | null;
  cpu_frequency_mhz?: number | null;
  memory_total_kb?: number | null;
  memory_available_kb?: number | null;
  swap_total_kb?: number | null;
  swap_free_kb?: number | null;
  uptime_seconds?: number | null;
  gpus?: {
    name?: string | null;
    vendor?: string | null;
    memory_total_mb?: number | null;
    driver_version?: string | null;
  }[];
};

type BenchmarkSettings = {
  concurrency: number[];
  requests_per_concurrency: number;
  prompt: string;
  prompt_words: number;
  max_tokens: number;
  temperature: number;
  model: string;
  api_base_url: string;
  timeout_seconds: number;
};

type BenchmarkResult = {
  concurrency: number;
  requests: number;
  success_count: number;
  error_count: number;
  duration_ms: number;
  avg_latency_ms: number;
  min_latency_ms: number;
  max_latency_ms: number;
  p50_latency_ms: number;
  p90_latency_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  prompt_tps: number;
  completion_tps: number;
  requests_per_sec: number;
  errors: string[];
};

type BenchmarkRecord = {
  id: string;
  ts: string;
  origin?: "host" | "dashboard";
  host?: { id: string; name: string; base_url: string } | null;
  host_hardware?: HardwareInfo | null;
  engine_config: EngineConfig;
  engine_status: string;
  settings: BenchmarkSettings;
  results: BenchmarkResult[];
};

// Dashboard API server and UI static host.
const server = Fastify({ logger: true });

// Resolve paths relative to the repo root for config + built UI.
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "../../..");
const configPath = process.env.AIMAN_HOSTS_CONFIG ?? path.join(repoRoot, "configs", "hosts.toml");
const hostsStorePath =
  process.env.AIMAN_HOSTS_STORE ?? path.join(repoRoot, "data", "hosts.json");
const dashboardBenchmarksPath =
  process.env.AIMAN_DASHBOARD_BENCHMARKS ??
  path.join(repoRoot, "data", "benchmarks-dashboard.jsonl");
const uiDir = path.resolve(__dirname, "../../dist/ui");

server.register(websocketPlugin);

server.get("/health", async () => ({ status: "ok" }));

// Expose configured host list to the UI.
server.get("/api/hosts", async () => {
  const hosts = await loadHosts();
  return { hosts };
});

// Create a new host entry in the store.
server.post("/api/hosts", async (request, reply) => {
  const payload = request.body as Partial<HostConfig>;
  const validation = validateHost(payload);
  if (!validation.ok) {
    return reply.code(400).send({ error: validation.error });
  }

  const hosts = await loadHosts();
  if (hosts.some((host) => host.id === payload.id)) {
    return reply.code(409).send({ error: "host already exists" });
  }

  const next = [...hosts, payload as HostConfig];
  await persistHosts(next);
  return reply.code(201).send({ host: payload });
});

// Update an existing host entry.
server.put("/api/hosts/:hostId", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const payload = request.body as Partial<HostConfig>;
  const validation = validateHost(payload);
  if (!validation.ok) {
    return reply.code(400).send({ error: validation.error });
  }
  if (payload.id !== hostId) {
    return reply.code(400).send({ error: "host id mismatch" });
  }

  const hosts = await loadHosts();
  const index = hosts.findIndex((host) => host.id === hostId);
  if (index === -1) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const next = [...hosts];
  next[index] = payload as HostConfig;
  await persistHosts(next);
  return reply.code(200).send({ host: payload });
});

// Delete a host entry from the store.
server.delete("/api/hosts/:hostId", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const hosts = await loadHosts();
  const next = hosts.filter((host) => host.id !== hostId);
  if (next.length === hosts.length) {
    return reply.code(404).send({ error: "unknown host" });
  }
  await persistHosts(next);
  return reply.code(200).send({ ok: true });
});

// Proxy model scan request to the selected host.
server.get("/api/hosts/:hostId/models", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const libraries = Array.isArray(host.model_libraries) ? host.model_libraries : [];
  if (!libraries.length) {
    return reply.code(200).send({ artifacts: [] });
  }

  const res = await fetch(`${host.base_url}/v1/models/scan`, {
    method: "POST",
    headers: {
      ...(host.api_key ? { Authorization: `Bearer ${host.api_key}` } : {}),
      "Content-Type": "application/json"
    },
    body: JSON.stringify({ libraries })
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy hardware info for a selected host.
server.get("/api/hosts/:hostId/hardware", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/hardware`, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Aggregate engine lists across all configured hosts.
server.get("/api/engines", async () => {
  const hosts = await loadHosts();
  const results = await Promise.all(
    hosts.map(async (host) => {
      try {
        const res = await fetch(`${host.base_url}/v1/engines`, {
          headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
        });
        if (!res.ok) {
          return { host, error: `HTTP ${res.status}` };
        }
        const body = (await res.json()) as { engines: unknown[] };
        return { host, engines: body.engines };
      } catch (err) {
        return { host, error: (err as Error).message };
      }
    })
  );

  return { results };
});

// Proxy config list for a selected host.
server.get("/api/hosts/:hostId/configs", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/configs`, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy create config to the selected host.
server.post("/api/hosts/:hostId/configs", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/configs`, {
    method: "POST",
    headers: {
      ...(host.api_key ? { Authorization: `Bearer ${host.api_key}` } : {}),
      "Content-Type": "application/json"
    },
    body: JSON.stringify(request.body ?? {})
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy update config to the selected host.
server.put("/api/hosts/:hostId/configs/:configId", async (request, reply) => {
  const { hostId, configId } = request.params as { hostId: string; configId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/configs/${configId}`, {
    method: "PUT",
    headers: {
      ...(host.api_key ? { Authorization: `Bearer ${host.api_key}` } : {}),
      "Content-Type": "application/json"
    },
    body: JSON.stringify(request.body ?? {})
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy delete config to the selected host.
server.delete("/api/hosts/:hostId/configs/:configId", async (request, reply) => {
  const { hostId, configId } = request.params as { hostId: string; configId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/configs/${configId}`, {
    method: "DELETE",
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy start command to the selected host.
server.post("/api/hosts/:hostId/engines/:engineId/start", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/engines/${engineId}/start`, {
    method: "POST",
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy stop command to the selected host.
server.post("/api/hosts/:hostId/engines/:engineId/stop", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const res = await fetch(`${host.base_url}/v1/engines/${engineId}/stop`, {
    method: "POST",
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy benchmark run to the selected host.
server.post("/api/hosts/:hostId/engines/:engineId/benchmark", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const payload = request.body as Record<string, unknown> | null;
  const mode =
    payload && "mode" in payload && typeof payload.mode === "string"
      ? payload.mode
      : "host";

  if (mode === "dashboard") {
    try {
      const record = await runDashboardBenchmark(host, engineId, payload ?? {});
      await appendDashboardBenchmark(record);
      return reply.code(200).send({ record });
    } catch (err) {
      const message = err instanceof Error ? err.message : "benchmark failed";
      return reply.code(400).send({ error: message });
    }
  }

  const settings =
    payload && "settings" in payload ? (payload as { settings: unknown }).settings : payload;
  const requestBody = {
    settings: settings ?? {},
    host: {
      id: host.id,
      name: host.name,
      base_url: host.base_url
    }
  };

  const res = await fetch(`${host.base_url}/v1/engines/${engineId}/benchmark`, {
    method: "POST",
    headers: {
      ...(host.api_key ? { Authorization: `Bearer ${host.api_key}` } : {}),
      "Content-Type": "application/json"
    },
    body: JSON.stringify(requestBody)
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy engine log history to the selected host.
server.get("/api/hosts/:hostId/engines/:engineId/logs", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const query = request.query as Record<string, string | string[] | undefined>;
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (Array.isArray(value)) {
      for (const item of value) {
        search.append(key, item);
      }
    } else if (value !== undefined) {
      search.set(key, String(value));
    }
  }

  const url = `${host.base_url}/v1/engines/${engineId}/logs${search.toString() ? `?${search}` : ""}`;
  const res = await fetch(url, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy engine log sessions to the selected host.
server.get("/api/hosts/:hostId/engines/:engineId/logs/sessions", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const query = request.query as Record<string, string | string[] | undefined>;
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (Array.isArray(value)) {
      for (const item of value) {
        search.append(key, item);
      }
    } else if (value !== undefined) {
      search.set(key, String(value));
    }
  }

  const url = `${host.base_url}/v1/engines/${engineId}/logs/sessions${
    search.toString() ? `?${search}` : ""
  }`;
  const res = await fetch(url, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Proxy engine status history to the selected host.
server.get("/api/hosts/:hostId/engines/:engineId/status", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const query = request.query as Record<string, string | string[] | undefined>;
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (Array.isArray(value)) {
      for (const item of value) {
        search.append(key, item);
      }
    } else if (value !== undefined) {
      search.set(key, String(value));
    }
  }

  const url = `${host.base_url}/v1/engines/${engineId}/status${search.toString() ? `?${search}` : ""}`;
  const res = await fetch(url, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
});

// Aggregate benchmark records across all configured hosts.
server.get("/api/benchmarks", async () => {
  const hosts = await loadHosts();
  const localRecords = await readDashboardBenchmarks();
  const results = await Promise.all(
    hosts.map(async (host) => {
      try {
        const res = await fetch(`${host.base_url}/v1/benchmarks`, {
          headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
        });
        if (!res.ok) {
          return { host, error: `HTTP ${res.status}` };
        }
        const body = (await res.json()) as { records?: unknown[] };
        return { host, records: body.records ?? [] };
      } catch (err) {
        return { host, error: (err as Error).message };
      }
    })
  );

  return { results, local: localRecords };
});

// Bridge WS log stream from host -> browser.
server.get(
  "/api/hosts/:hostId/engines/:engineId/logs/ws",
  { websocket: true },
  async (connection, request) => {
    const { hostId, engineId } = request.params as { hostId: string; engineId: string };
    const host = await findHost(hostId);
    if (!host) {
      connection.socket.close();
      return;
    }

    const targetUrl = `${host.base_url.replace(/\/$/, "")}/v1/engines/${engineId}/logs/ws`;
    const upstream = new WebSocket(targetUrl, {
      headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
    });

    upstream.on("message", (data) => {
      if (connection.socket.readyState === WebSocket.OPEN) {
        connection.socket.send(data.toString());
      }
    });

    upstream.on("close", () => {
      if (connection.socket.readyState === WebSocket.OPEN) {
        connection.socket.close();
      }
    });

    connection.socket.on("close", () => {
      upstream.close();
    });
  }
);

// Serve built UI assets.
server.register(fastifyStatic, {
  root: uiDir,
  prefix: "/"
});

const port = Number(process.env.AIMAN_DASHBOARD_PORT ?? "4020");
const host = process.env.AIMAN_DASHBOARD_BIND ?? "0.0.0.0";

server.listen({ port, host }).catch((err) => {
  server.log.error(err);
  process.exit(1);
});

// Load hosts from TOML config file.
async function loadHosts(): Promise<HostConfig[]> {
  try {
    const raw = await readFile(hostsStorePath, "utf8");
    if (!raw.trim()) {
      return [];
    }
    const parsed = JSON.parse(raw) as HostConfig[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    // Fallback to TOML if no JSON store exists yet.
    try {
      const raw = await readFile(configPath, "utf8");
      const data = toml.parse(raw) as HostsFile;
      const hosts = Array.isArray(data.host) ? data.host : [];
      if (hosts.length) {
        await persistHosts(hosts);
      }
      return hosts;
    } catch {
      return [];
    }
  }
}

// Find a host by ID.
async function findHost(id: string): Promise<HostConfig | undefined> {
  const hosts = await loadHosts();
  return hosts.find((host) => host.id === id);
}

// Best-effort JSON parsing for proxied responses.
async function safeJson(res: Response) {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

async function persistHosts(hosts: HostConfig[]) {
  const dir = path.dirname(hostsStorePath);
  await mkdir(dir, { recursive: true });
  await writeFile(hostsStorePath, JSON.stringify(hosts, null, 2));
}

function validateHost(payload: Partial<HostConfig>) {
  if (!payload.id?.trim()) {
    return { ok: false, error: "id is required" };
  }
  if (!payload.name?.trim()) {
    return { ok: false, error: "name is required" };
  }
  if (!payload.base_url?.trim()) {
    return { ok: false, error: "base_url is required" };
  }
  return { ok: true };
}

async function appendDashboardBenchmark(record: BenchmarkRecord) {
  const dir = path.dirname(dashboardBenchmarksPath);
  await mkdir(dir, { recursive: true });
  const line = `${JSON.stringify(record)}\n`;
  await writeFile(dashboardBenchmarksPath, line, { flag: "a" });
}

async function readDashboardBenchmarks(): Promise<BenchmarkRecord[]> {
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

async function runDashboardBenchmark(
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

  if (instance.status !== "Running") {
    throw new Error(`engine is not running (status ${instance.status})`);
  }

  const resolved = await normalizeBenchmarkSettings(config, host, settingsPayload);
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

type NormalizedBenchmarkSettings = {
  concurrency: number[];
  requestsPerConcurrency: number;
  prompt: string;
  promptWords: number;
  maxTokens: number;
  temperature: number;
  model: string;
  apiBaseUrl: string;
  apiKey?: string;
  timeoutSeconds: number;
};

async function normalizeBenchmarkSettings(
  config: EngineConfig,
  host: HostConfig,
  payload: Record<string, unknown>
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
      : inferApiBase(config, host);
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

type RequestOutcome = {
  latency_ms: number;
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  error?: string;
};

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

function inferApiBase(config: EngineConfig, host: HostConfig) {
  const hostValue =
    parseArgValue(config.args, "--host") ?? parseArgValue(config.args, "--bind") ?? "127.0.0.1";
  const portValue = parseArgValue(config.args, "--port");
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
