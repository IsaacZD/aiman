<template>
  <div class="app-shell">
    <!-- Topbar -->
    <header class="topbar">
      <span class="topbar-brand">aiman</span>
      <div class="topbar-stats">
        <span class="topbar-stat">
          <span class="topbar-stat-dot hosts"></span>
          <span class="topbar-stat-value">{{ hosts.length }}</span>
          hosts
        </span>
        <span class="topbar-stat">
          <span class="topbar-stat-dot running"></span>
          <span class="topbar-stat-value">{{ runningCount }}</span>
          running
        </span>
        <span class="topbar-stat">
          <span class="topbar-stat-dot stopped"></span>
          <span class="topbar-stat-value">{{ stoppedCount }}</span>
          stopped
        </span>
        <span class="topbar-stat">
          <span class="topbar-stat-dot starting"></span>
          <span class="topbar-stat-value">{{ startingCount }}</span>
          starting
        </span>
      </div>
      <div class="topbar-spacer"></div>
      <span v-if="lastRefreshed" class="topbar-lastrefresh">Refreshed {{ lastRefreshed }}</span>
      <button class="secondary" @click="refreshAll" :disabled="loading">
        {{ loading ? "Refreshing…" : "Refresh" }}
      </button>
    </header>

    <!-- Sidebar -->
    <nav class="sidebar">
      <div class="sidebar-nav">
        <button
          class="sidebar-item"
          :class="{ active: mainTab === 'engines' }"
          @click="mainTab = 'engines'"
        >
          <span class="sidebar-item-icon">⚙</span>
          Engines
        </button>
        <button
          class="sidebar-item"
          :class="{ active: mainTab === 'benchmarks' }"
          @click="mainTab = 'benchmarks'"
        >
          <span class="sidebar-item-icon">📊</span>
          Benchmarks
        </button>
        <button
          class="sidebar-item"
          :class="{ active: mainTab === 'admin' }"
          @click="mainTab = 'admin'"
        >
          <span class="sidebar-item-icon">🔧</span>
          Admin
        </button>
      </div>

      <!-- Host list shown in sidebar when Admin tab is active -->
      <template v-if="mainTab === 'admin'">
        <div class="sidebar-divider"></div>
        <div class="sidebar-section-header">
          <span class="sidebar-section-label">Hosts</span>
          <button class="sidebar-new-btn" @click="openHostModal()">+ New</button>
        </div>
        <div class="sidebar-hosts">
          <div
            v-for="host in hosts"
            :key="host.id"
            class="sidebar-host-row"
          >
            <button
              class="sidebar-host-item"
              :class="{ active: host.id === configHostId }"
              @click="selectHost(host)"
            >
              <span class="sidebar-host-dot"></span>
              {{ host.name }}
            </button>
            <button
              class="sidebar-host-edit"
              @click.stop="openHostModal(host)"
              title="Edit host"
            >✎</button>
          </div>
          <p v-if="!hosts.length" class="sidebar-empty">No hosts yet.</p>
        </div>
      </template>
    </nav>

    <!-- Main content area -->
    <main class="content-area">
      <EnginesView
        v-if="mainTab === 'engines'"
        :hosts="hosts"
        :engine-count="engineCount"
        :engines="engines"
        :engines-by-host="enginesByHost"
        :engine-results-by-host="engineResultsByHost"
        :hardware-by-host="hardwareByHost"
        :hardware-errors-by-host="hardwareErrorsByHost"
        :errors="errors"
        :loading="loading"
        @open-detail="openDetailModal"
        @start-engine="(engine) => startEngine(engine, refreshAll)"
        @stop-engine="(engine) => stopEngine(engine, refreshAll)"
        @open-benchmark="openBenchmarkModal"
      />

      <BenchmarksView
        v-if="mainTab === 'benchmarks'"
        :records="benchmarkRecords"
        :errors="benchmarkErrors"
        :loading="benchmarkLoading"
        @refresh="loadBenchmarks"
      />

      <AdminView
        v-if="mainTab === 'admin'"
        :selected-host="selectedHost"
        :configs="configs"
        :images="images"
        :host-errors="hostErrors"
        :config-errors="configErrors"
        :image-errors="imageErrors"
        @open-config-modal="handleOpenConfigModal"
        @open-config-template-modal="handleOpenConfigTemplateModal"
        @open-image-modal="handleOpenImageModal"
      />
    </main>
  </div>

  <!-- Modals are position:fixed so grid placement is irrelevant -->
  <EngineDetailModal
    :show="showDetailModal"
    :engine="selected"
    :logs="logs"
    :log-history="logHistory"
    :log-sessions="logSessions"
    :selected-session-id="selectedSessionId"
    :current-session-id="currentSessionId"
    @close="closeDetailModal"
    @select-current-session="selectCurrentSession"
    @refresh-sessions="loadLogSessions(selected)"
    @update:selected-session-id="onSessionIdChange"
  />

  <HostModal
    :show="showHostModal"
    :mode="hostMode"
    :form="hostForm"
    :errors="hostErrors"
    @close="closeHostModal"
    @submit="saveHost(handleRefreshAfterHostSave)"
    @delete="deleteHostFromModal(refreshAll)"
  />

  <ConfigModal
    :show="showConfigModal"
    :mode="configMode"
    :form="configForm"
    :errors="configErrors"
    :images="images"
    :model-artifacts="modelArtifacts"
    :show-model-picker="showModelPicker"
    :model-picker-options="modelPickerOptions"
    :model-picker-title="modelPickerTitle"
    v-model:model-picker-query="modelPickerQuery"
    v-model:vllm-args-form="vllmArgsForm"
    v-model:llama-cpp-args-form="llamaCppArgsForm"
    v-model:fastllm-args-form="fastllmArgsForm"
    v-model:k-transformers-args-form="kTransformersArgsForm"
    v-model:custom-args-form="customArgsForm"
    v-model:docker-engine-form="dockerEngineForm"
    @close="closeConfigModal"
    @submit="saveConfig(configHostId, images, onConfigSaved)"
    @delete="deleteConfigFromModal(configHostId, onConfigSaved)"
    @open-model-picker="openModelPicker"
    @close-model-picker="closeModelPicker"
    @select-model="selectModelFromPicker"
  />

  <ImageModal
    :show="showImageModal"
    :mode="imageMode"
    v-model="imageForm"
    :errors="imageErrors"
    @close="closeImageModal"
    @submit="saveImage(configHostId)"
    @delete="deleteImageFromModal(configHostId)"
  />
</template>

<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import type { Host, EngineConfig, DockerImage, EngineItem } from "./types";

import EnginesView from "./views/EnginesView.vue";
import BenchmarksView from "./views/BenchmarksView.vue";
import AdminView from "./views/AdminView.vue";
import EngineDetailModal from "./components/EngineDetailModal.vue";
import HostModal from "./components/HostModal.vue";
import ConfigModal from "./components/ConfigModal.vue";
import ImageModal from "./components/ImageModal.vue";

import { useEngines } from "./composables/useEngines";
import { useHosts } from "./composables/useHosts";
import { useLogs } from "./composables/useLogs";
import { useConfigs } from "./composables/useConfigs";
import { useDockerImages } from "./composables/useDockerImages";
import { useBenchmarks } from "./composables/useBenchmarks";

// ── composables ──────────────────────────────────────────────────────────────

const {
  hosts,
  hostErrors,
  hostMode,
  hostForm,
  showHostModal,
  hardwareByHost,
  hardwareErrorsByHost,
  generateHostId,
  resetHostForm,
  openHostModal,
  closeHostModal,
  saveHost,
  deleteHostFromModal,
  fetchHostsAndHardware
} = useHosts();

const {
  engines,
  errors,
  loading,
  lastRefreshed,
  configNameByHost,
  engineResultsByHost,
  engineCount,
  enginesByHost,
  updateConfigNameMapForHost,
  startEngine,
  stopEngine,
  refreshEngines
} = useEngines();

const {
  logs,
  logHistory,
  logSessions,
  selectedSessionId,
  currentSessionId,
  connectLogs,
  disconnectLogs,
  startSessionAutoRefresh,
  stopSessionAutoRefresh,
  scheduleLogHistoryLoad,
  loadLogHistory,
  loadLogSessions,
  selectCurrentSession,
  clearLogsState
} = useLogs();

const {
  configs,
  configErrors,
  configMode,
  configForm,
  configOriginalId,
  showConfigModal,
  showModelPicker,
  modelPickerOptions,
  modelPickerTitle,
  modelPickerOnSelect,
  modelPickerQuery,
  modelArtifacts,
  vllmArgsForm,
  llamaCppArgsForm,
  fastllmArgsForm,
  kTransformersArgsForm,
  customArgsForm,
  dockerEngineForm,
  resetConfigForm,
  openConfigModal,
  openConfigTemplateModal,
  closeConfigModal,
  openModelPicker,
  closeModelPicker,
  selectModelFromPicker,
  loadConfigs,
  loadModels,
  saveConfig,
  deleteConfigFromModal
} = useConfigs();

const {
  images,
  imageErrors,
  imageMode,
  imageForm,
  showImageModal,
  openImageModal,
  closeImageModal,
  loadImages,
  saveImage,
  deleteImageFromModal
} = useDockerImages();

const {
  benchmarkRecords,
  benchmarkErrors,
  benchmarkLoading,
  showBenchmarkModal,
  benchmarkTarget,
  benchmarkForm,
  benchmarkModalError,
  benchmarkRunning,
  openBenchmarkModal,
  closeBenchmarkModal,
  loadBenchmarks,
  runBenchmark
} = useBenchmarks();

// ── local state ──────────────────────────────────────────────────────────────

const mainTab = ref<"engines" | "benchmarks" | "admin">("engines");
const selected = ref<EngineItem | null>(null);
const showDetailModal = ref(false);
const configHostId = ref<string | null>(null);

const selectedHost = computed(() =>
  hosts.value.find((host) => host.id === configHostId.value) ?? null
);

// ── status counts (topbar chips) ─────────────────────────────────────────────

const runningCount = computed(() =>
  engines.value.filter((e) => e.instance.status === "Running").length
);
const stoppedCount = computed(() =>
  engines.value.filter((e) => e.instance.status === "Stopped").length
);
const startingCount = computed(() =>
  engines.value.filter((e) => e.instance.status === "Starting").length
);

// ── watches ──────────────────────────────────────────────────────────────────

watch(
  () => mainTab.value,
  (next) => {
    if (next === "benchmarks") {
      loadBenchmarks();
    }
  }
);

watch(
  () => selectedSessionId.value,
  () => {
    scheduleLogHistoryLoad(() => loadLogHistory(selected.value));
  }
);

// ── engine detail modal ───────────────────────────────────────────────────────

function openDetailModal(engine: EngineItem) {
  selected.value = engine;
  showDetailModal.value = true;
  connectLogs(engine);
  void loadLogSessions(engine);
  startSessionAutoRefresh(
    () => showDetailModal.value,
    () => loadLogSessions(selected.value),
    () => loadLogHistory(selected.value)
  );
}

function closeDetailModal() {
  showDetailModal.value = false;
  clearLogsState();
  stopSessionAutoRefresh();
  disconnectLogs();
}

function onSessionIdChange(value: string | null) {
  selectedSessionId.value = value;
}

// ── admin: host selection ─────────────────────────────────────────────────────

function selectHost(host: Host) {
  if (configHostId.value === host.id) {
    return;
  }
  configHostId.value = host.id;
  loadConfigs(host.id);
  loadModels(host.id);
  loadImages(host.id);
}

// ── admin: config modal wrappers ──────────────────────────────────────────────

function handleOpenConfigModal(config?: EngineConfig) {
  const opened = openConfigModal(config, configHostId.value);
  if (opened) {
    loadModels(configHostId.value);
    loadImages(configHostId.value);
  }
}

function handleOpenConfigTemplateModal(config: EngineConfig) {
  const opened = openConfigTemplateModal(config, configHostId.value);
  if (opened) {
    loadModels(configHostId.value);
    loadImages(configHostId.value);
  }
}

function onConfigSaved(nextConfigs: EngineConfig[]) {
  if (configHostId.value) {
    updateConfigNameMapForHost(configHostId.value, nextConfigs);
  }
}

// ── admin: image modal wrappers ───────────────────────────────────────────────

function handleOpenImageModal(image?: DockerImage) {
  openImageModal(image, configHostId.value);
}

// ── refresh all ───────────────────────────────────────────────────────────────

async function refreshAll() {
  loading.value = true;
  errors.value = [];
  try {
    const hostsRes = await fetch("/api/hosts");
    if (!hostsRes.ok) {
      errors.value = [`Failed to load hosts (HTTP ${hostsRes.status})`];
      hosts.value = [];
      engines.value = [];
      engineResultsByHost.value = {};
      return;
    }
    const hostsBody = (await hostsRes.json()) as { hosts: Host[] };
    hosts.value = hostsBody.hosts ?? [];
    if (hosts.value.length && hostMode.value === "create" && !hostForm.value.id) {
      resetHostForm();
    }

    // Load config names and hardware for each host in parallel.
    const nextConfigNameByHost: Record<string, Record<string, string>> = {};
    await Promise.all(
      hosts.value.map(async (host) => {
        try {
          const res = await fetch(`/api/hosts/${host.id}/configs`);
          if (!res.ok) {
            nextConfigNameByHost[host.id] = {};
            return;
          }
          const body = (await res.json()) as { configs: EngineConfig[] };
          const map: Record<string, string> = {};
          for (const config of body.configs ?? []) {
            map[config.id] = config.name;
          }
          nextConfigNameByHost[host.id] = map;
        } catch {
          // Ignore config load failures here; engines list can still render.
          nextConfigNameByHost[host.id] = {};
        }

        try {
          const res = await fetch(`/api/hosts/${host.id}/hardware`);
          if (!res.ok) {
            hardwareErrorsByHost.value[host.id] = `Hardware unavailable (HTTP ${res.status}).`;
            hardwareByHost.value[host.id] = null;
            return;
          }
          const body = (await res.json()) as { hardware?: import("./types").HardwareInfo };
          hardwareByHost.value[host.id] = body.hardware ?? null;
        } catch (err) {
          hardwareErrorsByHost.value[host.id] = (err as Error).message;
          hardwareByHost.value[host.id] = null;
        }
      })
    );
    configNameByHost.value = nextConfigNameByHost;

    const ok = await refreshEngines(hosts.value, nextConfigNameByHost);
    if (!ok) {
      return;
    }

    if (
      configHostId.value &&
      !hosts.value.some((host) => host.id === configHostId.value)
    ) {
      configHostId.value = null;
    }
    if (!configHostId.value && hosts.value.length) {
      configHostId.value = hosts.value[0].id;
    }
    await loadConfigs(configHostId.value);
    if (mainTab.value === "benchmarks") {
      await loadBenchmarks();
    }
  } finally {
    loading.value = false;
  }
}

async function handleRefreshAfterHostSave() {
  await refreshAll();
  if (configHostId.value) {
    await loadModels(configHostId.value);
  }
}

// ── lifecycle ─────────────────────────────────────────────────────────────────

onMounted(refreshAll);

onBeforeUnmount(() => {
  disconnectLogs();
  stopSessionAutoRefresh();
});
</script>
