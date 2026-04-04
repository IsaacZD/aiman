<template>
  <section class="panel">
    <div class="panel-head">
      <div>
        <h2>Benchmarks</h2>
        <p class="panel-sub">
          Historical runs across hosts. Each entry stores a full config snapshot.
        </p>
      </div>
      <div class="panel-actions">
        <button class="secondary" @click="$emit('refresh')" :disabled="loading">
          {{ loading ? "Refreshing..." : "Refresh" }}
        </button>
      </div>
    </div>

    <div v-if="errors.length" class="alert">
      <p v-for="error in errors" :key="error">{{ error }}</p>
    </div>

    <div class="benchmark-list">
      <article v-for="record in records" :key="record.id" class="benchmark-card">
        <div class="benchmark-head">
          <div>
            <h3>{{ record.engine_config.name }}</h3>
            <p class="benchmark-meta">
              {{ record.host?.name ?? "Unknown host" }} •
              {{ record.engine_config.engine_type }}
            </p>
            <p class="benchmark-meta">Run {{ formatBenchmarkTime(record.ts) }}</p>
          </div>
          <div class="benchmark-tags">
            <span class="pill">Model {{ record.settings.model }}</span>
            <span class="pill">pp {{ record.settings.pp.join(",") }}</span>
            <span class="pill">tg {{ record.settings.tg.join(",") }}</span>
            <span v-if="record.settings.prefix_caching" class="pill">prefix cache</span>
          </div>
        </div>

        <div class="benchmark-details">
          <div>
            <p class="benchmark-label">Host hardware</p>
            <p class="benchmark-value">
              {{ formatCpu(record.host_hardware) }} •
              {{ formatMemory(record.host_hardware) }} •
              {{ formatGpus(record.host_hardware) }}
            </p>
          </div>
        </div>

        <pre class="benchmark-output">{{ record.output }}</pre>
      </article>
      <p v-if="!records.length" class="empty">No benchmarks yet.</p>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { BenchmarkRecord } from "../types";
import { formatBenchmarkTime, formatCpu, formatMemory, formatGpus } from "../utils/format";

defineProps<{
  records: BenchmarkRecord[];
  errors: string[];
  loading: boolean;
}>();

defineEmits<{
  (e: "refresh"): void;
}>();
</script>
