<template>
  <div class="engine-fields">
    <label>
      Model path
      <input v-model="form.modelPath" type="text" placeholder="/models/qwen.gguf" />
    </label>
    <label>
      Port (optional)
      <input v-model="form.port" type="text" placeholder="8081" />
    </label>
    <label>
      GPU layers (optional)
      <input v-model="form.gpuLayers" type="text" placeholder="40" />
    </label>
    <label>
      Context size (optional)
      <input v-model="form.ctxSize" type="text" placeholder="4096" />
    </label>
    <label>
      Additional args (one per line)
      <textarea v-model="form.extraArgsText" rows="4" placeholder="--threads\n8"></textarea>
    </label>
  </div>
</template>

<script setup lang="ts">
import { defineModel } from "vue";
import type { LlamaCppArgsForm } from "../engine-args/llamaCpp";

const form = defineModel<LlamaCppArgsForm>({ required: true });
</script>
