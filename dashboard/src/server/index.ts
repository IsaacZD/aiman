import Fastify from "fastify";
import fastifyStatic from "@fastify/static";
import websocketPlugin from "@fastify/websocket";
import { readFile } from "node:fs/promises";
import path from "node:path";
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

const server = Fastify({ logger: true });

const repoRoot = path.resolve(__dirname, "../../..");
const configPath = process.env.AIMAN_HOSTS_CONFIG ?? path.join(repoRoot, "configs", "hosts.toml");
const uiDir = path.resolve(__dirname, "../../dist/ui");

server.register(websocketPlugin);

server.get("/health", async () => ({ status: "ok" }));

server.get("/api/hosts", async () => {
  const hosts = await loadHosts();
  return { hosts };
});

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

async function loadHosts(): Promise<HostConfig[]> {
  const raw = await readFile(configPath, "utf8");
  const data = toml.parse(raw) as HostsFile;
  return Array.isArray(data.host) ? data.host : [];
}

async function findHost(id: string): Promise<HostConfig | undefined> {
  const hosts = await loadHosts();
  return hosts.find((host) => host.id === id);
}

async function safeJson(res: Response) {
  try {
    return await res.json();
  } catch {
    return null;
  }
}
