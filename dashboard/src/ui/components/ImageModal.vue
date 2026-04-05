<template>
  <div v-if="show" class="modal-backdrop">
    <div class="modal modal-wide">
      <div class="modal-head">
        <h3>{{ mode === "create" ? "Create image" : "Edit image" }}</h3>
        <button class="ghost" @click="$emit('close')">Close</button>
      </div>
      <div v-if="errors.length" class="alert">
        <p v-for="error in errors" :key="error">{{ error }}</p>
      </div>
      <form class="config-form" @submit.prevent="$emit('submit')">
        <ContainerImageFields v-model="form" :id-locked="mode === 'edit'" />
        <div class="form-actions">
          <button
            v-if="mode === 'edit'"
            class="ghost"
            type="button"
            @click="$emit('delete')"
          >
            Delete
          </button>
          <button class="primary" type="submit">
            {{ mode === "create" ? "Create image" : "Save changes" }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import ContainerImageFields from "./ContainerImageFields.vue";

defineProps<{
  show: boolean;
  mode: "create" | "edit";
  errors: string[];
}>();

defineEmits<{
  (e: "close"): void;
  (e: "submit"): void;
  (e: "delete"): void;
}>();

const form = defineModel<any>({ required: true });
</script>
