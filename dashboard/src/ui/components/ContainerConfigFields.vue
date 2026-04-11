<template>
  <div class="engine-fields">
    <label>
      Image template
      <select v-model="form.image_id">
        <option value="">Select image template</option>
        <option v-for="image in images" :key="image.id" :value="image.id">
          {{ image.name || image.id }} — {{ image.image }}
        </option>
      </select>
    </label>
    <label>
      Container name
      <input v-model="form.container_name" type="text" placeholder="optional (defaults to config id)" />
    </label>
    <label>
      Extra ports
      <ArgumentListEditor
        v-model="form.extra_ports"
        add-label="Add port mapping"
        placeholder="8000:8000"
      />
    </label>
    <label>
      Extra volumes
      <ArgumentListEditor
        v-model="form.extra_volumes"
        add-label="Add volume"
        placeholder="/host/path:/container/path:ro"
      />
    </label>
    <label>
      Extra environment
      <EnvVarListEditor v-model="form.extra_env" />
    </label>
    <label>
      Override GPU access
      <input v-model="form.gpus" type="text" placeholder="inherit from image template" />
      <small style="display: block; margin-top: 4px; opacity: 0.7;">
        "all", "0", "0,1", or device ID. Overrides image-level setting.
      </small>
    </label>
    <label>
      Extra run args
      <ArgumentListEditor
        v-model="form.extra_run_args"
        add-label="Add runtime arg"
        placeholder="--ipc=host"
      />
      <small style="display: block; margin-top: 4px; opacity: 0.7;">
        Extra flags passed to podman create, merged after image-level run args.
      </small>
    </label>
    <label>
      Override user
      <input v-model="form.user" type="text" placeholder="inherit from image template" />
    </label>
    <label>
      Override command
      <input
        v-model="form.command"
        type="text"
        placeholder="inherit from image template"
      />
    </label>
    <label>
      Override args
      <ArgumentListEditor v-model="form.args" add-label="Add arg" placeholder="--model /models/llama" />
    </label>
    <div class="field-row">
      <label class="checkbox">
        Pull image
        <select v-model="form.pull_mode">
          <option value="inherit">Inherit</option>
          <option value="true">Always</option>
          <option value="false">Never</option>
        </select>
      </label>
      <label class="checkbox">
        Remove container
        <select v-model="form.remove_mode">
          <option value="inherit">Inherit</option>
          <option value="true">Always</option>
          <option value="false">Never</option>
        </select>
      </label>
    </div>
  </div>
</template>

<script setup lang="ts">
import { defineModel } from "vue";
import ArgumentListEditor from "./ArgumentListEditor.vue";
import EnvVarListEditor from "./EnvVarListEditor.vue";
import type { ContainerEngineForm } from "../engine-args/container";

type ImageOption = {
  id: string;
  name: string;
  image: string;
};

defineProps<{
  images: ImageOption[];
}>();

const form = defineModel<ContainerEngineForm>({ required: true });
</script>
