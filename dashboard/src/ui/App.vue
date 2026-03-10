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

    <section class="panel">
      <div class="panel-head">
        <div>
          <h2>Engines</h2>
          <p class="panel-sub">{{ engines.length }} configured engine(s)</p>
        </div>
        <div class="panel-actions">
          <button class="secondary" @click="scrollToConfigs">Manage configs</button>
          <button class="primary" @click="refreshAll" :disabled="loading">
            {{ loading ? "Refreshing..." : "Refresh" }}
          </button>
        </div>
      </div>

      <div v-if="errors.length" class="alert">
        <p v-for="error in errors" :key="error">{{ error }}</p>
      </div>

      <div class="grid">
        <article
          v-for="engine in engines"
          :key="engine.instance.id"
          class="engine"
          :class="statusClass(engine.instance.status)"
          @click="selectEngine(engine)"
        >
          <header>
            <p class="engine-host">{{ engine.host.name }}</p>
            <h3>{{ engine.instance.id }}</h3>
            <p class="engine-type">{{ engine.instance.config_id }}</p>
          </header>
          <div class="engine-meta">
            <span class="pill">{{ engine.instance.status }}</span>
            <span class="pill">PID {{ engine.instance.pid ?? "—" }}</span>
          </div>
          <div class="engine-actions">
            <button class="secondary" @click.stop="startEngine(engine)">Start</button>
            <button class="ghost" @click.stop="stopEngine(engine)">Stop</button>
          </div>
        </article>
      </div>
    </section>

    <section class="panel" id="configs">
      <div class="panel-head">
        <div>
          <h2>Engine configs</h2>
          <p class="panel-sub">Create, edit, and remove configs per host.</p>
        </div>
        <div class="config-toolbar">
          <select v-model="configHostId" @change="loadConfigs">
            <option v-for="host in hosts" :key="host.id" :value="host.id">
              {{ host.name }}
            </option>
          </select>
          <button class="secondary" @click="resetConfigForm">New config</button>
        </div>
      </div>

      <div v-if="configErrors.length" class="alert">
        <p v-for="error in configErrors" :key="error">{{ error }}</p>
      </div>

      <div class="config-grid">
        <div class="config-list">
          <article v-for="config in configs" :key="config.id" class="config-card">
            <div>
              <h3>{{ config.name }}</h3>
              <p class="config-meta">{{ config.id }} • {{ config.engine_type }}</p>
            </div>
            <div class="config-actions">
              <button class="secondary" @click="editConfig(config)">Edit</button>
              <button class="ghost" @click="deleteConfig(config)">Delete</button>
            </div>
          </article>
          <p v-if="!configs.length" class="empty">No configs yet.</p>
        </div>

        <form class="config-form" @submit.prevent="saveConfig">
          <h3>{{ configMode === "create" ? "Create config" : "Edit config" }}</h3>
          <label>
            Config ID
            <input v-model="configForm.id" type="text" placeholder="deepseek-vllm" />
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
            </select>
          </label>
          <label>
            Command
            <input v-model="configForm.command" type="text" placeholder="/opt/vllm/serve" />
          </label>
          <label>
            Args (one per line)
            <textarea v-model="configForm.argsText" rows="4" placeholder="--model&#10;deepseek-ai/DeepSeek-R1"></textarea>
          </label>
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
          <div class="config-submit">
            <button class="primary" type="submit">
              {{ configMode === "create" ? "Create" : "Save changes" }}
            </button>
          </div>
        </form>
      </div>
    </section>

    <section class="panel">
      <div class="panel-head">
        <div>
          <h2>Engine detail</h2>
          <p class="panel-sub">
            {{ selected ? `${selected.host.name} • ${selected.instance.id}` : "Pick an engine" }}
          </p>
        </div>
        <div class="tabs">
          <button
            class="tab"
            :class="{ active: detailTab === 'live' }"
            @click="detailTab = 'live'"
          >
            Live logs
          </button>
          <button
            class="tab"
            :class="{ active: detailTab === 'history' }"
            @click="detailTab = 'history'"
          >
            History
          </button>
        </div>
      </div>

      <div v-if="detailTab === 'live'" class="logs">
        <p v-if="!selected" class="empty">Select an engine to stream logs.</p>
        <div v-else class="log-lines">
          <p v-for="(line, idx) in logs" :key="idx">{{ line }}</p>
        </div>
      </div>

      <div v-else class="history">
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
                [{{ item.ts }}] {{ item.status }} (PID {{ item.pid ?? "—" }})
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
    </section>
  </main>
</template>

<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from "vue";

type Host = {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
};

type EnvVar = {
  key: string;
  value: string;
};

type EngineConfig = {
  id: string;
  name: string;
  engine_type: "Vllm" | "LlamaCpp" | "KTransformers";
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

// High-level UI state.
const hosts = ref<Host[]>([]);
const engines = ref<EngineItem[]>([]);
const selected = ref<EngineItem | null>(null);
const logs = ref<string[]>([]);
const errors = ref<string[]>([]);
const loading = ref(false);
const lastRefreshed = ref<string | null>(null);
const detailTab = ref<"live" | "history">("live");
const historyMinutes = ref(120);
const statusHistory = ref<EngineInstance[]>([]);
const logHistory = ref<LogEntry[]>([]);
const configHostId = ref<string | null>(null);
const configs = ref<EngineConfig[]>([]);
const configErrors = ref<string[]>([]);
const configMode = ref<"create" | "edit">("create");
const configForm = ref(createEmptyConfigForm());

let ws: WebSocket | null = null;

// Load hosts + engines from the dashboard API.
async function refreshAll() {
  loading.value = true;
  errors.value = [];
  try {
    const hostsRes = await fetch("/api/hosts");
    const hostsBody = (await hostsRes.json()) as { hosts: Host[] };
    hosts.value = hostsBody.hosts ?? [];

    const enginesRes = await fetch("/api/engines");
    const enginesBody = (await enginesRes.json()) as { results: EnginesResult[] };

    const nextEngines: EngineItem[] = [];
    for (const result of enginesBody.results ?? []) {
      if (result.error) {
        errors.value.push(`${result.host.name}: ${result.error}`);
        continue;
      }
      for (const instance of result.engines ?? []) {
        nextEngines.push({ host: result.host, instance });
      }
    }
    engines.value = nextEngines;
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

// Selecting an engine updates detail views.
function selectEngine(engine: EngineItem) {
  selected.value = engine;
  if (detailTab.value === "live") {
    connectLogs();
  }
  if (detailTab.value === "history") {
    loadHistory();
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

function scrollToConfigs() {
  document.getElementById("configs")?.scrollIntoView({ behavior: "smooth" });
}

function createEmptyConfigForm() {
  return {
    id: "",
    name: "",
    engine_type: "Vllm" as EngineConfig["engine_type"],
    command: "",
    argsText: "",
    envText: "",
    working_dir: "",
    auto_restart_enabled: false,
    auto_restart_max_retries: 0,
    auto_restart_backoff_secs: 5
  };
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

function resetConfigForm() {
  configMode.value = "create";
  configForm.value = createEmptyConfigForm();
}

function editConfig(config: EngineConfig) {
  configMode.value = "edit";
  configForm.value = {
    id: config.id,
    name: config.name,
    engine_type: config.engine_type,
    command: config.command,
    argsText: config.args.join("\n"),
    envText: config.env.map((item) => `${item.key}=${item.value}`).join("\n"),
    working_dir: config.working_dir ?? "",
    auto_restart_enabled: config.auto_restart.enabled,
    auto_restart_max_retries: config.auto_restart.max_retries,
    auto_restart_backoff_secs: config.auto_restart.backoff_secs
  };
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

  const config: EngineConfig = {
    id: configForm.value.id.trim(),
    name: configForm.value.name.trim(),
    engine_type: configForm.value.engine_type,
    command: configForm.value.command.trim(),
    args: parseArgsLines(configForm.value.argsText),
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

  const method = configMode.value === "create" ? "POST" : "PUT";
  const url =
    configMode.value === "create"
      ? `/api/hosts/${configHostId.value}/configs`
      : `/api/hosts/${configHostId.value}/configs/${encodeURIComponent(config.id)}`;

  const res = await fetch(url, {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(config)
  });

  if (!res.ok) {
    const body = await res.json().catch(() => null);
    configErrors.value = [
      body?.error ? `Save failed: ${body.error}` : `Save failed (HTTP ${res.status}).`
    ];
    return;
  }

  resetConfigForm();
  await loadConfigs();
  await refreshAll();
}

async function deleteConfig(config: EngineConfig) {
  if (!configHostId.value) {
    return;
  }
  if (!confirm(`Delete config ${config.name}?`)) {
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

function parseArgsLines(raw: string) {
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
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
