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
          <button class="ghost" @click="$emit('prepare-all-images')" :disabled="!hasUnpreparedImages">
            Prepare all
          </button>
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
              <h3>
                {{ image.name || image.id }}
                <span class="image-status-badge" :class="imageStatusClass(image.status)">
                  {{ image.status ?? "NotReady" }}
                </span>
              </h3>
              <p class="config-meta">{{ image.build ? "Dockerfile" : image.image }}</p>
              <p class="config-meta config-id">{{ image.id }}</p>
            </div>
            <div class="config-actions">
              <button
                class="ghost"
                @click="$emit('prepare-image', image)"
                :disabled="image.status === 'Preparing'"
              >
                {{ image.status === "Preparing" ? "Preparing…" : image.build ? "Build" : "Pull" }}
              </button>
              <button class="ghost" @click="$emit('open-image-template-modal', image)">
                Create from template
              </button>
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
import { ref, computed, watch } from "vue";
import type { Host, EngineConfig, ContainerImage, ImageStatus } from "../types";

const props = defineProps<{
  selectedHost: Host | null;
  configs: EngineConfig[];
  images: ContainerImage[];
  hostErrors: string[];
  configErrors: string[];
  imageErrors: string[];
}>();

defineEmits<{
  (e: "open-config-modal", config?: EngineConfig): void;
  (e: "open-config-template-modal", config: EngineConfig): void;
  (e: "open-image-modal", image?: ContainerImage): void;
  (e: "open-image-template-modal", image: ContainerImage): void;
  (e: "prepare-image", image: ContainerImage): void;
  (e: "prepare-all-images"): void;
  (e: "prune-images"): void;
}>();

const adminTab = ref<"configs" | "images">("configs");

const hasUnpreparedImages = computed(() =>
  props.images.some((img) => img.status !== "Ready" && img.status !== "Preparing")
);

function imageStatusClass(status: ImageStatus | undefined): string {
  switch (status) {
    case "Ready":
      return "status-ready";
    case "Preparing":
      return "status-preparing";
    case "Failed":
      return "status-failed";
    default:
      return "status-not-ready";
  }
}

// Reset to configs tab when switching to a different host.
watch(
  () => props.selectedHost?.id,
  () => {
    adminTab.value = "configs";
  }
);
</script>

<style scoped>
.image-status-badge {
  display: inline-block;
  font-size: 0.7rem;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 8px;
  margin-left: 8px;
  vertical-align: middle;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.status-ready {
  background: rgba(60, 207, 145, 0.18);
  color: #3ccf91;
}
.status-preparing {
  background: rgba(245, 192, 79, 0.18);
  color: #f5c04f;
}
.status-failed {
  background: rgba(255, 107, 107, 0.18);
  color: #ff6b6b;
}
.status-not-ready {
  background: rgba(154, 163, 178, 0.18);
  color: #9aa3b2;
}
</style>
