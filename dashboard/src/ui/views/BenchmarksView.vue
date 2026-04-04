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
            <span class="pill">{{ formatBenchmarkOrigin(record.origin) }}</span>
            <span class="pill">Model {{ record.settings.model }}</span>
            <span class="pill">Max tokens {{ record.settings.max_tokens }}</span>
            <span class="pill">Prompt {{ record.settings.prompt_words }} words</span>
          </div>
        </div>

        <div class="benchmark-details">
          <div>
            <p class="benchmark-label">Prompt</p>
            <p class="benchmark-value">
              {{ truncatePrompt(record.settings.prompt) }}
            </p>
          </div>
          <div>
            <p class="benchmark-label">Host hardware</p>
            <p class="benchmark-value">
              {{ formatCpu(record.host_hardware) }} •
              {{ formatMemory(record.host_hardware) }} •
              {{ formatGpus(record.host_hardware) }}
            </p>
          </div>
        </div>

        <div class="benchmark-table">
          <table>
            <thead>
              <tr>
                <th>Concurrency</th>
                <th>Requests</th>
                <th>Success</th>
                <th>Avg latency</th>
                <th>P90 latency</th>
                <th>Prompt tps</th>
                <th>Completion tps</th>
                <th>Req/s</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="result in record.results" :key="result.concurrency">
                <td>{{ result.concurrency }}</td>
                <td>{{ result.requests }}</td>
                <td>{{ result.success_count }}</td>
                <td>{{ formatMs(result.avg_latency_ms) }}</td>
                <td>{{ formatMs(result.p90_latency_ms) }}</td>
                <td>{{ formatRate(result.prompt_tps) }}</td>
                <td>{{ formatRate(result.completion_tps) }}</td>
                <td>{{ formatRate(result.requests_per_sec) }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </article>
      <p v-if="!records.length" class="empty">No benchmarks yet.</p>
    </div>
  </section>
</template>

<script setup lang="ts">
import type { BenchmarkRecord } from "../types";
import {
  formatBenchmarkTime,
  formatBenchmarkOrigin,
  formatMs,
  formatRate,
  truncatePrompt,
  formatCpu,
  formatMemory,
  formatGpus
} from "../utils/format";

defineProps<{
  records: BenchmarkRecord[];
  errors: string[];
  loading: boolean;
}>();

defineEmits<{
  (e: "refresh"): void;
}>();
</script>
