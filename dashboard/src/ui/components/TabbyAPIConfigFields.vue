<template>
  <div class="engine-fields">
    <label>
      Model directory <code>--model-dir</code>
      <div class="model-input-row">
        <input v-model="form.modelDir" type="text" placeholder="/models/exl3-model" />
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
      Port <code>--port</code>
      <input v-model="form.port" type="text" placeholder="5000" />
    </label>
    <label>
      GPU split <code>--gpu-split</code>
      <input v-model="form.gpuSplit" type="text" placeholder="24,24" />
    </label>
    <label>
      Additional args
      <ArgumentListEditor
        v-model="form.extraArgs"
        add-label="Add argument"
        placeholder="--max-seq-len 8192"
      />
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed, defineModel } from "vue";
import type { TabbyAPIArgsForm } from "../engine-args/tabbyapi";
import ArgumentListEditor from "./ArgumentListEditor.vue";

type ModelOption = {
  path: string;
  label: string;
};

const props = defineProps<{
  modelOptions?: ModelOption[];
  openModelPicker?: (onSelect: (path: string) => void) => void;
}>();
const form = defineModel<TabbyAPIArgsForm>({ required: true });
const modelOptions = computed(() => props.modelOptions ?? []);

function openPicker() {
  props.openModelPicker?.((path) => {
    form.value.modelDir = path;
  });
}
</script>
