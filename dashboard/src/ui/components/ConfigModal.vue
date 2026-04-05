<template>
  <div v-if="show" class="modal-backdrop">
    <div class="modal modal-wide">
      <div class="modal-head">
        <h3>{{ mode === "create" ? "Create config" : "Edit config" }}</h3>
        <button class="ghost" @click="$emit('close')">Close</button>
      </div>
      <div v-if="errors.length" class="alert">
        <p v-for="error in errors" :key="error">{{ error }}</p>
      </div>
      <form class="config-form" @submit.prevent="$emit('submit')">
        <label class="config-id-label">
          <span class="config-id-title">Config ID</span>
          <span class="config-id-value">
            {{ form.id || "auto-generated" }}
          </span>
        </label>
        <label>
          Display name
          <input v-model="form.name" type="text" placeholder="DeepSeek via vLLM" />
        </label>
        <label>
          Engine type
          <select v-model="form.engine_type">
            <option value="Vllm">Vllm</option>
            <option value="Lvllm">Lvllm</option>
            <option value="LlamaCpp">LlamaCpp</option>
            <option value="ik_llamacpp">ik_llamacpp</option>
            <option value="fastllm">fastllm</option>
            <option value="KTransformers">KTransformers</option>
            <option value="Container">Container</option>
            <option value="Custom">Custom</option>
          </select>
        </label>
        <label>
          {{ form.engine_type === "Container" ? "Container runtime" : "Command" }}
          <input
            v-model="form.command"
            type="text"
            :placeholder="form.engine_type === 'Container' ? 'podman' : '/opt/vllm/serve'"
          />
        </label>
        <div class="engine-fields-panel">
          <VllmConfigFields
            v-if="form.engine_type === 'Vllm' || form.engine_type === 'Lvllm'"
            v-model="vllmArgsForm"
            :model-options="vllmModelOptions"
            :open-model-picker="
              (onSelect) => $emit('open-model-picker', vllmModelOptions, 'Select vLLM model', onSelect)
            "
          />
          <LlamaCppConfigFields
            v-else-if="
              form.engine_type === 'LlamaCpp' || form.engine_type === 'ik_llamacpp'
            "
            v-model="llamaCppArgsForm"
            :model-options="ggufModelOptions"
            :open-model-picker="
              (onSelect) => $emit('open-model-picker', ggufModelOptions, 'Select GGUF model', onSelect)
            "
          />
          <FastllmConfigFields
            v-else-if="form.engine_type === 'fastllm'"
            v-model="fastllmArgsForm"
            :model-options="vllmModelOptions"
            :open-model-picker="
              (onSelect) => $emit('open-model-picker', vllmModelOptions, 'Select FastLLM model', onSelect)
            "
          />
          <KTransformersConfigFields
            v-else-if="form.engine_type === 'KTransformers'"
            v-model="kTransformersArgsForm"
            :model-options="ggufModelOptions"
            :open-model-picker="
              (onSelect) => $emit('open-model-picker', ggufModelOptions, 'Select GGUF model', onSelect)
            "
          />
          <ContainerConfigFields
            v-else-if="form.engine_type === 'Container'"
            v-model="containerEngineForm"
            :images="images"
          />
          <CustomConfigFields v-else v-model="customArgsForm" />
        </div>
        <label v-if="form.engine_type !== 'Container'">
          Environment variables
          <EnvVarListEditor v-model="form.envEntries" />
        </label>
        <label>
          Working dir
          <input v-model="form.working_dir" type="text" placeholder="/opt/engines" />
        </label>
        <div class="config-inline">
          <label>
            <input v-model="form.auto_restart_enabled" type="checkbox" />
            Auto restart
          </label>
          <label>
            Max retries
            <input v-model.number="form.auto_restart_max_retries" type="number" min="0" />
          </label>
          <label>
            Backoff (sec)
            <input v-model.number="form.auto_restart_backoff_secs" type="number" min="1" />
          </label>
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
            {{ mode === "create" ? "Create config" : "Save changes" }}
          </button>
        </div>
      </form>
    </div>
  </div>

  <div v-if="showModelPicker" class="modal-backdrop">
    <div class="modal modal-wide model-picker-modal">
      <div class="modal-head">
        <h3>{{ modelPickerTitle }}</h3>
        <button class="ghost" @click="$emit('close-model-picker')">Close</button>
      </div>
      <div class="model-picker-search">
        <input
          v-model="modelPickerQuery"
          type="text"
          placeholder="Search by name, path, kind, or library"
        />
      </div>
      <p class="panel-sub">
        {{
          filteredModelPickerOptions.length
            ? `${filteredModelPickerOptions.length} models available`
            : modelPickerOptions.length
              ? "No matches. Try another search."
              : "No models found. Check the host model library paths."
        }}
      </p>
      <div v-if="filteredModelPickerOptions.length" class="model-card-groups">
        <section v-for="group in groupedModelPickerOptions" :key="group.library">
          <div class="model-group-title">{{ group.library }}</div>
          <div v-for="kindGroup in group.kinds" :key="kindGroup.kind" class="model-kind-group">
            <div class="model-kind-title">{{ kindGroup.kind }}</div>
            <div class="model-card-grid">
              <button
                v-for="option in kindGroup.items"
                :key="option.path"
                class="model-card"
                type="button"
                @click="$emit('select-model', option.path)"
              >
                <div class="model-card-title">{{ option.label }}</div>
                <div class="model-card-path">{{ option.path }}</div>
                <div class="model-card-meta">{{ option.kind }} • {{ option.library }}</div>
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { EngineConfig, ContainerImage, ModelArtifact, EnvVar } from "../types";
import VllmConfigFields from "./VllmConfigFields.vue";
import LlamaCppConfigFields from "./LlamaCppConfigFields.vue";
import FastllmConfigFields from "./FastllmConfigFields.vue";
import KTransformersConfigFields from "./KTransformersConfigFields.vue";
import CustomConfigFields from "./CustomConfigFields.vue";
import ContainerConfigFields from "./ContainerConfigFields.vue";
import EnvVarListEditor from "./EnvVarListEditor.vue";

const props = defineProps<{
  show: boolean;
  mode: "create" | "edit";
  form: {
    id: string;
    name: string;
    engine_type: EngineConfig["engine_type"];
    command: string;
    envEntries: EnvVar[];
    working_dir: string;
    auto_restart_enabled: boolean;
    auto_restart_max_retries: number;
    auto_restart_backoff_secs: number;
  };
  errors: string[];
  images: ContainerImage[];
  modelArtifacts: ModelArtifact[];
  showModelPicker: boolean;
  modelPickerOptions: ModelArtifact[];
  modelPickerTitle: string;
}>();

const modelPickerQuery = defineModel<string>("modelPickerQuery", { required: true });

defineEmits<{
  (e: "close"): void;
  (e: "submit"): void;
  (e: "delete"): void;
  (e: "open-model-picker", options: ModelArtifact[], title: string, onSelect: (path: string) => void): void;
  (e: "close-model-picker"): void;
  (e: "select-model", path: string): void;
}>();

const vllmArgsForm = defineModel<any>("vllmArgsForm", { required: true });
const llamaCppArgsForm = defineModel<any>("llamaCppArgsForm", { required: true });
const fastllmArgsForm = defineModel<any>("fastllmArgsForm", { required: true });
const kTransformersArgsForm = defineModel<any>("kTransformersArgsForm", { required: true });
const customArgsForm = defineModel<any>("customArgsForm", { required: true });
const containerEngineForm = defineModel<any>("containerEngineForm", { required: true });

const vllmModelOptions = computed(() =>
  props.modelArtifacts.filter((artifact) => artifact.kind === "snapshot")
);
const ggufModelOptions = computed(() =>
  props.modelArtifacts.filter((artifact) => artifact.kind === "gguf")
);

const filteredModelPickerOptions = computed(() => {
  const query = (modelPickerQuery.value ?? "").trim().toLowerCase();
  if (!query) {
    return props.modelPickerOptions;
  }
  return props.modelPickerOptions.filter((option) => {
    return (
      option.label.toLowerCase().includes(query) ||
      option.path.toLowerCase().includes(query) ||
      option.id.toLowerCase().includes(query) ||
      option.library.toLowerCase().includes(query) ||
      option.kind.toLowerCase().includes(query)
    );
  });
});

const groupedModelPickerOptions = computed(() => {
  const grouped: Record<string, Record<string, ModelArtifact[]>> = {};
  for (const option of filteredModelPickerOptions.value) {
    const library = option.library || "Unknown library";
    if (!grouped[library]) {
      grouped[library] = {};
    }
    if (!grouped[library][option.kind]) {
      grouped[library][option.kind] = [];
    }
    grouped[library][option.kind].push(option);
  }
  const sortedLibraries = Object.keys(grouped).sort((a, b) => a.localeCompare(b));
  return sortedLibraries.map((library) => {
    const kinds = grouped[library];
    const sortedKinds = Object.keys(kinds).sort((a, b) => a.localeCompare(b));
    return {
      library,
      kinds: sortedKinds.map((kind) => ({
        kind,
        items: kinds[kind].sort((a, b) => a.label.localeCompare(b.label))
      }))
    };
  });
});
</script>
