<template>
  <div v-if="show" class="modal-backdrop">
    <div class="modal modal-wide">
      <div class="modal-head">
        <h3>Run benchmark</h3>
        <button class="ghost" @click="$emit('close')">Close</button>
      </div>
      <p class="panel-sub">
        {{
          target
            ? `${target.host.name} • ${target.configName ?? target.instance.id}`
            : "Pick an engine"
        }}
      </p>
      <div v-if="error" class="alert">
        {{ error }}
      </div>
      <form class="config-form" @submit.prevent="$emit('submit')">
        <label>
          Model override (optional)
          <input v-model="localForm.model" type="text" placeholder="leave blank for auto-detect" />
        </label>
        <label>
          API base URL override (optional)
          <input v-model="localForm.apiBaseUrl" type="text" placeholder="http://127.0.0.1:8000" />
        </label>
        <label>
          API key (optional, not stored)
          <input v-model="localForm.apiKey" type="password" placeholder="engine API key" />
        </label>
        <label>
          Prompt sizes — <code>--pp</code> (comma separated token counts)
          <input v-model="localForm.pp" type="text" placeholder="512,2048" />
        </label>
        <label>
          Generation sizes — <code>--tg</code> (comma separated token counts)
          <input v-model="localForm.tg" type="text" placeholder="32,128" />
        </label>
        <label>
          Context depths — <code>--depth</code> (comma separated token counts)
          <input v-model="localForm.depth" type="text" placeholder="0" />
        </label>
        <label>
          Concurrency levels (comma separated)
          <input v-model="localForm.concurrency" type="text" placeholder="1" />
        </label>
        <label>
          Runs per test
          <input v-model.number="localForm.runs" type="number" min="1" />
        </label>
        <label>
          Latency mode
          <select v-model="localForm.latencyMode">
            <option value="generation">generation</option>
            <option value="api">api</option>
            <option value="none">none</option>
          </select>
        </label>
        <label class="checkbox-label">
          <input v-model="localForm.prefixCaching" type="checkbox" />
          Enable prefix caching
        </label>
        <label class="checkbox-label">
          <input v-model="localForm.noWarmup" type="checkbox" />
          Skip warmup
        </label>
        <div class="form-actions">
          <button class="ghost" type="button" @click="$emit('close')">Cancel</button>
          <button class="primary" type="submit" :disabled="running || !target">
            {{ running ? "Running..." : "Run benchmark" }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { EngineItem } from "../types";

const props = defineProps<{
  show: boolean;
  target: EngineItem | null;
  form: {
    pp: string;
    tg: string;
    depth: string;
    runs: number;
    concurrency: string;
    model: string;
    apiBaseUrl: string;
    apiKey: string;
    prefixCaching: boolean;
    latencyMode: "api" | "generation" | "none";
    noWarmup: boolean;
  };
  error: string | null;
  running: boolean;
}>();

defineEmits<{
  (e: "close"): void;
  (e: "submit"): void;
}>();

// Directly expose localForm as a writable alias to prop.form (parent passes the same ref)
const localForm = props.form;
</script>
