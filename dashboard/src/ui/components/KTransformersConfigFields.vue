<template>
  <div class="engine-fields">
    <label>
      Model path
      <input
        v-model="form.modelPath"
        type="text"
        placeholder="/models/ktr.gguf"
        :list="modelListId"
      />
      <datalist v-if="modelOptions.length" :id="modelListId">
        <option v-for="option in modelOptions" :key="option.path" :value="option.path">
          {{ option.label }}
        </option>
      </datalist>
    </label>
    <label>
      Port (optional)
      <input v-model="form.port" type="text" placeholder="8080" />
    </label>
    <label>
      Additional args (one per line)
      <textarea v-model="form.extraArgsText" rows="4" placeholder="--gpu-layers\n40"></textarea>
    </label>
  </div>
</template>

<script setup lang="ts">
import { computed, defineModel } from "vue";
import type { KTransformersArgsForm } from "../engine-args/kTransformers";

type ModelOption = {
  path: string;
  label: string;
};

const props = defineProps<{ modelOptions?: ModelOption[] }>();
const form = defineModel<KTransformersArgsForm>({ required: true });
const modelListId = "ktransformers-models";
const modelOptions = computed(() => props.modelOptions ?? []);
</script>
