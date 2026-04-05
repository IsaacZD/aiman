import type { Host, EngineInstance, HardwareInfo } from "../types";

export interface EventCallbacks {
  onEngineStatus: (hostId: string, instance: EngineInstance) => void;
  onHardware: (hostId: string, hardware: HardwareInfo) => void;
}

// One EventSource per host; handles automatic reconnection on disconnect.
export function useEvents() {
  const sources = new Map<string, EventSource>();

  function connect(hosts: Host[], callbacks: EventCallbacks) {
    // Close any existing connections before opening new ones.
    disconnect();

    for (const host of hosts) {
      const source = new EventSource(`/api/hosts/${host.id}/events`);

      source.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data as string) as {
            type: string;
            instance?: EngineInstance;
            hardware?: HardwareInfo;
          };
          if (data.type === "engine_status" && data.instance) {
            callbacks.onEngineStatus(host.id, data.instance);
          } else if (data.type === "hardware" && data.hardware) {
            callbacks.onHardware(host.id, data.hardware);
          }
        } catch {
          // Ignore malformed events.
        }
      };

      source.onerror = () => {
        // EventSource reconnects automatically; no action needed here.
      };

      sources.set(host.id, source);
    }
  }

  function disconnect() {
    for (const source of sources.values()) {
      source.close();
    }
    sources.clear();
  }

  return { connect, disconnect };
}
