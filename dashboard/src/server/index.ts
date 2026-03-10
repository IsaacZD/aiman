import Fastify from "fastify";
import fastifyStatic from "@fastify/static";
import { readFile } from "node:fs/promises";
import path from "node:path";
import toml from "toml";

const server = Fastify({ logger: true });

const repoRoot = path.resolve(__dirname, "../../..");
const configPath = process.env.AIMAN_HOSTS_CONFIG ?? path.join(repoRoot, "configs", "hosts.toml");
const uiDir = path.resolve(__dirname, "../../dist/ui");

server.get("/health", async () => ({ status: "ok" }));

server.get("/api/hosts", async () => {
  const raw = await readFile(configPath, "utf8");
  const data = toml.parse(raw) as { host?: unknown };
  return { hosts: Array.isArray(data.host) ? data.host : [] };
});

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
