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
          Run location
          <select v-model="localForm.mode">
            <option value="host">Host machine</option>
            <option value="dashboard">Dashboard machine</option>
          </select>
        </label>
        <label>
          Parallelism (comma separated)
          <input v-model="localForm.concurrencyText" type="text" placeholder="1,2,4,8" />
        </label>
        <label>
          Requests per concurrency
          <input v-model.number="localForm.requestsPerConcurrency" type="number" min="1" />
        </label>
        <label>
          Max tokens
          <input v-model.number="localForm.maxTokens" type="number" min="1" />
        </label>
        <label>
          Temperature
          <input v-model.number="localForm.temperature" type="number" min="0" max="2" step="0.1" />
        </label>
        <label>
          Model override (optional)
          <input v-model="localForm.model" type="text" placeholder="leave blank for auto" />
        </label>
        <label>
          Prompt words
          <input v-model.number="localForm.promptWords" type="number" min="1" />
        </label>
        <label>
          Custom prompt (optional)
          <textarea
            v-model="localForm.prompt"
            rows="3"
            placeholder="leave blank to auto-generate"
          ></textarea>
        </label>
        <label>
          API base URL override (optional)
          <input
            v-model="localForm.apiBaseUrl"
            type="text"
            placeholder="http://127.0.0.1:8000"
          />
        </label>
        <label>
          API key (optional, not stored)
          <input v-model="localForm.apiKey" type="password" placeholder="engine API key" />
        </label>
        <label>
          Timeout (seconds)
          <input v-model.number="localForm.timeoutSeconds" type="number" min="10" />
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
    mode: "host" | "dashboard";
    concurrencyText: string;
    requestsPerConcurrency: number;
    maxTokens: number;
    temperature: number;
    model: string;
    promptWords: number;
    prompt: string;
    apiBaseUrl: string;
    apiKey: string;
    timeoutSeconds: number;
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
