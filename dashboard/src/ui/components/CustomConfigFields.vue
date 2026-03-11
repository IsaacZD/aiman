<template>
  <div class="engine-fields">
    <label>
      Arguments (one per line)
      <textarea v-model="form.argsText" rows="6" placeholder="--flag\nvalue"></textarea>
    </label>
  </div>
</template>

<script setup lang="ts">
import { defineModel } from "vue";
import type { CustomArgsForm } from "../engine-args/custom";

const form = defineModel<CustomArgsForm>({ required: true });
</script>
