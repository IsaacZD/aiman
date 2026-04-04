import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import type {
  BenchmarkRecord,
  BenchmarkSettings,
  DockerImage,
  EngineConfig,
  EngineInstance,
  HardwareInfo,
  HostConfig
} from "./types";

export type { BenchmarkRecord };

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

  const apiBaseUrl =
    typeof payload.api_base_url === "string" && payload.api_base_url.trim()
      ? normalizeBaseUrl(payload.api_base_url)
      : inferApiBase(config, host, image ?? undefined);
  if (!apiBaseUrl) {
    throw new Error("unable to infer engine API base URL");
  }

  const apiKey =
    typeof payload.api_key === "string" && payload.api_key.trim()
      ? payload.api_key.trim()
      : undefined;

  const model =
    typeof payload.model === "string" && payload.model.trim()
      ? payload.model.trim()
      : await fetchDefaultModel(apiBaseUrl, apiKey);

  const pp = parseNumberList(payload.pp, [512, 2048]);
  const tg = parseNumberList(payload.tg, [32, 128]);
  const depth = parseNumberList(payload.depth, [0]);
  const runs = Math.max(1, Number(payload.runs) || 3);
  const concurrency = parseNumberList(payload.concurrency, [1]);
  const prefixCaching = Boolean(payload.prefix_caching);
  const latencyMode =
    typeof payload.latency_mode === "string" ? payload.latency_mode : "generation";
  const noWarmup = Boolean(payload.no_warmup);

  const args: string[] = [
    "--base-url",
    `${apiBaseUrl}/v1`,
    "--model",
    model,
    "--runs",
    String(runs),
    "--latency-mode",
    latencyMode
  ];
  for (const v of pp) args.push("--pp", String(v));
  for (const v of tg) args.push("--tg", String(v));
  for (const v of depth) args.push("--depth", String(v));
  for (const v of concurrency) args.push("--concurrency", String(v));
  if (apiKey) args.push("--api-key", apiKey);
  if (prefixCaching) args.push("--enable-prefix-caching");
  if (noWarmup) args.push("--no-warmup");

  const output = await runSubprocess("llama-benchy", args);

  const settings: BenchmarkSettings = {
    model,
    api_base_url: apiBaseUrl,
    pp,
    tg,
    depth,
    runs,
    concurrency,
    prefix_caching: prefixCaching,
    latency_mode: latencyMode
  };

  return {
    id: `bench-${Date.now()}`,
    ts: new Date().toISOString(),
    host: { id: host.id, name: host.name, base_url: host.base_url },
    host_hardware: hardware,
    engine_config: config,
    engine_status: instance.status,
    settings,
    output
  };
}

// Spawns a subprocess and resolves with its combined stdout. Rejects on non-zero
// exit, timeout (10 min default), or if the binary is not found.
function runSubprocess(cmd: string, args: string[], timeoutMs = 600_000): Promise<string> {
  return new Promise((resolve, reject) => {
    let proc: ReturnType<typeof spawn>;
    try {
      proc = spawn(cmd, args, { stdio: ["ignore", "pipe", "pipe"] });
    } catch (err) {
      reject(err);
      return;
    }

    const stdout: string[] = [];
    const stderr: string[] = [];
    proc.stdout.on("data", (chunk: Buffer) => stdout.push(chunk.toString()));
    proc.stderr.on("data", (chunk: Buffer) => stderr.push(chunk.toString()));

    const timer = setTimeout(() => {
      proc.kill("SIGTERM");
      reject(new Error(`llama-benchy timed out after ${timeoutMs / 1000}s`));
    }, timeoutMs);

    proc.on("error", (err: NodeJS.ErrnoException) => {
      clearTimeout(timer);
      if (err.code === "ENOENT") {
        reject(new Error("llama-benchy not found in PATH — install it with: pip install llama-benchy"));
      } else {
        reject(err);
      }
    });

    proc.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) {
        const tail = stderr.join("").slice(-500);
        reject(new Error(`llama-benchy exited with code ${code}${tail ? `: ${tail}` : ""}`));
      } else {
        resolve(stdout.join(""));
      }
    });
  });
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

async function fetchDefaultModel(apiBaseUrl: string, apiKey?: string): Promise<string> {
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

function parseNumberList(value: unknown, fallback: number[]): number[] {
  const list = Array.isArray(value)
    ? value.map(Number)
    : typeof value === "string"
      ? value.split(",").map((s) => Number(s.trim()))
      : [];
  const filtered = list.filter((n) => Number.isFinite(n) && n >= 0);
  return filtered.length ? filtered : fallback;
}

function normalizeBaseUrl(value: string) {
  return value.trim().replace(/\/$/, "");
}

function inferApiBase(config: EngineConfig, host: HostConfig, image?: DockerImage) {
  const dockerArgs = config.docker?.args ?? [];
  const dockerPorts = [...(image?.ports ?? []), ...(config.docker?.extra_ports ?? [])];
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
  for (let i = 0; i < args.length; i++) {
    const v = args[i];
    if (v === key) return args[i + 1];
    if (v.startsWith(`${key}=`)) return v.slice(key.length + 1);
  }
  return null;
}

function parseDockerHostPort(ports: string[]) {
  for (const mapping of ports) {
    const port = parseDockerPortMapping(mapping);
    if (port) return String(port);
  }
  return null;
}

function parseDockerPortMapping(mapping: string) {
  const trimmed = mapping.trim();
  if (!trimmed) return null;
  const noProto = trimmed.split("/")[0] ?? trimmed;
  const parts = noProto.split(":");
  const hostPort = parts.length >= 2 ? parts[parts.length - 2] : parts[0];
  const parsed = Number(hostPort);
  return Number.isFinite(parsed) ? parsed : null;
}

function defaultPort(engineType: EngineConfig["engine_type"]) {
  if (engineType === "LlamaCpp" || engineType === "ik_llamacpp" || engineType === "fastllm") {
    return 8080;
  }
  return 8000;
}
