<template>
  <div class="engine-fields">
    <label>
      Model path or name
      <div class="model-input-row">
        <input v-model="form.modelPath" type="text" placeholder="Qwen/Qwen3-0.6B" />
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
      <input v-model="form.port" type="text" placeholder="8080" />
    </label>
    <label>
      Additional args (one per line)
      <textarea v-model="form.extraArgsText" rows="4" placeholder="--model-name\nqwen"></textarea>
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed, defineModel } from "vue";
import type { FastllmArgsForm } from "../engine-args/fastllm";

type ModelOption = {
  path: string;
  label: string;
};

const props = defineProps<{
  modelOptions?: ModelOption[];
  openModelPicker?: (onSelect: (path: string) => void) => void;
}>();
// Reuse the shared model picker, but keep the label text FastLLM-specific.
const form = defineModel<FastllmArgsForm>({ required: true });
const modelOptions = computed(() => props.modelOptions ?? []);

function openPicker() {
  props.openModelPicker?.((path) => {
    form.value.modelPath = path;
  });
}
</script>
