import Fastify from "fastify";
import fastifyStatic from "@fastify/static";
import websocketPlugin from "@fastify/websocket";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import toml from "toml";
import WebSocket from "ws";

type HostConfig = {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
};

type HostsFile = {
  host?: HostConfig[];
};

// Dashboard API server and UI static host.
const server = Fastify({ logger: true });

// Resolve paths relative to the repo root for config + built UI.
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "../../..");
const configPath = process.env.AIMAN_HOSTS_CONFIG ?? path.join(repoRoot, "configs", "hosts.toml");
const uiDir = path.resolve(__dirname, "../../dist/ui");

server.register(websocketPlugin);

server.get("/health", async () => ({ status: "ok" }));

// Expose configured host list to the UI.
server.get("/api/hosts", async () => {
  const hosts = await loadHosts();
  return { hosts };
});

// Aggregate engine lists across all configured hosts.
server.get("/api/engines", async () => {
  const hosts = await loadHosts();
  const results = await Promise.all(
    hosts.map(async (host) => {
      try {
        const res = await fetch(`${host.base_url}/v1/engines`, {
          headers: { Authorization: `Bearer ${host.api_key}` }
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
    headers: { Authorization: `Bearer ${host.api_key}` }
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
      Authorization: `Bearer ${host.api_key}`,
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
      Authorization: `Bearer ${host.api_key}`,
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
    headers: { Authorization: `Bearer ${host.api_key}` }
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
    headers: { Authorization: `Bearer ${host.api_key}` }
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
    headers: { Authorization: `Bearer ${host.api_key}` }
  });
  const body = await safeJson(res);
  return reply.code(res.status).send(body ?? { ok: res.ok });
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
      headers: { Authorization: `Bearer ${host.api_key}` }
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
  const raw = await readFile(configPath, "utf8");
  const data = toml.parse(raw) as HostsFile;
  return Array.isArray(data.host) ? data.host : [];
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
