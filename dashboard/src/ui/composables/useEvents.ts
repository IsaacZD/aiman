import type { Host, EngineInstance, HardwareInfo } from "../types";

export interface EventCallbacks {
  onEngineStatus: (hostId: string, instance: EngineInstance) => void;
  onHardware: (hostId: string, hardware: HardwareInfo) => void;
}

const INITIAL_BACKOFF = 1000;
const MAX_BACKOFF = 30000;

// One EventSource per host; reconnects with exponential backoff on disconnect.
export function useEvents() {
  const sources = new Map<string, EventSource>();
  const backoffMs = new Map<string, number>();
  const reconnectTimers = new Map<string, number>();

  function connectHost(host: Host, callbacks: EventCallbacks) {
    const source = new EventSource(`/api/hosts/${host.id}/events`);

    source.onopen = () => {
      console.log(`[SSE] Connected to host ${host.id}`);
    };

    source.onmessage = (event) => {
      console.log(`[SSE] Received event from host ${host.id}:`, event.data);
      // Reset backoff on successful message.
      backoffMs.set(host.id, INITIAL_BACKOFF);
      try {
        const data = JSON.parse(event.data as string) as {
          type: string;
          instance?: EngineInstance;
          hardware?: HardwareInfo;
        };
        console.log(`[SSE] Parsed event type: ${data.type}`);
        if (data.type === "engine_status" && data.instance) {
          console.log(`[SSE] Engine status update:`, data.instance);
          callbacks.onEngineStatus(host.id, data.instance);
        } else if (data.type === "hardware" && data.hardware) {
          console.log(`[SSE] Hardware update:`, data.hardware);
          callbacks.onHardware(host.id, data.hardware);
        }
      } catch (err) {
        console.error(`[SSE] Failed to parse event from host ${host.id}:`, err, event.data);
      }
    };

    source.onerror = (err) => {
      console.error(`[SSE] Connection error for host ${host.id}:`, err);
      // Close to prevent browser's automatic rapid reconnection.
      source.close();
      sources.delete(host.id);

      const delay = backoffMs.get(host.id) ?? INITIAL_BACKOFF;
      backoffMs.set(host.id, Math.min(delay * 2, MAX_BACKOFF));
      console.log(`[SSE] Reconnecting to host ${host.id} in ${delay}ms`);

      const timer = window.setTimeout(() => {
        reconnectTimers.delete(host.id);
        connectHost(host, callbacks);
      }, delay);
      reconnectTimers.set(host.id, timer);
    };

    sources.set(host.id, source);
  }

  function connect(hosts: Host[], callbacks: EventCallbacks) {
    // Close any existing connections before opening new ones.
    disconnect();

    for (const host of hosts) {
      backoffMs.set(host.id, INITIAL_BACKOFF);
      connectHost(host, callbacks);
    }
  }

  function disconnect() {
    for (const source of sources.values()) {
      source.close();
    }
    sources.clear();
    for (const timer of reconnectTimers.values()) {
      window.clearTimeout(timer);
    }
    reconnectTimers.clear();
    backoffMs.clear();
  }

  return { connect, disconnect };
}
