<template>
  <article
    class="engine"
    :class="statusClass(engine.instance.status)"
    @click="$emit('open-detail', engine)"
  >
    <span class="status-dot" :class="statusDotClass(engine.instance.status)"></span>
    <header>
      <p class="engine-host">{{ engine.host.name }}</p>
      <h3>{{ engine.configName ?? engine.instance.id }}</h3>
      <p class="engine-type">{{ engine.instance.config_id }}</p>
    </header>
    <div class="engine-meta">
      <span class="pill">PID {{ engine.instance.pid ?? "—" }}</span>
    </div>
    <div class="engine-actions">
      <button class="secondary" @click.stop="$emit('start', engine)">Start</button>
      <button class="ghost" @click.stop="$emit('stop', engine)">Stop</button>
      <button
        class="secondary"
        :disabled="engine.instance.status !== 'Running'"
        @click.stop="$emit('benchmark', engine)"
      >
        Benchmark
      </button>
    </div>
  </article>
</template>

<script setup lang="ts">
import type { EngineItem } from "../types";
import { statusClass, statusDotClass } from "../utils/format";

defineProps<{
  engine: EngineItem;
}>();

defineEmits<{
  (e: "open-detail", engine: EngineItem): void;
  (e: "start", engine: EngineItem): void;
  (e: "stop", engine: EngineItem): void;
  (e: "benchmark", engine: EngineItem): void;
}>();
</script>
