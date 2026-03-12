<template>
  <div class="list-editor">
    <div v-for="(arg, index) in args" :key="index" class="list-editor-row">
      <input v-model="args[index]" type="text" :placeholder="placeholder" />
      <button class="ghost list-editor-remove" type="button" @click="remove(index)">
        Remove
      </button>
    </div>
    <button class="ghost list-editor-add" type="button" @click="add">
      {{ addLabel }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, defineModel } from "vue";

const props = defineProps<{
  addLabel?: string;
  placeholder?: string;
}>();

const args = defineModel<string[]>({ required: true });

const addLabel = computed(() => props.addLabel ?? "Add argument");
const placeholder = computed(() => props.placeholder ?? "");

function add() {
  args.value.push("");
}

function remove(index: number) {
  args.value.splice(index, 1);
}
</script>
