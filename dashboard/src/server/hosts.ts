import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import toml from "toml";
import type { HostConfig, HostsFile } from "./types";

export type { HostConfig };

// Paths are injected at module init to keep this module testable.
let configPath: string;
let hostsStorePath: string;

export function initHostPaths(opts: { configPath: string; hostsStorePath: string }) {
  configPath = opts.configPath;
  hostsStorePath = opts.hostsStorePath;
}

// Load hosts from JSON store, falling back to TOML config.
export async function loadHosts(): Promise<HostConfig[]> {
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
export async function findHost(id: string): Promise<HostConfig | undefined> {
  const hosts = await loadHosts();
  return hosts.find((host) => host.id === id);
}

// Persist the host list to the JSON store.
export async function persistHosts(hosts: HostConfig[]) {
  const dir = path.dirname(hostsStorePath);
  await mkdir(dir, { recursive: true });
  await writeFile(hostsStorePath, JSON.stringify(hosts, null, 2));
}

// Validate a host payload, returning an ok/error discriminated union.
export function validateHost(payload: Partial<HostConfig>): { ok: true } | { ok: false; error: string } {
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
