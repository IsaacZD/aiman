import type { HardwareInfo, LogSession } from "../types";

export function statusClass(status: string) {
  return `status-${status.toLowerCase()}`;
}

export function statusDotClass(status: string) {
  if (status === "Running") {
    return "is-running";
  }
  if (status === "Starting") {
    return "is-starting";
  }
  if (status === "Stopped") {
    return "is-stopped";
  }
  return "is-unknown";
}

export function formatBytesFromKb(value: number | null | undefined): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  const gib = value / 1024 / 1024;
  if (gib >= 100) {
    return `${Math.round(gib)} GB`;
  }
  if (gib >= 10) {
    return `${gib.toFixed(1)} GB`;
  }
  return `${gib.toFixed(2)} GB`;
}

export function formatBytesFromMb(value: number): string {
  const gib = value / 1024;
  if (gib >= 100) {
    return `${Math.round(gib)} GB`;
  }
  if (gib >= 10) {
    return `${gib.toFixed(1)} GB`;
  }
  return `${gib.toFixed(2)} GB`;
}

export function formatCpu(info: HardwareInfo | null | undefined): string {
  if (!info) {
    return "—";
  }
  const brand = info.cpu_brand?.trim();
  const logical = info.cpu_cores_logical ?? null;
  const physical = info.cpu_cores_physical ?? null;
  const coreLabel = physical
    ? `${physical}C/${logical ?? physical}T`
    : logical
    ? `${logical} threads`
    : null;
  const frequency = info.cpu_frequency_mhz ? `${info.cpu_frequency_mhz} MHz` : null;
  const parts = [brand, coreLabel, frequency].filter(Boolean) as string[];
  return parts.length ? parts.join(" • ") : "—";
}

export function formatMemory(info: HardwareInfo | null | undefined): string {
  if (!info) {
    return "—";
  }
  const total = formatBytesFromKb(info.memory_total_kb);
  const free = formatBytesFromKb(info.memory_available_kb);
  if (total && free) {
    return `${total} total • ${free} free`;
  }
  return total ?? free ?? "—";
}

export function formatOs(info: HardwareInfo | null | undefined): string {
  if (!info) {
    return "—";
  }
  const parts = [info.os_name, info.os_version].filter(Boolean);
  const os = parts.length ? parts.join(" ") : null;
  const kernel = info.kernel_version ? `kernel ${info.kernel_version}` : null;
  return [os, kernel].filter(Boolean).join(" • ") || "—";
}

export function formatGpus(info: HardwareInfo | null | undefined): string {
  const gpus = info?.gpus ?? [];
  if (!gpus.length) {
    return "—";
  }
  const grouped = new Map<string, { count: number; memoryLabel?: string }>();
  for (const gpu of gpus) {
    const name = gpu.name?.trim() || gpu.vendor?.trim() || "GPU";
    const memoryLabel = gpu.memory_total_mb ? formatBytesFromMb(gpu.memory_total_mb) : undefined;
    const key = memoryLabel ? `${name} (${memoryLabel})` : name;
    const entry = grouped.get(key);
    if (entry) {
      entry.count += 1;
    } else {
      grouped.set(key, { count: 1, memoryLabel });
    }
  }
  const labels = Array.from(grouped.entries()).map(([label, info]) =>
    info.count > 1 ? `${label} x${info.count}` : label
  );
  return labels.join(" • ");
}

export function formatUptime(info: HardwareInfo | null | undefined): string {
  if (!info) {
    return "—";
  }
  return formatDuration(info.uptime_seconds);
}

export function formatDuration(totalSeconds: number | null | undefined): string {
  if (totalSeconds === null || totalSeconds === undefined) {
    return "—";
  }
  let seconds = Math.max(0, Math.floor(totalSeconds));
  const days = Math.floor(seconds / 86400);
  seconds -= days * 86400;
  const hours = Math.floor(seconds / 3600);
  seconds -= hours * 3600;
  const minutes = Math.floor(seconds / 60);
  const parts: string[] = [];
  if (days) {
    parts.push(`${days}d`);
  }
  if (hours) {
    parts.push(`${hours}h`);
  }
  if (!parts.length) {
    parts.push(`${minutes}m`);
  } else if (parts.length < 2 && minutes) {
    parts.push(`${minutes}m`);
  }
  return parts.join(" ");
}

export function formatBenchmarkTime(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return value;
  }
  return parsed.toLocaleString();
}

export function formatBenchmarkOrigin(origin?: "host" | "dashboard"): string {
  return origin === "dashboard" ? "Dashboard run" : "Host run";
}

export function formatMs(value: number): string {
  if (!value) {
    return "—";
  }
  if (value >= 1000) {
    return `${(value / 1000).toFixed(2)}s`;
  }
  return `${value}ms`;
}

export function formatRate(value: number): string {
  if (!Number.isFinite(value)) {
    return "—";
  }
  if (value >= 100) {
    return `${Math.round(value)}`;
  }
  return value.toFixed(1);
}

export function truncatePrompt(prompt: string, maxLength = 160): string {
  if (!prompt) {
    return "—";
  }
  if (prompt.length <= maxLength) {
    return prompt;
  }
  return `${prompt.slice(0, maxLength)}...`;
}

export function formatSessionLabel(session: LogSession): string {
  const stopped = session.stopped_at ? session.stopped_at : "running";
  const runningTag = session.stopped_at ? "" : " (current)";
  return `${session.started_at} → ${stopped}${runningTag}`;
}
