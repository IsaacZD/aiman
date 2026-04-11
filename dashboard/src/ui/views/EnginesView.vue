<template>
  <section class="panel">
    <div class="panel-head">
      <div>
        <h2>Engines</h2>
        <p class="panel-sub">{{ engineCount }} configured engine(s)</p>
      </div>
    </div>

    <!-- Stats strip: quick counts by status -->
    <div class="stats-strip">
      <span class="stat-chip">
        <span class="stat-chip-dot total"></span>
        <span class="stat-chip-value">{{ totalCount }}</span>
        total
      </span>
      <span class="stat-chip">
        <span class="stat-chip-dot running"></span>
        <span class="stat-chip-value">{{ runningCount }}</span>
        running
      </span>
      <span class="stat-chip">
        <span class="stat-chip-dot stopped"></span>
        <span class="stat-chip-value">{{ stoppedCount }}</span>
        stopped
      </span>
      <span class="stat-chip">
        <span class="stat-chip-dot starting"></span>
        <span class="stat-chip-value">{{ startingCount }}</span>
        starting
      </span>
    </div>

    <!-- Filter bar -->
    <div class="filter-bar">
      <input
        class="filter-input"
        v-model="filterQuery"
        placeholder="Filter by engine or host name…"
        type="search"
      />
      <label class="show-hidden-toggle">
        <input type="checkbox" v-model="showHidden" />
        Show hidden configs
      </label>
      <span v-if="filterQuery.trim()" class="panel-sub" style="margin: 0">
        {{ filteredHosts.length === 0 ? 'No matches' : `${filteredHosts.length} host(s) match` }}
      </span>
    </div>

    <div v-if="errors.length" class="alert">
      <p v-for="error in errors" :key="error">{{ error }}</p>
    </div>

    <div class="host-sections">
      <section v-for="host in filteredHosts" :key="host.id" class="host-section">
        <div class="host-header">
          <div>
            <h3>{{ host.name }}</h3>
            <p class="host-meta">{{ host.id }} • {{ host.base_url }}</p>
          </div>
          <HardwareCard :hardware="hardwareByHost[host.id]" />
        </div>
        <div v-if="hardwareErrorsByHost[host.id]" class="alert">
          {{ hardwareErrorsByHost[host.id] }}
        </div>
        <div v-if="engineResultsByHost[host.id]?.error" class="alert">
          {{ host.name }}: {{ engineResultsByHost[host.id].error }}
        </div>
        <div class="grid">
          <EngineCard
            v-for="engine in filteredEnginesByHost[host.id] ?? []"
            :key="engine.instance.id"
            :engine="engine"
            @open-detail="$emit('open-detail', $event)"
            @start="$emit('start-engine', $event)"
            @stop="$emit('stop-engine', $event)"
            @benchmark="$emit('open-benchmark', $event)"
          />
        </div>
        <p v-if="!(filteredEnginesByHost[host.id]?.length)" class="empty">
          No engines configured for this host.
        </p>
      </section>
      <p v-if="!hosts.length" class="empty">No hosts yet.</p>
      <p v-else-if="filteredHosts.length === 0 && filterQuery.trim()" class="empty">
        No engines match "{{ filterQuery.trim() }}".
      </p>
    </div>
  </section>
</template>

<script setup lang="ts">
import { ref, computed } from "vue";
import type { Host, EngineItem, HardwareInfo, EnginesResult } from "../types";
import HardwareCard from "../components/HardwareCard.vue";
import EngineCard from "../components/EngineCard.vue";

const props = defineProps<{
  hosts: Host[];
  engineCount: number;
  engines: EngineItem[];
  enginesByHost: Record<string, EngineItem[]>;
  engineResultsByHost: Record<string, EnginesResult>;
  hardwareByHost: Record<string, HardwareInfo | null>;
  hardwareErrorsByHost: Record<string, string>;
  errors: string[];
  loading: boolean;
}>();

defineEmits<{
  (e: "open-detail", engine: EngineItem): void;
  (e: "start-engine", engine: EngineItem): void;
  (e: "stop-engine", engine: EngineItem): void;
  (e: "open-benchmark", engine: EngineItem): void;
}>();

// ── filter state ─────────────────────────────────────────────────────────────

const filterQuery = ref("");
const showHidden = ref(false);

// ── status counts (based on filtered engines) ─────────────────────────────────

const filteredEnginesFlat = computed(() => {
  return Object.values(filteredEnginesByHost.value).flat();
});

const totalCount = computed(() => filteredEnginesFlat.value.length);
const runningCount = computed(
  () => filteredEnginesFlat.value.filter((e) => e.instance.status === "Running").length
);
const stoppedCount = computed(
  () => filteredEnginesFlat.value.filter((e) => e.instance.status === "Stopped").length
);
const startingCount = computed(
  () => filteredEnginesFlat.value.filter((e) => e.instance.status === "Starting").length
);

// ── filtered views ────────────────────────────────────────────────────────────

const filteredEnginesByHost = computed(() => {
  const q = filterQuery.value.trim().toLowerCase();
  if (!q && showHidden.value) return props.enginesByHost;
  const result: Record<string, EngineItem[]> = {};
  for (const host of props.hosts) {
    let filtered = props.enginesByHost[host.id] ?? [];
    if (!showHidden.value) {
      filtered = filtered.filter((e) => e.visible !== false);
    }
    if (q) {
      filtered = filtered.filter((e) => {
        const name = (e.configName ?? "").toLowerCase();
        const hostName = host.name.toLowerCase();
        return name.includes(q) || hostName.includes(q);
      });
    }
    if (filtered.length) {
      result[host.id] = filtered;
    }
  }
  return result;
});

const filteredHosts = computed(() => {
  const q = filterQuery.value.trim().toLowerCase();
  if (!q) return props.hosts;
  return props.hosts.filter((h) => !!filteredEnginesByHost.value[h.id]);
});
</script>
