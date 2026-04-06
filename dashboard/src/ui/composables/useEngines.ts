import { ref, computed } from "vue";
import type { Host, EngineItem, EnginesResult, HardwareInfo, EngineConfig } from "../types";

export function useEngines() {
  const engines = ref<EngineItem[]>([]);
  const errors = ref<string[]>([]);
  const loading = ref(false);
  const lastRefreshed = ref<string | null>(null);
  const configNameByHost = ref<Record<string, Record<string, string>>>({});
  const engineResultsByHost = ref<Record<string, EnginesResult>>({});

  const engineCount = computed(() => engines.value.length);

  const enginesByHost = ref<Record<string, EngineItem[]>>({});

  function rebuildEnginesByHost() {
    const grouped: Record<string, EngineItem[]> = {};
    for (const host of Object.keys(engineResultsByHost.value)) {
      if (!grouped[host]) {
        grouped[host] = [];
      }
    }
    for (const engine of engines.value) {
      const hostId = engine.host.id;
      if (!grouped[hostId]) {
        grouped[hostId] = [];
      }
      grouped[hostId].push(engine);
    }
    for (const list of Object.values(grouped)) {
      list.sort((a, b) =>
        (a.configName ?? a.instance.id).localeCompare(b.configName ?? b.instance.id)
      );
    }
    enginesByHost.value = grouped;
  }

  function updateConfigNameMapForHost(hostId: string, hostConfigs: EngineConfig[]) {
    const map: Record<string, string> = {};
    for (const config of hostConfigs ?? []) {
      map[config.id] = config.name;
    }
    configNameByHost.value = { ...configNameByHost.value, [hostId]: map };
    engines.value = engines.value.map((engine) => {
      if (engine.host.id !== hostId) {
        return engine;
      }
      const configName = map[engine.instance.config_id];
      if (configName === engine.configName) {
        return engine;
      }
      return { ...engine, configName };
    });
    rebuildEnginesByHost();
  }

  async function startEngine(engine: EngineItem, onSuccess: () => Promise<void>) {
    await fetch(`/api/hosts/${engine.host.id}/engines/${engine.instance.id}/start`, {
      method: "POST"
    });
    await onSuccess();
  }

  async function stopEngine(engine: EngineItem, onSuccess: () => Promise<void>) {
    await fetch(`/api/hosts/${engine.host.id}/engines/${engine.instance.id}/stop`, {
      method: "POST"
    });
    await onSuccess();
  }

  async function refreshEngines(
    hosts: Host[],
    configNameByHostMap: Record<string, Record<string, string>>
  ) {
    const enginesRes = await fetch("/api/engines");
    if (!enginesRes.ok) {
      errors.value = [`Failed to load engines (HTTP ${enginesRes.status})`];
      engines.value = [];
      engineResultsByHost.value = {};
      return false;
    }
    const enginesBody = (await enginesRes.json()) as { results: EnginesResult[] };

    const nextEngines: EngineItem[] = [];
    const nextEngineResultsByHost: Record<string, EnginesResult> = {};
    for (const result of enginesBody.results ?? []) {
      nextEngineResultsByHost[result.host.id] = result;
      if (result.error) {
        continue;
      }
      for (const instance of result.engines ?? []) {
        const configName = configNameByHostMap[result.host.id]?.[instance.config_id];
        nextEngines.push({ host: result.host, instance, configName });
      }
    }
    engines.value = nextEngines;
    engineResultsByHost.value = nextEngineResultsByHost;
    lastRefreshed.value = new Date().toLocaleTimeString();
    rebuildEnginesByHost();
    return true;
  }

  return {
    engines,
    errors,
    loading,
    lastRefreshed,
    configNameByHost,
    engineResultsByHost,
    engineCount,
    enginesByHost,
    updateConfigNameMapForHost,
    startEngine,
    stopEngine,
    refreshEngines
  };
}
