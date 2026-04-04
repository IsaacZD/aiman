<template>
  <section class="panel">
    <div class="panel-head">
      <div>
        <h2>Engines</h2>
        <p class="panel-sub">{{ engineCount }} configured engine(s)</p>
      </div>
      <div class="panel-actions">
        <button class="primary" @click="$emit('refresh')" :disabled="loading">
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
            v-for="engine in enginesByHost[host.id] ?? []"
            :key="engine.instance.id"
            :engine="engine"
            @open-detail="$emit('open-detail', $event)"
            @start="$emit('start-engine', $event)"
            @stop="$emit('stop-engine', $event)"
            @benchmark="$emit('open-benchmark', $event)"
          />
        </div>
        <p v-if="!(enginesByHost[host.id]?.length)" class="empty">
          No engines configured for this host.
        </p>
      </section>
      <p v-if="!hosts.length" class="empty">No hosts yet.</p>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { Host, EngineItem, HardwareInfo, EnginesResult } from "../types";
import HardwareCard from "../components/HardwareCard.vue";
import EngineCard from "../components/EngineCard.vue";

defineProps<{
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
  (e: "refresh"): void;
  (e: "open-detail", engine: EngineItem): void;
  (e: "start-engine", engine: EngineItem): void;
  (e: "stop-engine", engine: EngineItem): void;
  (e: "open-benchmark", engine: EngineItem): void;
}>();
</script>
