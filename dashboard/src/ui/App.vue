<template>
  <main class="page">
    <header class="hero">
      <div>
        <p class="eyebrow">aiman dashboard</p>
        <h1>LLM engines, centralized.</h1>
        <p class="subtext">
          Start, stop, and monitor engines across your LAN. Each card maps to a single engine
          config on a host.
        </p>
      </div>
      <div class="status-card">
        <p class="status-label">Connected hosts</p>
        <p class="status-value">{{ hosts.length }}</p>
        <p class="status-hint">Last refresh {{ lastRefreshed ?? "—" }}</p>
      </div>
    </header>

    <nav class="primary-tabs">
      <button class="tab" :class="{ active: mainTab === 'engines' }" @click="mainTab = 'engines'">
        Engines
      </button>
      <button class="tab" :class="{ active: mainTab === 'admin' }" @click="mainTab = 'admin'">
        Admin
      </button>
    </nav>

    <section v-if="mainTab === 'engines'" class="panel">
      <div class="panel-head">
        <div>
          <h2>Engines</h2>
          <p class="panel-sub">{{ engineCount }} configured engine(s)</p>
        </div>
        <div class="panel-actions">
          <button class="primary" @click="refreshAll" :disabled="loading">
            {{ loading ? "Refreshing..." : "Refresh" }}
          </button>
        </div>
      </div>

      <div v-if="errors.length" class="alert">
        <p v-for="error in errors" :key="error">{{ error }}</p>
      </div>

      <div class="host-sections">
        <section v-for="host in hosts" :key="host.id" class="host-section">
          <div class="host-header">
            <div>
              <h3>{{ host.name }}</h3>
              <p class="host-meta">{{ host.id }} • {{ host.base_url }}</p>
            </div>
            <div class="hardware-card">
              <p class="hardware-title">Hardware</p>
              <template v-if="hardwareByHost[host.id]">
                <div class="hardware-grid">
                  <div class="hardware-item">
                    <span class="hardware-label">CPU</span>
                    <span class="hardware-value">
                      {{ formatCpu(hardwareByHost[host.id]) }}
                    </span>
                  </div>
                  <div class="hardware-item">
                    <span class="hardware-label">Memory</span>
                    <span class="hardware-value">
                      {{ formatMemory(hardwareByHost[host.id]) }}
                    </span>
                  </div>
                  <div class="hardware-item">
                    <span class="hardware-label">OS</span>
                    <span class="hardware-value">
                      {{ formatOs(hardwareByHost[host.id]) }}
                    </span>
                  </div>
                  <div class="hardware-item">
                    <span class="hardware-label">GPU</span>
                    <span class="hardware-value">
                      {{ formatGpus(hardwareByHost[host.id]) }}
                    </span>
                  </div>
                  <div class="hardware-item">
                    <span class="hardware-label">Uptime</span>
                    <span class="hardware-value">
                      {{ formatUptime(hardwareByHost[host.id]) }}
                    </span>
                  </div>
                </div>
              </template>
              <p v-else class="hardware-empty">No hardware data.</p>
            </div>
          </div>
          <div v-if="hardwareErrorsByHost[host.id]" class="alert">
            {{ hardwareErrorsByHost[host.id] }}
          </div>
          <div v-if="engineResultsByHost[host.id]?.error" class="alert">
            {{ host.name }}: {{ engineResultsByHost[host.id].error }}
          </div>
          <div class="grid">
            <article
              v-for="engine in enginesByHost[host.id] ?? []"
              :key="engine.instance.id"
              class="engine"
              :class="statusClass(engine.instance.status)"
              @click="openDetailModal(engine)"
            >
              <span
                class="status-dot"
                :class="statusDotClass(engine.instance.status)"
              ></span>
              <header>
                <p class="engine-host">{{ engine.host.name }}</p>
                <h3>{{ engine.configName ?? engine.instance.id }}</h3>
                <p class="engine-type">{{ engine.instance.config_id }}</p>
              </header>
              <div class="engine-meta">
                <span class="pill">PID {{ engine.instance.pid ?? "—" }}</span>
              </div>
              <div class="engine-actions">
                <button class="secondary" @click.stop="startEngine(engine)">Start</button>
                <button class="ghost" @click.stop="stopEngine(engine)">Stop</button>
              </div>
            </article>
          </div>
          <p v-if="!(enginesByHost[host.id]?.length)" class="empty">
            No engines configured for this host.
          </p>
        </section>
        <p v-if="!hosts.length" class="empty">No hosts yet.</p>
      </div>
    </section>

    <section v-if="mainTab === 'admin'" class="panel admin-panel">
      <div class="panel-head">
        <div>
          <h2>Admin management</h2>
          <p class="panel-sub">Manage hosts and engine configs in one place.</p>
        </div>
      </div>

      <div class="admin-grid">
        <div class="admin-pane">
          <div class="pane-head">
            <div class="pane-title">
              <h3>Hosts</h3>
              <button class="secondary" @click="openHostModal">New host</button>
            </div>
            <p class="panel-sub">Pick a host to manage configs.</p>
          </div>
          <div v-if="hostErrors.length" class="alert">
            <p v-for="error in hostErrors" :key="error">{{ error }}</p>
          </div>
          <div class="host-list">
            <article
              v-for="host in hosts"
              :key="host.id"
              class="host-card clickable"
              :class="{ active: host.id === configHostId }"
              @click="selectHost(host)"
            >
              <div>
                <h3>{{ host.name }}</h3>
                <p class="host-meta">{{ host.id }} • {{ host.base_url }}</p>
              </div>
              <div class="host-actions">
                <button class="secondary" @click.stop="openHostModal(host)">Edit</button>
              </div>
            </article>
            <p v-if="!hosts.length" class="empty">No hosts yet.</p>
          </div>
        </div>

        <div v-if="selectedHost" class="admin-pane">
          <div class="pane-head">
            <div class="pane-title">
              <h3>Configs</h3>
              <button class="secondary" @click="openConfigModal()">New config</button>
            </div>
            <p class="panel-sub">
              {{ selectedHost ? `${selectedHost.name} • ${selectedHost.id}` : "Select a host." }}
            </p>
          </div>
          <div v-if="configErrors.length" class="alert">
            <p v-for="error in configErrors" :key="error">{{ error }}</p>
          </div>
          <div class="config-list">
            <article v-for="config in configs" :key="config.id" class="config-card">
              <div>
                <h3>{{ config.name }}</h3>
                <p class="config-meta">{{ config.engine_type }}</p>
                <p class="config-meta config-id">{{ config.id }}</p>
              </div>
              <div class="config-actions">
                <button class="secondary" @click="openConfigModal(config)">Edit</button>
              </div>
            </article>
            <p v-if="!configs.length" class="empty">No configs yet.</p>
          </div>
        </div>
      </div>
    </section>

    <div v-if="showDetailModal" class="modal-backdrop">
      <div class="modal modal-wide">
        <div class="modal-head">
          <h3>Engine detail</h3>
          <button class="ghost" @click="closeDetailModal">Close</button>
        </div>
        <p class="panel-sub">
          {{ selected ? `${selected.host.name} • ${selected.instance.id}` : "Pick an engine" }}
        </p>
        <div class="logs">
          <p v-if="!selected" class="empty">Select an engine to stream logs.</p>
          <div v-else class="log-lines">
            <p v-for="(line, idx) in logs" :key="idx">{{ line }}</p>
          </div>
        </div>
        <div class="history">
          <div class="history-controls">
            <label>
              Show last
              <select v-model.number="historyMinutes" @change="loadHistory">
                <option :value="30">30 minutes</option>
                <option :value="120">2 hours</option>
                <option :value="360">6 hours</option>
                <option :value="1440">24 hours</option>
              </select>
            </label>
            <button class="secondary" @click="loadHistory" :disabled="!selected">
              Load history
            </button>
          </div>
          <div class="history-grid">
            <div class="history-card">
              <h3>Status history</h3>
              <div class="history-list">
                <p v-if="!statusHistory.length" class="empty">No status entries.</p>
                <p v-for="(item, idx) in statusHistory" :key="idx">
                  [{{ item.ts ?? "—" }}] {{ item.status }} (PID {{ item.pid ?? "—" }})
                </p>
              </div>
            </div>
            <div class="history-card">
              <h3>Log history</h3>
              <div class="history-list">
                <p v-if="!logHistory.length" class="empty">No log entries.</p>
                <p v-for="(item, idx) in logHistory" :key="idx">
                  [{{ item.ts }}] {{ item.stream }}: {{ item.line }}
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="showHostModal" class="modal-backdrop">
      <div class="modal">
        <div class="modal-head">
          <h3>{{ hostMode === "create" ? "Create host" : "Edit host" }}</h3>
          <button class="ghost" @click="closeHostModal">Close</button>
        </div>
        <form class="host-form" @submit.prevent="saveHost">
          <label>
            Host ID
            <input v-model="hostForm.id" type="text" placeholder="llm-01" />
          </label>
          <label>
            Name
            <input v-model="hostForm.name" type="text" placeholder="LLM Server 01" />
          </label>
          <label>
            Base URL
            <input v-model="hostForm.base_url" type="text" placeholder="http://10.0.0.12:4010" />
          </label>
          <label>
            API key (optional)
            <input v-model="hostForm.api_key" type="password" placeholder="dev-secret" />
          </label>
          <label>
            Model libraries (one path per line)
            <textarea
              v-model="hostForm.model_libraries_text"
              rows="3"
              placeholder="/data/huggingface\n/mnt/models/hf"
            ></textarea>
          </label>
          <div class="form-actions">
            <button
              v-if="hostMode === 'edit'"
              class="ghost"
              type="button"
              @click="deleteHostFromModal"
            >
              Delete
            </button>
            <button class="primary" type="submit">
              {{ hostMode === "create" ? "Create host" : "Save changes" }}
            </button>
          </div>
        </form>
      </div>
    </div>

    <div v-if="showConfigModal" class="modal-backdrop">
      <div class="modal modal-wide">
        <div class="modal-head">
          <h3>{{ configMode === "create" ? "Create config" : "Edit config" }}</h3>
          <button class="ghost" @click="closeConfigModal">Close</button>
        </div>
        <div v-if="configErrors.length" class="alert">
          <p v-for="error in configErrors" :key="error">{{ error }}</p>
        </div>
        <form class="config-form" @submit.prevent="saveConfig">
          <label>
            Config ID
            <input
              v-model="configForm.id"
              type="text"
              placeholder="auto-generated"
              readonly
            />
          </label>
          <label>
            Display name
            <input v-model="configForm.name" type="text" placeholder="DeepSeek via vLLM" />
          </label>
          <label>
            Engine type
            <select v-model="configForm.engine_type">
              <option value="Vllm">Vllm</option>
              <option value="LlamaCpp">LlamaCpp</option>
              <option value="KTransformers">KTransformers</option>
              <option value="Custom">Custom</option>
            </select>
          </label>
          <label>
            Command
            <input v-model="configForm.command" type="text" placeholder="/opt/vllm/serve" />
          </label>
          <div class="engine-fields-panel">
            <VllmConfigFields
              v-if="configForm.engine_type === 'Vllm'"
              v-model="vllmArgsForm"
              :model-options="vllmModelOptions"
            />
            <LlamaCppConfigFields
              v-else-if="configForm.engine_type === 'LlamaCpp'"
              v-model="llamaCppArgsForm"
              :model-options="ggufModelOptions"
            />
            <KTransformersConfigFields
              v-else-if="configForm.engine_type === 'KTransformers'"
              v-model="kTransformersArgsForm"
              :model-options="ggufModelOptions"
            />
            <CustomConfigFields v-else v-model="customArgsForm" />
          </div>
          <label>
            Env (KEY=VALUE per line)
            <textarea v-model="configForm.envText" rows="4" placeholder="HF_HOME=/data/hf"></textarea>
          </label>
          <label>
            Working dir
            <input v-model="configForm.working_dir" type="text" placeholder="/opt/engines" />
          </label>
          <div class="config-inline">
            <label>
              <input v-model="configForm.auto_restart_enabled" type="checkbox" />
              Auto restart
            </label>
            <label>
              Max retries
              <input v-model.number="configForm.auto_restart_max_retries" type="number" min="0" />
            </label>
            <label>
              Backoff (sec)
              <input v-model.number="configForm.auto_restart_backoff_secs" type="number" min="1" />
            </label>
          </div>
          <div class="form-actions">
            <button
              v-if="configMode === 'edit'"
              class="ghost"
              type="button"
              @click="deleteConfigFromModal"
            >
              Delete
            </button>
            <button class="primary" type="submit">
              {{ configMode === "create" ? "Create config" : "Save changes" }}
            </button>
          </div>
        </form>
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import VllmConfigFields from "./components/VllmConfigFields.vue";
import LlamaCppConfigFields from "./components/LlamaCppConfigFields.vue";
import KTransformersConfigFields from "./components/KTransformersConfigFields.vue";
import CustomConfigFields from "./components/CustomConfigFields.vue";
import {
  buildVllmArgs,
  createVllmArgsForm,
  parseVllmArgs
} from "./engine-args/vllm";
import {
  buildLlamaCppArgs,
  createLlamaCppArgsForm,
  parseLlamaCppArgs
} from "./engine-args/llamaCpp";
import {
  buildKTransformersArgs,
  createKTransformersArgsForm,
  parseKTransformersArgs
} from "./engine-args/kTransformers";
import {
  buildCustomArgs,
  createCustomArgsForm,
  parseCustomArgs
} from "./engine-args/custom";

type Host = {
  id: string;
  name: string;
  base_url: string;
  api_key?: string;
  model_libraries?: string[];
};

type EnvVar = {
  key: string;
  value: string;
};

type EngineConfig = {
  id: string;
  name: string;
  engine_type: "Vllm" | "LlamaCpp" | "KTransformers" | "Custom";
  command: string;
  args: string[];
  env: EnvVar[];
  working_dir?: string | null;
  auto_restart: {
    enabled: boolean;
    max_retries: number;
    backoff_secs: number;
  };
};

type EngineInstance = {
  id: string;
  config_id: string;
  status: string;
  pid?: number | null;
  ts?: string;
};

type EngineItem = {
  host: Host;
  instance: EngineInstance;
  configName?: string;
};

type EnginesResult = {
  host: Host;
  engines?: EngineInstance[];
  error?: string;
};

type LogEntry = {
  ts: string;
  stream: string;
  line: string;
};

type ModelArtifact = {
  id: string;
  kind: "snapshot" | "gguf" | string;
  path: string;
  label: string;
  library: string;
};

type HardwareInfo = {
  hostname?: string | null;
  os_name?: string | null;
  os_version?: string | null;
  kernel_version?: string | null;
  cpu_brand?: string | null;
  cpu_cores_logical?: number | null;
  cpu_cores_physical?: number | null;
  cpu_frequency_mhz?: number | null;
  memory_total_kb?: number | null;
  memory_available_kb?: number | null;
  swap_total_kb?: number | null;
  swap_free_kb?: number | null;
  uptime_seconds?: number | null;
  gpus?: {
    name?: string | null;
    vendor?: string | null;
    memory_total_mb?: number | null;
    driver_version?: string | null;
  }[];
};

// High-level UI state.
const hosts = ref<Host[]>([]);
const engines = ref<EngineItem[]>([]);
const selected = ref<EngineItem | null>(null);
const logs = ref<string[]>([]);
const errors = ref<string[]>([]);
const loading = ref(false);
const lastRefreshed = ref<string | null>(null);
const mainTab = ref<"engines" | "admin">("engines");
const historyMinutes = ref(120);
const statusHistory = ref<EngineInstance[]>([]);
const logHistory = ref<LogEntry[]>([]);
const hardwareByHost = ref<Record<string, HardwareInfo | null>>({});
const hardwareErrorsByHost = ref<Record<string, string>>({});
const engineResultsByHost = ref<Record<string, EnginesResult>>({});
const configNameByHost = ref<Record<string, Record<string, string>>>({});
const showDetailModal = ref(false);
const configHostId = ref<string | null>(null);
const configs = ref<EngineConfig[]>([]);
const configErrors = ref<string[]>([]);
const configMode = ref<"create" | "edit">("create");
const configForm = ref(createEmptyConfigForm());
const configOriginalId = ref<string | null>(null);
const vllmArgsForm = ref(createVllmArgsForm());
const llamaCppArgsForm = ref(createLlamaCppArgsForm());
const kTransformersArgsForm = ref(createKTransformersArgsForm());
const customArgsForm = ref(createCustomArgsForm());
const showConfigModal = ref(false);
const hostErrors = ref<string[]>([]);
const hostMode = ref<"create" | "edit">("create");
const hostForm = ref(createEmptyHostForm());
const showHostModal = ref(false);
const modelArtifacts = ref<ModelArtifact[]>([]);

const selectedHost = computed(() =>
  hosts.value.find((host) => host.id === configHostId.value) ?? null
);

const vllmModelOptions = computed(() =>
  modelArtifacts.value.filter((artifact) => artifact.kind === "snapshot")
);
const ggufModelOptions = computed(() =>
  modelArtifacts.value.filter((artifact) => artifact.kind === "gguf")
);
const engineCount = computed(() => engines.value.length);
const enginesByHost = computed(() => {
  const grouped: Record<string, EngineItem[]> = {};
  for (const host of hosts.value) {
    grouped[host.id] = [];
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
  return grouped;
});

const defaultCommands: Record<EngineConfig["engine_type"], string> = {
  Vllm: "python",
  LlamaCpp: "llama-server",
  KTransformers: "ktransformers-server",
  Custom: ""
};

const lastEngineType = ref<EngineConfig["engine_type"]>(configForm.value.engine_type);
watch(
  () => configForm.value.engine_type,
  (next) => {
    const previous = lastEngineType.value;
    const previousDefault = defaultCommands[previous];
    const nextDefault = defaultCommands[next];
    if (!configForm.value.command.trim() || configForm.value.command === previousDefault) {
      configForm.value.command = nextDefault;
    }
    lastEngineType.value = next;
  }
);

let ws: WebSocket | null = null;

// Load hosts + engines from the dashboard API.
async function refreshAll() {
  loading.value = true;
  errors.value = [];
  try {
    const hostsRes = await fetch("/api/hosts");
    if (!hostsRes.ok) {
      errors.value = [`Failed to load hosts (HTTP ${hostsRes.status})`];
      hosts.value = [];
      engines.value = [];
      engineResultsByHost.value = {};
      return;
    }
    const hostsBody = (await hostsRes.json()) as { hosts: Host[] };
    hosts.value = hostsBody.hosts ?? [];
    if (hosts.value.length && hostMode.value === "create" && !hostForm.value.id) {
      hostForm.value = createEmptyHostForm();
    }

    const nextConfigNameByHost: Record<string, Record<string, string>> = {};
    const nextHardwareByHost: Record<string, HardwareInfo | null> = {};
    const nextHardwareErrors: Record<string, string> = {};
    await Promise.all(
      hosts.value.map(async (host) => {
        try {
          const res = await fetch(`/api/hosts/${host.id}/configs`);
          if (!res.ok) {
            nextConfigNameByHost[host.id] = {};
            return;
          }
          const body = (await res.json()) as { configs: EngineConfig[] };
          const map: Record<string, string> = {};
          for (const config of body.configs ?? []) {
            map[config.id] = config.name;
          }
          nextConfigNameByHost[host.id] = map;
        } catch {
          // Ignore config load failures here; engines list can still render.
          nextConfigNameByHost[host.id] = {};
        }

        try {
          const res = await fetch(`/api/hosts/${host.id}/hardware`);
          if (!res.ok) {
            nextHardwareErrors[host.id] = `Hardware unavailable (HTTP ${res.status}).`;
            nextHardwareByHost[host.id] = null;
            return;
          }
          const body = (await res.json()) as { hardware?: HardwareInfo };
          nextHardwareByHost[host.id] = body.hardware ?? null;
        } catch (err) {
          nextHardwareErrors[host.id] = (err as Error).message;
          nextHardwareByHost[host.id] = null;
        }
      })
    );
    configNameByHost.value = nextConfigNameByHost;
    hardwareByHost.value = nextHardwareByHost;
    hardwareErrorsByHost.value = nextHardwareErrors;

    const enginesRes = await fetch("/api/engines");
    if (!enginesRes.ok) {
      errors.value = [`Failed to load engines (HTTP ${enginesRes.status})`];
      engines.value = [];
      engineResultsByHost.value = {};
      return;
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
        const configName = configNameByHost.value[result.host.id]?.[instance.config_id];
        nextEngines.push({ host: result.host, instance, configName });
      }
    }
    engines.value = nextEngines;
    engineResultsByHost.value = nextEngineResultsByHost;
    lastRefreshed.value = new Date().toLocaleTimeString();
    if (
      configHostId.value &&
      !hosts.value.some((host) => host.id === configHostId.value)
    ) {
      configHostId.value = null;
    }
    if (!configHostId.value && hosts.value.length) {
      configHostId.value = hosts.value[0].id;
    }
    await loadConfigs();
  } finally {
    loading.value = false;
  }
}

// Proxy control commands to the dashboard server.
async function startEngine(engine: EngineItem) {
  await fetch(`/api/hosts/${engine.host.id}/engines/${engine.instance.id}/start`, {
    method: "POST"
  });
  await refreshAll();
}

async function stopEngine(engine: EngineItem) {
  await fetch(`/api/hosts/${engine.host.id}/engines/${engine.instance.id}/stop`, {
    method: "POST"
  });
  await refreshAll();
}

// Selecting an engine opens detail modal.
function openDetailModal(engine: EngineItem) {
  selected.value = engine;
  showDetailModal.value = true;
  connectLogs();
  loadHistory();
}

function closeDetailModal() {
  showDetailModal.value = false;
  logs.value = [];
  statusHistory.value = [];
  logHistory.value = [];
  if (ws) {
    ws.close();
    ws = null;
  }
}

// Open a WS stream to receive live logs.
function connectLogs() {
  if (!selected.value) {
    return;
  }

  if (ws) {
    ws.close();
  }

  logs.value = [];
  const { host, instance } = selected.value;
  ws = new WebSocket(`/api/hosts/${host.id}/engines/${instance.id}/logs/ws`);

  ws.onmessage = (event) => {
    try {
      const entry = JSON.parse(event.data);
      logs.value.push(`[${entry.ts}] ${entry.stream}: ${entry.line}`);
      if (logs.value.length > 500) {
        logs.value.shift();
      }
    } catch {
      logs.value.push(event.data);
    }
  };
}

// Pull status/log history for the selected engine.
async function loadHistory() {
  if (!selected.value) {
    return;
  }

  const since = new Date(Date.now() - historyMinutes.value * 60 * 1000).toISOString();
  const { host, instance } = selected.value;
  const [statusRes, logsRes] = await Promise.all([
    fetch(
      `/api/hosts/${host.id}/engines/${instance.id}/status?since=${encodeURIComponent(since)}&limit=300`
    ),
    fetch(
      `/api/hosts/${host.id}/engines/${instance.id}/logs?since=${encodeURIComponent(since)}&limit=500`
    )
  ]);

  if (statusRes.ok) {
    const body = (await statusRes.json()) as { entries: EngineInstance[] };
    statusHistory.value = body.entries ?? [];
  }

  if (logsRes.ok) {
    const body = (await logsRes.json()) as { entries: LogEntry[] };
    logHistory.value = body.entries ?? [];
  }
}

function clearLogs() {
  logs.value = [];
}

function statusClass(status: string) {
  return `status-${status.toLowerCase()}`;
}

function statusDotClass(status: string) {
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

function formatCpu(info: HardwareInfo | null | undefined) {
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

function formatMemory(info: HardwareInfo | null | undefined) {
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

function formatOs(info: HardwareInfo | null | undefined) {
  if (!info) {
    return "—";
  }
  const parts = [info.os_name, info.os_version].filter(Boolean);
  const os = parts.length ? parts.join(" ") : null;
  const kernel = info.kernel_version ? `kernel ${info.kernel_version}` : null;
  return [os, kernel].filter(Boolean).join(" • ") || "—";
}

function formatGpus(info: HardwareInfo | null | undefined) {
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

function formatUptime(info: HardwareInfo | null | undefined) {
  if (!info) {
    return "—";
  }
  return formatDuration(info.uptime_seconds);
}

function formatDuration(totalSeconds: number | null | undefined) {
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

function formatBytesFromKb(value: number | null | undefined) {
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

function formatBytesFromMb(value: number) {
  const gib = value / 1024;
  if (gib >= 100) {
    return `${Math.round(gib)} GB`;
  }
  if (gib >= 10) {
    return `${gib.toFixed(1)} GB`;
  }
  return `${gib.toFixed(2)} GB`;
}

function createEmptyHostForm() {
  return {
    id: "",
    name: "",
    base_url: "",
    api_key: "",
    model_libraries_text: ""
  };
}

function createEmptyConfigForm() {
  return {
    id: "",
    name: "",
    engine_type: "Vllm" as EngineConfig["engine_type"],
    command: "",
    envText: "",
    working_dir: "",
    auto_restart_enabled: false,
    auto_restart_max_retries: 0,
    auto_restart_backoff_secs: 5
  };
}

function generateConfigId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `cfg-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

async function loadConfigs() {
  configErrors.value = [];
  if (!configHostId.value) {
    configs.value = [];
    return;
  }

  try {
    const res = await fetch(`/api/hosts/${configHostId.value}/configs`);
    if (!res.ok) {
      configErrors.value = [`Failed to load configs (HTTP ${res.status})`];
      configs.value = [];
      return;
    }
    const body = (await res.json()) as { configs: EngineConfig[] };
    configs.value = body.configs ?? [];
  } catch (err) {
    configErrors.value = [(err as Error).message];
  }
}

async function loadModels() {
  if (!configHostId.value) {
    modelArtifacts.value = [];
    return;
  }
  try {
    const res = await fetch(`/api/hosts/${configHostId.value}/models`);
    if (!res.ok) {
      modelArtifacts.value = [];
      return;
    }
    const body = (await res.json()) as { artifacts: ModelArtifact[] };
    modelArtifacts.value = body.artifacts ?? [];
  } catch {
    modelArtifacts.value = [];
  }
}

function resetHostForm() {
  hostMode.value = "create";
  hostForm.value = createEmptyHostForm();
}

function openHostModal(host?: Host) {
  if (host) {
    editHost(host);
  } else {
    resetHostForm();
  }
  showHostModal.value = true;
}

function closeHostModal() {
  showHostModal.value = false;
  hostErrors.value = [];
}

function editHost(host: Host) {
  hostMode.value = "edit";
  hostForm.value = {
    id: host.id,
    name: host.name,
    base_url: host.base_url,
    api_key: host.api_key ?? "",
    model_libraries_text: (host.model_libraries ?? []).join("\n")
  };
}

async function saveHost() {
  hostErrors.value = [];
  if (!hostForm.value.id.trim()) {
    hostErrors.value = ["Host ID is required."];
    return;
  }
  if (!hostForm.value.name.trim()) {
    hostErrors.value = ["Host name is required."];
    return;
  }
  if (!hostForm.value.base_url.trim()) {
    hostErrors.value = ["Base URL is required."];
    return;
  }

  const apiKey = hostForm.value.api_key.trim();
  const modelLibraries = hostForm.value.model_libraries_text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  const payload = {
    id: hostForm.value.id.trim(),
    name: hostForm.value.name.trim(),
    base_url: hostForm.value.base_url.trim(),
    ...(apiKey ? { api_key: apiKey } : {}),
    ...(modelLibraries.length ? { model_libraries: modelLibraries } : {})
  };

  const method = hostMode.value === "create" ? "POST" : "PUT";
  const url =
    hostMode.value === "create"
      ? "/api/hosts"
      : `/api/hosts/${encodeURIComponent(payload.id)}`;

  if (hostMode.value === "edit" && !confirm(`Save changes to host "${payload.name}"?`)) {
    return;
  }

  const res = await fetch(url, {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  });

  if (!res.ok) {
    const body = await res.json().catch(() => null);
    if (
      res.status === 409 &&
      hostMode.value === "create" &&
      payload.id &&
      confirm(`Host "${payload.id}" already exists. Edit it instead?`)
    ) {
      const existing = hosts.value.find((host) => host.id === payload.id);
      if (existing) {
        openHostModal(existing);
      }
      return;
    }
    hostErrors.value = [
      body?.error ? `Save failed: ${body.error}` : `Save failed (HTTP ${res.status}).`
    ];
    return;
  }

  closeHostModal();
  resetHostForm();
  await refreshAll();
  if (configHostId.value === payload.id) {
    await loadModels();
  }
}

async function deleteHost(host: Host) {
  if (!confirm(`Delete host "${host.name}"? This cannot be undone.`)) {
    return;
  }
  const res = await fetch(`/api/hosts/${encodeURIComponent(host.id)}`, { method: "DELETE" });
  if (!res.ok) {
    hostErrors.value = [`Delete failed (HTTP ${res.status}).`];
    return;
  }
  await refreshAll();
}

async function deleteHostFromModal() {
  if (!hostForm.value.id.trim()) {
    return;
  }
  await deleteHost({ id: hostForm.value.id, name: hostForm.value.name } as Host);
  closeHostModal();
}

function resetConfigForm() {
  configMode.value = "create";
  configForm.value = {
    ...createEmptyConfigForm(),
    id: generateConfigId()
  };
  configOriginalId.value = null;
  vllmArgsForm.value = createVllmArgsForm();
  llamaCppArgsForm.value = createLlamaCppArgsForm();
  kTransformersArgsForm.value = createKTransformersArgsForm();
  customArgsForm.value = createCustomArgsForm();
}

function openConfigModal(config?: EngineConfig) {
  if (!configHostId.value) {
    configErrors.value = ["Select a host before creating a config."];
    return;
  }
  if (config && typeof config === "object" && "id" in config) {
    editConfig(config);
  } else {
    resetConfigForm();
  }
  if (!configForm.value.command.trim()) {
    configForm.value.command = defaultCommands[configForm.value.engine_type];
  }
  loadModels();
  showConfigModal.value = true;
}

function closeConfigModal() {
  showConfigModal.value = false;
  configErrors.value = [];
}

function editConfig(config: EngineConfig) {
  configMode.value = "edit";
  configOriginalId.value = config.id;
  configForm.value = {
    id: config.id,
    name: config.name,
    engine_type: config.engine_type,
    command: config.command,
    envText: config.env.map((item) => `${item.key}=${item.value}`).join("\n"),
    working_dir: config.working_dir ?? "",
    auto_restart_enabled: config.auto_restart.enabled,
    auto_restart_max_retries: config.auto_restart.max_retries,
    auto_restart_backoff_secs: config.auto_restart.backoff_secs
  };
  if (config.engine_type === "Vllm") {
    vllmArgsForm.value = parseVllmArgs(config.args ?? []);
  } else if (config.engine_type === "LlamaCpp") {
    llamaCppArgsForm.value = parseLlamaCppArgs(config.args ?? []);
  } else if (config.engine_type === "KTransformers") {
    kTransformersArgsForm.value = parseKTransformersArgs(config.args ?? []);
  } else {
    customArgsForm.value = parseCustomArgs(config.args ?? []);
  }
}

async function saveConfig() {
  configErrors.value = [];
  if (!configHostId.value) {
    configErrors.value = ["Select a host before saving."];
    return;
  }

  const errors: string[] = [];
  if (!configForm.value.id.trim()) {
    errors.push("Config ID is required.");
  }
  if (!configForm.value.name.trim()) {
    errors.push("Display name is required.");
  }
  if (!configForm.value.command.trim()) {
    errors.push("Command is required.");
  }

  const envEntries = parseEnvLines(configForm.value.envText, errors);
  if (errors.length) {
    configErrors.value = errors;
    return;
  }

  let args: string[] = [];
  if (configForm.value.engine_type === "Vllm") {
    args = buildVllmArgs(vllmArgsForm.value);
  } else if (configForm.value.engine_type === "LlamaCpp") {
    args = buildLlamaCppArgs(llamaCppArgsForm.value);
  } else if (configForm.value.engine_type === "KTransformers") {
    args = buildKTransformersArgs(kTransformersArgsForm.value);
  } else {
    args = buildCustomArgs(customArgsForm.value);
  }

  const config: EngineConfig = {
    id: configForm.value.id.trim(),
    name: configForm.value.name.trim(),
    engine_type: configForm.value.engine_type,
    command: configForm.value.command.trim(),
    args,
    env: envEntries,
    working_dir: configForm.value.working_dir.trim()
      ? configForm.value.working_dir.trim()
      : null,
    auto_restart: {
      enabled: configForm.value.auto_restart_enabled,
      max_retries: Number(configForm.value.auto_restart_max_retries) || 0,
      backoff_secs: Number(configForm.value.auto_restart_backoff_secs) || 5
    }
  };

  const actionLabel = configMode.value === "create" ? "Create config" : "Save changes to config";
  if (!confirm(`${actionLabel} "${config.name}"?`)) {
    return;
  }

  const method = configMode.value === "create" ? "POST" : "PUT";
  const targetId = configMode.value === "edit" ? configOriginalId.value : null;
  if (configMode.value === "edit" && !targetId) {
    configErrors.value = ["Original Config ID is missing; reload the page and try again."];
    return;
  }
  const isRename =
    configMode.value === "edit" && targetId !== null && targetId !== config.id;

  const url =
    configMode.value === "create"
      ? `/api/hosts/${configHostId.value}/configs`
      : `/api/hosts/${configHostId.value}/configs/${encodeURIComponent(targetId!)}`;

  const parseError = async (res: Response) => {
    const rawBody = await res.text().catch(() => "");
    if (!rawBody) {
      return `Save failed (HTTP ${res.status}).`;
    }
    try {
      const parsed = JSON.parse(rawBody) as { error?: string; message?: string };
      return `Save failed: ${parsed.error ?? parsed.message ?? rawBody}`;
    } catch {
      return `Save failed: ${rawBody}`;
    }
  };

  if (isRename) {
    const createRes = await fetch(`/api/hosts/${configHostId.value}/configs`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(config)
    });

    if (!createRes.ok) {
      configErrors.value = [await parseError(createRes)];
      return;
    }

    const deleteRes = await fetch(
      `/api/hosts/${configHostId.value}/configs/${encodeURIComponent(targetId!)}`,
      { method: "DELETE" }
    );

    if (!deleteRes.ok) {
      const suffix = `Delete failed (HTTP ${deleteRes.status}).`;
      configErrors.value = [
        `Rename partially succeeded: new config created, but old config was not deleted. ${suffix}`
      ];
      await loadConfigs();
      await refreshAll();
      return;
    }

    closeConfigModal();
    resetConfigForm();
    await loadConfigs();
    await refreshAll();
    return;
  }

  const res = await fetch(url, {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(config)
  });

  if (!res.ok) {
    configErrors.value = [await parseError(res)];
    return;
  }

  closeConfigModal();
  resetConfigForm();
  await loadConfigs();
  await refreshAll();
}

async function deleteConfig(config: EngineConfig) {
  if (!configHostId.value) {
    return;
  }
  if (!confirm(`Delete config "${config.name}"? This cannot be undone.`)) {
    return;
  }
  const res = await fetch(
    `/api/hosts/${configHostId.value}/configs/${encodeURIComponent(config.id)}`,
    { method: "DELETE" }
  );
  if (!res.ok) {
    configErrors.value = [`Delete failed (HTTP ${res.status}).`];
    return;
  }
  await loadConfigs();
  await refreshAll();
}

async function deleteConfigFromModal() {
  if (!configForm.value.id.trim()) {
    return;
  }
  await deleteConfig({ id: configForm.value.id, name: configForm.value.name } as EngineConfig);
  closeConfigModal();
}

function selectHost(host: Host) {
  if (configHostId.value === host.id) {
    return;
  }
  configHostId.value = host.id;
  loadConfigs();
  loadModels();
}

function parseEnvLines(raw: string, errors: string[]) {
  const entries: EnvVar[] = [];
  const lines = raw
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  for (const line of lines) {
    const index = line.indexOf("=");
    if (index <= 0) {
      errors.push(`Env line "${line}" must be KEY=VALUE.`);
      continue;
    }
    entries.push({ key: line.slice(0, index), value: line.slice(index + 1) });
  }

  return entries;
}

// Initial load and cleanup.
onMounted(refreshAll);

onBeforeUnmount(() => {
  if (ws) {
    ws.close();
  }
});
</script>
