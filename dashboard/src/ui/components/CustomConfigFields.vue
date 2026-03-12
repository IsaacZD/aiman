<template>
  <div class="engine-fields">
    <label>
      Arguments
      <ArgumentListEditor
        v-model="form.args"
        add-label="Add argument"
        placeholder="--flag value"
      />
    </label>
  </div>
</template>

<script setup lang="ts">
import { defineModel } from "vue";
import type { CustomArgsForm } from "../engine-args/custom";
import ArgumentListEditor from "./ArgumentListEditor.vue";

const form = defineModel<CustomArgsForm>({ required: true });
</script>
