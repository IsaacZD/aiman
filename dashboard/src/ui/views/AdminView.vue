<template>
  <section class="panel admin-panel">
    <div class="panel-head">
      <div>
        <h2>Admin management</h2>
        <p class="panel-sub">Managing: <strong>{{ selectedHost?.name ?? '—' }}</strong></p>
      </div>
    </div>

    <div v-if="hostErrors.length" class="alert">
      <p v-for="error in hostErrors" :key="error">{{ error }}</p>
    </div>

    <p v-if="!selectedHost" class="empty">Select a host from the sidebar to manage its configs and images.</p>

    <template v-else>
      <!-- Tab bar with inline action button -->
      <div class="admin-tabs">
        <button class="tab" :class="{ active: adminTab === 'configs' }" @click="adminTab = 'configs'">
          Configs
        </button>
        <button class="tab" :class="{ active: adminTab === 'images' }" @click="adminTab = 'images'">
          Images
        </button>
        <div class="admin-tabs-spacer"></div>
        <button v-if="adminTab === 'configs'" class="secondary" @click="$emit('open-config-modal')">
          New config
        </button>
        <template v-if="adminTab === 'images'">
          <button class="ghost" @click="$emit('prune-images')">Prune images</button>
          <button class="secondary" @click="$emit('open-image-modal')">New image</button>
        </template>
      </div>

      <!-- Configs tab -->
      <div v-if="adminTab === 'configs'">
        <div v-if="configErrors.length" class="alert">
          <p v-for="error in configErrors" :key="error">{{ error }}</p>
        </div>
        <div class="config-list">
          <article v-for="config in configs" :key="config.id" class="config-card">
            <div>
              <h3>{{ config.name }}</h3>
              <p class="config-meta">{{ config.engine_type }}</p>
              <p class="config-meta config-id">{{ config.id }}</p>
            </div>
            <div class="config-actions">
              <button class="ghost" @click="$emit('open-config-template-modal', config)">
                Create from template
              </button>
              <button class="secondary" @click="$emit('open-config-modal', config)">Edit</button>
            </div>
          </article>
          <p v-if="!configs.length" class="empty">No configs yet.</p>
        </div>
      </div>

      <!-- Images tab -->
      <div v-if="adminTab === 'images'">
        <div v-if="imageErrors.length" class="alert">
          <p v-for="error in imageErrors" :key="error">{{ error }}</p>
        </div>
        <div class="config-list">
          <article v-for="image in images" :key="image.id" class="config-card">
            <div>
              <h3>{{ image.name || image.id }}</h3>
              <p class="config-meta">{{ image.image }}</p>
              <p class="config-meta config-id">{{ image.id }}</p>
            </div>
            <div class="config-actions">
              <button class="secondary" @click="$emit('open-image-modal', image)">Edit</button>
            </div>
          </article>
          <p v-if="!images.length" class="empty">No images yet.</p>
        </div>
      </div>
    </template>
  </section>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import type { Host, EngineConfig, DockerImage } from "../types";

const props = defineProps<{
  selectedHost: Host | null;
  configs: EngineConfig[];
  images: DockerImage[];
  hostErrors: string[];
  configErrors: string[];
  imageErrors: string[];
}>();

defineEmits<{
  (e: "open-config-modal", config?: EngineConfig): void;
  (e: "open-config-template-modal", config: EngineConfig): void;
  (e: "open-image-modal", image?: DockerImage): void;
  (e: "prune-images"): void;
}>();

const adminTab = ref<"configs" | "images">("configs");

// Reset to configs tab when switching to a different host.
watch(
  () => props.selectedHost?.id,
  () => {
    adminTab.value = "configs";
  }
);
</script>
