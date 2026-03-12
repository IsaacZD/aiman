<template>
  <div class="list-editor env-editor">
    <div v-for="(entry, index) in entries" :key="index" class="list-editor-row env-editor-row">
      <input v-model="entries[index].key" type="text" placeholder="HF_HOME" />
      <input v-model="entries[index].value" type="text" placeholder="/data/hf" />
      <button class="ghost list-editor-remove" type="button" @click="remove(index)">
        Remove
      </button>
    </div>
    <button class="ghost list-editor-add" type="button" @click="add">Add env var</button>
  </div>
</template>

<script setup lang="ts">
import { defineModel } from "vue";

type EnvVarEntry = {
  key: string;
  value: string;
};

const entries = defineModel<EnvVarEntry[]>({ required: true });

function add() {
  entries.value.push({ key: "", value: "" });
}

function remove(index: number) {
  entries.value.splice(index, 1);
}
</script>
