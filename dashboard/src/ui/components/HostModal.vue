<template>
  <div v-if="show" class="modal-backdrop">
    <div class="modal">
      <div class="modal-head">
        <h3>{{ mode === "create" ? "Create host" : "Edit host" }}</h3>
        <button class="ghost" @click="$emit('close')">Close</button>
      </div>
      <form class="host-form" @submit.prevent="$emit('submit')">
        <label class="config-id-label">
          <span class="config-id-title">Host ID</span>
          <span class="config-id-value">
            {{ form.id || "auto-generated" }}
          </span>
        </label>
        <label>
          Name
          <input v-model="form.name" type="text" placeholder="LLM Server 01" />
        </label>
        <label>
          Base URL
          <input v-model="form.base_url" type="text" placeholder="http://10.0.0.12:4010" />
        </label>
        <label>
          API key (optional)
          <input v-model="form.api_key" type="password" placeholder="dev-secret" />
        </label>
        <label>
          Model libraries (one path per line)
          <textarea
            v-model="form.model_libraries_text"
            rows="3"
            placeholder="/data/huggingface\n/mnt/models/hf"
          ></textarea>
        </label>
        <div v-if="errors.length" class="alert">
          <p v-for="error in errors" :key="error">{{ error }}</p>
        </div>
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
            {{ mode === "create" ? "Create host" : "Save changes" }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  show: boolean;
  mode: "create" | "edit";
  form: {
    id: string;
    name: string;
    base_url: string;
    api_key: string;
    model_libraries_text: string;
  };
  errors: string[];
}>();

defineEmits<{
  (e: "close"): void;
  (e: "submit"): void;
  (e: "delete"): void;
}>();
</script>
