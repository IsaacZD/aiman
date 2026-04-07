import type { Host, EngineInstance, HardwareInfo, ContainerImage } from "../types";

export interface EventCallbacks {
  onEngineStatus: (hostId: string, instance: EngineInstance) => void;
  onHardware: (hostId: string, hardware: HardwareInfo) => void;
  onImageStatus?: (hostId: string, image: ContainerImage) => void;
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

    source.onmessage = (event) => {
      // Reset backoff on successful message.
      backoffMs.set(host.id, INITIAL_BACKOFF);
      try {
        const data = JSON.parse(event.data as string) as {
          type: string;
          instance?: EngineInstance;
          hardware?: HardwareInfo;
          image?: ContainerImage;
        };
        if (data.type === "engine_status" && data.instance) {
          callbacks.onEngineStatus(host.id, data.instance);
        } else if (data.type === "hardware" && data.hardware) {
          callbacks.onHardware(host.id, data.hardware);
        } else if (data.type === "image_status" && data.image && callbacks.onImageStatus) {
          callbacks.onImageStatus(host.id, data.image);
        }
      } catch (err) {
        console.error(`[SSE] Failed to parse event from host ${host.id}:`, err, event.data);
      }
    };

    source.onerror = () => {
      // Close to prevent browser's automatic rapid reconnection.
      source.close();
      sources.delete(host.id);

      const delay = backoffMs.get(host.id) ?? INITIAL_BACKOFF;
      backoffMs.set(host.id, Math.min(delay * 2, MAX_BACKOFF));

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
