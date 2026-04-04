import Fastify from "fastify";
import fastifyStatic from "@fastify/static";
import websocketPlugin from "@fastify/websocket";
import path from "node:path";
import { fileURLToPath } from "node:url";
import WebSocket from "ws";

import { initHostPaths, loadHosts, findHost, persistHosts, validateHost } from "./hosts";
import { proxyRequest, buildQueryString } from "./proxy";
import {
  initBenchmarkPath,
  runDashboardBenchmark,
  appendDashboardBenchmark,
  readDashboardBenchmarks
} from "./benchmark";
import type { HostConfig } from "./types";

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

// Wire up path dependencies for sub-modules.
initHostPaths({ configPath, hostsStorePath });
initBenchmarkPath({ dashboardBenchmarksPath });

// Dashboard API server and UI static host.
const server = Fastify({ logger: true });

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

  const { status, body } = await proxyRequest(host, "/v1/models/scan", {
    method: "POST",
    body: { libraries }
  });
  return reply.code(status).send(body);
});

// Proxy hardware info for a selected host.
server.get("/api/hosts/:hostId/hardware", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, "/v1/hardware");
  return reply.code(status).send(body);
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

  const { status, body } = await proxyRequest(host, "/v1/configs");
  return reply.code(status).send(body);
});

// Proxy docker image list for a selected host.
server.get("/api/hosts/:hostId/images", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, "/v1/images");
  return reply.code(status).send(body);
});

// Create docker image template on a host.
server.post("/api/hosts/:hostId/images", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, "/v1/images", {
    method: "POST",
    body: request.body ?? {}
  });
  return reply.code(status).send(body);
});

// Update docker image template on a host.
server.put("/api/hosts/:hostId/images/:imageId", async (request, reply) => {
  const { hostId, imageId } = request.params as { hostId: string; imageId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(
    host,
    `/v1/images/${encodeURIComponent(imageId)}`,
    { method: "PUT", body: request.body ?? {} }
  );
  return reply.code(status).send(body);
});

// Delete docker image template on a host.
server.delete("/api/hosts/:hostId/images/:imageId", async (request, reply) => {
  const { hostId, imageId } = request.params as { hostId: string; imageId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(
    host,
    `/v1/images/${encodeURIComponent(imageId)}`,
    { method: "DELETE" }
  );
  return reply.code(status).send(body);
});

// Prune orphaned aiman-managed Docker images on a host.
server.post("/api/hosts/:hostId/images/prune", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, "/v1/images/prune", { method: "POST" });
  return reply.code(status).send(body);
});

// Proxy create config to the selected host.
server.post("/api/hosts/:hostId/configs", async (request, reply) => {
  const { hostId } = request.params as { hostId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, "/v1/configs", {
    method: "POST",
    body: request.body ?? {}
  });
  return reply.code(status).send(body);
});

// Proxy update config to the selected host.
server.put("/api/hosts/:hostId/configs/:configId", async (request, reply) => {
  const { hostId, configId } = request.params as { hostId: string; configId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, `/v1/configs/${configId}`, {
    method: "PUT",
    body: request.body ?? {}
  });
  return reply.code(status).send(body);
});

// Proxy delete config to the selected host.
server.delete("/api/hosts/:hostId/configs/:configId", async (request, reply) => {
  const { hostId, configId } = request.params as { hostId: string; configId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, `/v1/configs/${configId}`, {
    method: "DELETE"
  });
  return reply.code(status).send(body);
});

// Proxy start command to the selected host.
server.post("/api/hosts/:hostId/engines/:engineId/start", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, `/v1/engines/${engineId}/start`, {
    method: "POST"
  });
  return reply.code(status).send(body);
});

// Proxy stop command to the selected host.
server.post("/api/hosts/:hostId/engines/:engineId/stop", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const { status, body } = await proxyRequest(host, `/v1/engines/${engineId}/stop`, {
    method: "POST"
  });
  return reply.code(status).send(body);
});

// Run a benchmark via llama-benchy on the dashboard machine.
server.post("/api/hosts/:hostId/engines/:engineId/benchmark", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const payload = request.body as Record<string, unknown> | null;
  try {
    const record = await runDashboardBenchmark(host, engineId, payload ?? {});
    await appendDashboardBenchmark(record);
    return reply.code(200).send({ record });
  } catch (err) {
    const message = err instanceof Error ? err.message : "benchmark failed";
    return reply.code(400).send({ error: message });
  }
});

// Proxy engine log history to the selected host.
server.get("/api/hosts/:hostId/engines/:engineId/logs", async (request, reply) => {
  const { hostId, engineId } = request.params as { hostId: string; engineId: string };
  const host = await findHost(hostId);
  if (!host) {
    return reply.code(404).send({ error: "unknown host" });
  }

  const query = request.query as Record<string, string | string[] | undefined>;
  const qs = buildQueryString(query);
  const url = `${host.base_url}/v1/engines/${engineId}/logs${qs}`;
  const res = await fetch(url, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await res.json().catch(() => null);
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
  const qs = buildQueryString(query);
  const url = `${host.base_url}/v1/engines/${engineId}/logs/sessions${qs}`;
  const res = await fetch(url, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await res.json().catch(() => null);
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
  const qs = buildQueryString(query);
  const url = `${host.base_url}/v1/engines/${engineId}/status${qs}`;
  const res = await fetch(url, {
    headers: host.api_key ? { Authorization: `Bearer ${host.api_key}` } : undefined
  });
  const body = await res.json().catch(() => null);
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
