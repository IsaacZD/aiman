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
        <button class="primary" @click="refreshAll" :disabled="loading">
          {{ loading ? "Refreshing..." : "Refresh" }}
        </button>
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

// Initial load and cleanup.
onMounted(refreshAll);

onBeforeUnmount(() => {
  if (ws) {
    ws.close();
  }
});
</script>
