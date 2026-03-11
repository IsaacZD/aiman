<template>
  <div class="engine-fields">
    <label>
      Model path
      <input v-model="form.modelPath" type="text" placeholder="/models/ktr.gguf" />
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
import { defineModel } from "vue";
import type { KTransformersArgsForm } from "../engine-args/kTransformers";

const form = defineModel<KTransformersArgsForm>({ required: true });
</script>
