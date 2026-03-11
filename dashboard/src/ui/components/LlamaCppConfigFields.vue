<template>
  <div class="engine-fields">
    <label>
      Model path
      <div class="model-input-row">
        <input v-model="form.modelPath" type="text" placeholder="/models/qwen.gguf" />
        <button
          class="ghost model-picker-btn"
          type="button"
          :disabled="!modelOptions.length"
          @click="openPicker"
        >
          Browse
        </button>
      </div>
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
import { computed, defineModel } from "vue";
import type { LlamaCppArgsForm } from "../engine-args/llamaCpp";

type ModelOption = {
  path: string;
  label: string;
};

const props = defineProps<{
  modelOptions?: ModelOption[];
  openModelPicker?: (onSelect: (path: string) => void) => void;
}>();
const form = defineModel<LlamaCppArgsForm>({ required: true });
const modelOptions = computed(() => props.modelOptions ?? []);

function openPicker() {
  props.openModelPicker?.((path) => {
    form.value.modelPath = path;
  });
}
</script>
