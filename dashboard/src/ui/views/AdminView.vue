<template>
  <section class="panel admin-panel">
    <div class="panel-head">
      <div>
        <h2>Admin management</h2>
        <p class="panel-sub">Managing: <strong>{{ selectedHost?.name ?? '—' }}</strong></p>
      </div>
    </div>

    <div class="admin-grid">
      <div class="admin-pane">
        <div class="pane-head">
          <div class="pane-title">
            <h3>Hosts</h3>
            <button class="secondary" @click="$emit('open-host-modal')">New host</button>
          </div>
          <p class="panel-sub">Pick a host to manage configs.</p>
        </div>
        <div v-if="hostErrors.length" class="alert">
          <p v-for="error in hostErrors" :key="error">{{ error }}</p>
        </div>
        <div class="host-list">
          <article
            v-for="host in hosts"
            :key="host.id"
            class="host-card clickable"
            :class="{ active: host.id === selectedHostId }"
            @click="$emit('select-host', host)"
          >
            <div>
              <h3>{{ host.name }}</h3>
              <p class="host-meta">{{ host.id }} • {{ host.base_url }}</p>
            </div>
            <div class="host-actions">
              <button class="secondary" @click.stop="$emit('open-host-modal', host)">Edit</button>
            </div>
          </article>
          <p v-if="!hosts.length" class="empty">No hosts yet.</p>
        </div>
      </div>

      <div v-if="selectedHost" class="admin-pane">
        <div class="pane-head">
          <div class="pane-title">
            <h3>Configs</h3>
            <button class="secondary" @click="$emit('open-config-modal')">New config</button>
          </div>
          <p class="panel-sub">
            {{ selectedHost ? `${selectedHost.name} • ${selectedHost.id}` : "Select a host." }}
          </p>
        </div>
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

      <div v-if="selectedHost" class="admin-pane">
        <div class="pane-head">
          <div class="pane-title">
            <h3>Images</h3>
            <button class="secondary" @click="$emit('open-image-modal')">New image</button>
          </div>
          <p class="panel-sub">
            {{ selectedHost ? `${selectedHost.name} • ${selectedHost.id}` : "Select a host." }}
          </p>
        </div>
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
    </div>
  </section>
</template>

<script setup lang="ts">
import type { Host, EngineConfig, DockerImage } from "../types";

defineProps<{
  hosts: Host[];
  selectedHost: Host | null;
  selectedHostId: string | null;
  configs: EngineConfig[];
  images: DockerImage[];
  hostErrors: string[];
  configErrors: string[];
  imageErrors: string[];
}>();

defineEmits<{
  (e: "open-host-modal", host?: Host): void;
  (e: "select-host", host: Host): void;
  (e: "open-config-modal", config?: EngineConfig): void;
  (e: "open-config-template-modal", config: EngineConfig): void;
  (e: "open-image-modal", image?: DockerImage): void;
}>();
</script>
