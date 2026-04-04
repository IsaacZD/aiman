import type { HostConfig } from "./types";

// Best-effort JSON parsing for proxied responses.
export async function safeJson(res: Response) {
  try {
    return await res.json();
  } catch {
    return null;
  }
}

// Build a URLSearchParams string from a query record, supporting array values.
export function buildQueryString(query: Record<string, string | string[] | undefined>): string {
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
  return search.toString() ? `?${search}` : "";
}

type ProxyRequestOptions = {
  method?: string;
  body?: unknown;
  query?: Record<string, string | string[] | undefined>;
};

// Generic helper that forwards a request to a host and returns { status, body }.
export async function proxyRequest(
  host: HostConfig,
  hostPath: string,
  opts: ProxyRequestOptions = {}
): Promise<{ status: number; body: unknown }> {
  const { method = "GET", body, query } = opts;

  const qs = query ? buildQueryString(query) : "";
  const url = `${host.base_url}${hostPath}${qs}`;

  const headers: Record<string, string> = {};
  if (host.api_key) {
    headers["Authorization"] = `Bearer ${host.api_key}`;
  }
  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }

  const res = await fetch(url, {
    method,
    headers: Object.keys(headers).length ? headers : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined
  });

  const parsed = await safeJson(res);
  return { status: res.status, body: parsed ?? { ok: res.ok } };
}
