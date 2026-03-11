<template>
  <div class="engine-fields">
    <label>
      Model path or name
      <input v-model="form.modelPath" type="text" placeholder="/models/llama" />
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
import { defineModel } from "vue";
import type { VllmArgsForm } from "../engine-args/vllm";

const form = defineModel<VllmArgsForm>({ required: true });
</script>
