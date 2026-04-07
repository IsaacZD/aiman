<template>
  <div class="engine-fields">
    <label>
      Image ID
      <input v-model="form.id" type="text" placeholder="vllm-openai" :disabled="idLocked" />
    </label>
    <label>
      Display name
      <input v-model="form.name" type="text" placeholder="vLLM OpenAI" />
    </label>

    <!-- Image source: mutually exclusive -->
    <div class="field-group">
      <div class="field-row">
        <label class="radio">
          <input type="radio" :value="false" v-model="form.build.enabled" />
          Pull from registry
        </label>
        <label class="radio">
          <input type="radio" :value="true" v-model="form.build.enabled" />
          Build from Dockerfile
        </label>
      </div>

      <!-- Pull mode: image reference -->
      <div v-if="!form.build.enabled" class="field-group-body">
        <label>
          Image reference
          <input v-model="form.image" type="text" placeholder="ghcr.io/org/engine:tag" />
        </label>
      </div>

      <!-- Build mode: Dockerfile -->
      <div v-if="form.build.enabled" class="field-group-body">
        <label>
          Dockerfile content
          <textarea
            v-model="form.build.dockerfile_content"
            rows="6"
            placeholder="FROM nvidia/cuda:12.4.1-runtime-ubuntu22.04"
          ></textarea>
        </label>
        <div class="field-row">
          <label class="checkbox">
            <input v-model="form.build.pull" type="checkbox" />
            Pull base images
          </label>
          <label class="checkbox">
            <input v-model="form.build.no_cache" type="checkbox" />
            No build cache
          </label>
        </div>
        <label>
          Build args
          <EnvVarListEditor v-model="form.build.build_args" />
        </label>
      </div>
    </div>

    <div class="field-row">
      <label class="checkbox">
        <input v-model="form.remove" type="checkbox" />
        Remove container after stop
      </label>
    </div>
    <label>
      Ports
      <ArgumentListEditor
        v-model="form.ports"
        add-label="Add port mapping"
        placeholder="8000:8000"
      />
    </label>
    <label>
      Volumes
      <ArgumentListEditor
        v-model="form.volumes"
        add-label="Add volume"
        placeholder="/host/path:/container/path:ro"
      />
    </label>
    <label>
      Environment variables
      <EnvVarListEditor v-model="form.env" />
    </label>
    <label>
      GPU access
      <input v-model="form.gpus" type="text" placeholder="all" />
      <small style="display: block; margin-top: 4px; opacity: 0.7;">
        "all", "0", "0,1", or device ID. Leave empty for no GPU access.
      </small>
    </label>
    <label>
      Container user
      <input v-model="form.user" type="text" placeholder="1000:1000" />
    </label>
    <label>
      Container command
      <input
        v-model="form.command"
        type="text"
        placeholder="python -m vllm.entrypoints.openai.api_server"
      />
    </label>
    <label>
      Container args
      <ArgumentListEditor v-model="form.args" add-label="Add arg" placeholder="--model /models/llama" />
    </label>
  </div>
</template>

<script setup lang="ts">
import { defineModel, watch } from "vue";
import ArgumentListEditor from "./ArgumentListEditor.vue";
import EnvVarListEditor from "./EnvVarListEditor.vue";
import type { ContainerImageForm } from "../engine-args/container";

defineProps<{
  idLocked?: boolean;
}>();

const form = defineModel<ContainerImageForm>({ required: true });

// Clear the other source when toggling mode to enforce mutual exclusivity.
watch(
  () => form.value.build.enabled,
  (building) => {
    if (building) {
      form.value.image = "";
      form.value.pull = false;
    } else {
      form.value.build.dockerfile_content = "";
      form.value.build.build_args = [];
    }
  }
);
</script>
