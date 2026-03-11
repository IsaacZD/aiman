<template>
  <div class="engine-fields">
    <label>
      Model path or name
      <div class="model-input-row">
        <input v-model="form.modelPath" type="text" placeholder="/models/llama" />
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
      <input v-model="form.port" type="text" placeholder="8000" />
    </label>
    <label>
      Tensor parallel size (optional)
      <input v-model="form.tensorParallelSize" type="text" placeholder="2" />
    </label>
    <label>
      Additional args (one per line)
      <textarea v-model="form.extraArgsText" rows="4" placeholder="--max-model-len\n4096"></textarea>
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed, defineModel } from "vue";
import type { VllmArgsForm } from "../engine-args/vllm";

type ModelOption = {
  path: string;
  label: string;
};

const props = defineProps<{
  modelOptions?: ModelOption[];
  openModelPicker?: (onSelect: (path: string) => void) => void;
}>();
const form = defineModel<VllmArgsForm>({ required: true });
const modelOptions = computed(() => props.modelOptions ?? []);

function openPicker() {
  props.openModelPicker?.((path) => {
    form.value.modelPath = path;
  });
}
</script>
