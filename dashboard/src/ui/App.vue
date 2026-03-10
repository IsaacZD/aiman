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
          <h2>Live logs</h2>
          <p class="panel-sub">
            {{ selected ? `${selected.host.name} • ${selected.instance.id}` : "Pick an engine" }}
          </p>
        </div>
        <button class="secondary" @click="clearLogs" :disabled="!selected">Clear</button>
      </div>

      <div class="logs">
        <p v-if="!selected" class="empty">Select an engine to stream logs.</p>
        <div v-else class="log-lines">
          <p v-for="(line, idx) in logs" :key="idx">{{ line }}</p>
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

const hosts = ref<Host[]>([]);
const engines = ref<EngineItem[]>([]);
const selected = ref<EngineItem | null>(null);
const logs = ref<string[]>([]);
const errors = ref<string[]>([]);
const loading = ref(false);
const lastRefreshed = ref<string | null>(null);

let ws: WebSocket | null = null;

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

function selectEngine(engine: EngineItem) {
  selected.value = engine;
  connectLogs();
}

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

function clearLogs() {
  logs.value = [];
}

function statusClass(status: string) {
  return `status-${status.toLowerCase()}`;
}

onMounted(refreshAll);

onBeforeUnmount(() => {
  if (ws) {
    ws.close();
  }
});
</script>
