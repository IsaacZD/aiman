<template>
  <div class="engine-fields">
    <label>
      Model path <code>--model</code>
      <div class="model-input-row">
        <input v-model="form.modelPath" type="text" placeholder="/models/ktr.gguf" />
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
      <input v-model="form.port" type="text" placeholder="8080" />
    </label>
    <label>
      Additional args
      <ArgumentListEditor v-model="form.extraArgs" add-label="Add argument" placeholder="--gpu-layers 40" />
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed, defineModel } from "vue";
import type { KTransformersArgsForm } from "../engine-args/kTransformers";
import ArgumentListEditor from "./ArgumentListEditor.vue";

type ModelOption = {
  path: string;
  label: string;
};

const props = defineProps<{
  modelOptions?: ModelOption[];
  openModelPicker?: (onSelect: (path: string) => void) => void;
}>();
const form = defineModel<KTransformersArgsForm>({ required: true });
const modelOptions = computed(() => props.modelOptions ?? []);

function openPicker() {
  props.openModelPicker?.((path) => {
    form.value.modelPath = path;
  });
}
</script>
