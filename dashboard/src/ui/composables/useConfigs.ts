import { ref, watch } from "vue";
import type { EngineConfig, EnvVar, ModelArtifact, DockerImage } from "../types";
import {
  buildVllmArgs,
  createVllmArgsForm,
  parseVllmArgs
} from "../engine-args/vllm";
import {
  buildLlamaCppArgs,
  createLlamaCppArgsForm,
  parseLlamaCppArgs
} from "../engine-args/llamaCpp";
import {
  buildFastllmArgs,
  createFastllmArgsForm,
  parseFastllmArgs
} from "../engine-args/fastllm";
import {
  buildKTransformersArgs,
  createKTransformersArgsForm,
  parseKTransformersArgs
} from "../engine-args/kTransformers";
import {
  buildCustomArgs,
  createCustomArgsForm,
  parseCustomArgs
} from "../engine-args/custom";
import { createDockerEngineForm } from "../engine-args/docker";

// Defaults keep the config form helpful without forcing a full command line.
export const defaultCommands: Record<EngineConfig["engine_type"], string> = {
  Vllm: "vllm serve",
  Lvllm: "lvllm serve",
  LlamaCpp: "llama-server",
  ik_llamacpp: "ikllama-server",
  fastllm: "ftllm serve",
  KTransformers: "ktransformers-server",
  Custom: "",
  Docker: "docker"
};

export function createEmptyConfigForm() {
  return {
    id: "",
    name: "",
    engine_type: "Vllm" as EngineConfig["engine_type"],
    command: "",
    envEntries: [] as EnvVar[],
    working_dir: "",
    auto_restart_enabled: false,
    auto_restart_max_retries: 0,
    auto_restart_backoff_secs: 5
  };
}

function generateConfigId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `cfg-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function buildEnvEntries(entries: EnvVar[], errors: string[]): EnvVar[] {
  const cleaned: EnvVar[] = [];
  for (const entry of entries) {
    const key = entry.key.trim();
    const value = entry.value ?? "";
    if (!key) {
      if (value.trim()) {
        errors.push("Env var values must include a key.");
      }
      continue;
    }
    cleaned.push({ key, value });
  }
  return cleaned;
}

export function cleanStringList(entries: string[]): string[] {
  return entries.map((entry) => entry.trim()).filter(Boolean);
}

export function parseOverride(value: "inherit" | "true" | "false"): boolean | null {
  if (value === "true") {
    return true;
  }
  if (value === "false") {
    return false;
  }
  return null;
}

export function useConfigs() {
  const configs = ref<EngineConfig[]>([]);
  const configErrors = ref<string[]>([]);
  const configMode = ref<"create" | "edit">("create");
  const configForm = ref(createEmptyConfigForm());
  const configOriginalId = ref<string | null>(null);
  const showConfigModal = ref(false);
  const showModelPicker = ref(false);
  const modelPickerOptions = ref<ModelArtifact[]>([]);
  const modelPickerTitle = ref("Select model");
  const modelPickerOnSelect = ref<((path: string) => void) | null>(null);
  const modelPickerQuery = ref("");
  const modelArtifacts = ref<ModelArtifact[]>([]);

  const vllmArgsForm = ref(createVllmArgsForm());
  const llamaCppArgsForm = ref(createLlamaCppArgsForm());
  const fastllmArgsForm = ref(createFastllmArgsForm());
  const kTransformersArgsForm = ref(createKTransformersArgsForm());
  const customArgsForm = ref(createCustomArgsForm());
  const dockerEngineForm = ref(createDockerEngineForm());

  const lastEngineType = ref<EngineConfig["engine_type"]>(configForm.value.engine_type);
  watch(
    () => configForm.value.engine_type,
    (next) => {
      const previous = lastEngineType.value;
      const previousDefault = defaultCommands[previous];
      const nextDefault = defaultCommands[next];
      if (!configForm.value.command.trim() || configForm.value.command === previousDefault) {
        configForm.value.command = nextDefault;
      }
      lastEngineType.value = next;
    }
  );

  function resetConfigForm() {
    configMode.value = "create";
    configForm.value = {
      ...createEmptyConfigForm(),
      id: generateConfigId()
    };
    configOriginalId.value = null;
    // Keep arg forms in sync with engine templates so switching types is cheap.
    vllmArgsForm.value = createVllmArgsForm();
    llamaCppArgsForm.value = createLlamaCppArgsForm();
    fastllmArgsForm.value = createFastllmArgsForm();
    kTransformersArgsForm.value = createKTransformersArgsForm();
    customArgsForm.value = createCustomArgsForm();
    dockerEngineForm.value = createDockerEngineForm();
  }

  function editConfig(config: EngineConfig) {
    configMode.value = "edit";
    configOriginalId.value = config.id;
    configForm.value = {
      id: config.id,
      name: config.name,
      engine_type: config.engine_type,
      command: config.command,
      envEntries:
        config.engine_type === "Docker"
          ? []
          : config.env.map((item) => ({ key: item.key, value: item.value })),
      working_dir: config.working_dir ?? "",
      auto_restart_enabled: config.auto_restart.enabled,
      auto_restart_max_retries: config.auto_restart.max_retries,
      auto_restart_backoff_secs: config.auto_restart.backoff_secs
    };
    dockerEngineForm.value = createDockerEngineForm();
    // Parse args into the right template so the form mirrors existing configs.
    if (config.engine_type === "Vllm" || config.engine_type === "Lvllm") {
      vllmArgsForm.value = parseVllmArgs(config.args ?? []);
    } else if (config.engine_type === "LlamaCpp" || config.engine_type === "ik_llamacpp") {
      llamaCppArgsForm.value = parseLlamaCppArgs(config.args ?? []);
    } else if (config.engine_type === "fastllm") {
      fastllmArgsForm.value = parseFastllmArgs(config.args ?? []);
    } else if (config.engine_type === "KTransformers") {
      kTransformersArgsForm.value = parseKTransformersArgs(config.args ?? []);
    } else if (config.engine_type === "Docker") {
      const docker = config.docker ?? null;
      dockerEngineForm.value = {
        image_id: docker?.image_id ?? "",
        container_name: docker?.container_name ?? "",
        extra_ports: docker?.extra_ports ? [...docker.extra_ports] : [],
        extra_volumes: docker?.extra_volumes ? [...docker.extra_volumes] : [],
        extra_env: docker?.extra_env
          ? [...docker.extra_env, ...config.env]
          : [...config.env],
        extra_run_args: docker?.extra_run_args ? [...docker.extra_run_args] : [],
        workdir: docker?.workdir ?? "",
        user: docker?.user ?? "",
        command: docker?.command ?? "",
        args: docker?.args ? [...docker.args] : [],
        pull_mode: docker?.pull === true ? "true" : docker?.pull === false ? "false" : "inherit",
        remove_mode:
          docker?.remove === true ? "true" : docker?.remove === false ? "false" : "inherit"
      };
    } else {
      customArgsForm.value = parseCustomArgs(config.args ?? []);
    }
  }

  function openConfigModal(config?: EngineConfig, configHostId?: string | null) {
    if (!configHostId) {
      configErrors.value = ["Select a host before creating a config."];
      return false;
    }
    if (config && typeof config === "object" && "id" in config) {
      editConfig(config);
    } else {
      resetConfigForm();
    }
    if (!configForm.value.command.trim()) {
      configForm.value.command = defaultCommands[configForm.value.engine_type];
    }
    showConfigModal.value = true;
    return true;
  }

  function openConfigTemplateModal(config: EngineConfig, configHostId?: string | null) {
    if (!configHostId) {
      configErrors.value = ["Select a host before creating a config."];
      return false;
    }
    configErrors.value = [];
    editConfig(config);
    configMode.value = "create";
    configOriginalId.value = null;
    configForm.value = {
      ...configForm.value,
      id: generateConfigId()
    };
    if (!configForm.value.command.trim()) {
      configForm.value.command = defaultCommands[configForm.value.engine_type];
    }
    showConfigModal.value = true;
    return true;
  }

  function closeConfigModal() {
    showConfigModal.value = false;
    configErrors.value = [];
    closeModelPicker();
  }

  function openModelPicker(
    options: ModelArtifact[],
    title: string,
    onSelect: (path: string) => void
  ) {
    modelPickerOptions.value = options;
    modelPickerTitle.value = title;
    modelPickerOnSelect.value = onSelect;
    modelPickerQuery.value = "";
    showModelPicker.value = true;
  }

  function closeModelPicker() {
    showModelPicker.value = false;
    modelPickerOnSelect.value = null;
  }

  function selectModelFromPicker(path: string) {
    modelPickerOnSelect.value?.(path);
    closeModelPicker();
  }

  async function loadConfigs(configHostId: string | null): Promise<EngineConfig[]> {
    configErrors.value = [];
    if (!configHostId) {
      configs.value = [];
      return [];
    }

    try {
      const res = await fetch(`/api/hosts/${configHostId}/configs`);
      if (!res.ok) {
        configErrors.value = [`Failed to load configs (HTTP ${res.status})`];
        configs.value = [];
        return [];
      }
      const body = (await res.json()) as { configs: EngineConfig[] };
      configs.value = body.configs ?? [];
      return configs.value;
    } catch (err) {
      configErrors.value = [(err as Error).message];
      return [];
    }
  }

  async function loadModels(configHostId: string | null) {
    if (!configHostId) {
      modelArtifacts.value = [];
      return;
    }
    try {
      const res = await fetch(`/api/hosts/${configHostId}/models`);
      if (!res.ok) {
        modelArtifacts.value = [];
        return;
      }
      const body = (await res.json()) as { artifacts: ModelArtifact[] };
      modelArtifacts.value = body.artifacts ?? [];
    } catch {
      modelArtifacts.value = [];
    }
  }

  async function saveConfig(
    configHostId: string | null,
    images: DockerImage[],
    onSuccess: (nextConfigs: EngineConfig[]) => void
  ) {
    configErrors.value = [];
    if (!configHostId) {
      configErrors.value = ["Select a host before saving."];
      return;
    }

    const errors: string[] = [];
    if (!configForm.value.id.trim()) {
      errors.push("Config ID is required.");
    }
    if (!configForm.value.name.trim()) {
      errors.push("Display name is required.");
    }
    const isDocker = configForm.value.engine_type === "Docker";
    if (!isDocker && !configForm.value.command.trim()) {
      errors.push("Command is required.");
    }
    if (isDocker && !dockerEngineForm.value.image_id.trim()) {
      errors.push("Docker image template is required.");
    }

    const envEntries = isDocker ? [] : buildEnvEntries(configForm.value.envEntries, errors);
    const extraEnv = isDocker ? buildEnvEntries(dockerEngineForm.value.extra_env, errors) : [];
    if (errors.length) {
      configErrors.value = errors;
      return;
    }

    let args: string[] = [];
    // Build args from the template-specific form, keeping unknown flags in extra args.
    if (configForm.value.engine_type === "Vllm" || configForm.value.engine_type === "Lvllm") {
      args = buildVllmArgs(vllmArgsForm.value);
    } else if (
      configForm.value.engine_type === "LlamaCpp" ||
      configForm.value.engine_type === "ik_llamacpp"
    ) {
      args = buildLlamaCppArgs(llamaCppArgsForm.value);
    } else if (configForm.value.engine_type === "fastllm") {
      args = buildFastllmArgs(fastllmArgsForm.value);
    } else if (configForm.value.engine_type === "KTransformers") {
      args = buildKTransformersArgs(kTransformersArgsForm.value);
    } else if (configForm.value.engine_type === "Docker") {
      args = [];
    } else {
      args = buildCustomArgs(customArgsForm.value);
    }

    const runtimeCommand = configForm.value.command.trim() || (isDocker ? "docker" : "");
    const dockerConfig = isDocker
      ? {
          image_id: dockerEngineForm.value.image_id.trim(),
          container_name: dockerEngineForm.value.container_name.trim() || null,
          extra_ports: cleanStringList(dockerEngineForm.value.extra_ports),
          extra_volumes: cleanStringList(dockerEngineForm.value.extra_volumes),
          extra_env: extraEnv,
          extra_run_args: cleanStringList(dockerEngineForm.value.extra_run_args),
          workdir: dockerEngineForm.value.workdir.trim() || null,
          user: dockerEngineForm.value.user.trim() || null,
          command: dockerEngineForm.value.command.trim() || null,
          args: cleanStringList(dockerEngineForm.value.args),
          pull: parseOverride(dockerEngineForm.value.pull_mode),
          remove: parseOverride(dockerEngineForm.value.remove_mode)
        }
      : null;

    const config: EngineConfig = {
      id: configForm.value.id.trim(),
      name: configForm.value.name.trim(),
      engine_type: configForm.value.engine_type,
      command: runtimeCommand,
      args,
      env: envEntries,
      working_dir: configForm.value.working_dir.trim()
        ? configForm.value.working_dir.trim()
        : null,
      auto_restart: {
        enabled: configForm.value.auto_restart_enabled,
        max_retries: Number(configForm.value.auto_restart_max_retries) || 0,
        backoff_secs: Number(configForm.value.auto_restart_backoff_secs) || 5
      },
      ...(dockerConfig ? { docker: dockerConfig } : {})
    };

    const actionLabel = configMode.value === "create" ? "Create config" : "Save changes to config";
    if (!confirm(`${actionLabel} "${config.name}"?`)) {
      return;
    }

    const method = configMode.value === "create" ? "POST" : "PUT";
    const targetId = configMode.value === "edit" ? configOriginalId.value : null;
    if (configMode.value === "edit" && !targetId) {
      configErrors.value = ["Original Config ID is missing; reload the page and try again."];
      return;
    }
    const isRename =
      configMode.value === "edit" && targetId !== null && targetId !== config.id;

    const url =
      configMode.value === "create"
        ? `/api/hosts/${configHostId}/configs`
        : `/api/hosts/${configHostId}/configs/${encodeURIComponent(targetId!)}`;

    const parseError = async (res: Response) => {
      const rawBody = await res.text().catch(() => "");
      if (!rawBody) {
        return `Save failed (HTTP ${res.status}).`;
      }
      try {
        const parsed = JSON.parse(rawBody) as { error?: string; message?: string };
        return `Save failed: ${parsed.error ?? parsed.message ?? rawBody}`;
      } catch {
        return `Save failed: ${rawBody}`;
      }
    };

    if (isRename) {
      const createRes = await fetch(`/api/hosts/${configHostId}/configs`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config)
      });

      if (!createRes.ok) {
        configErrors.value = [await parseError(createRes)];
        return;
      }

      const deleteRes = await fetch(
        `/api/hosts/${configHostId}/configs/${encodeURIComponent(targetId!)}`,
        { method: "DELETE" }
      );

      if (!deleteRes.ok) {
        const suffix = `Delete failed (HTTP ${deleteRes.status}).`;
        configErrors.value = [
          `Rename partially succeeded: new config created, but old config was not deleted. ${suffix}`
        ];
        const nextConfigs = await loadConfigs(configHostId);
        onSuccess(nextConfigs);
        return;
      }

      closeConfigModal();
      resetConfigForm();
      const nextConfigs = await loadConfigs(configHostId);
      onSuccess(nextConfigs);
      return;
    }

    const res = await fetch(url, {
      method,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(config)
    });

    if (!res.ok) {
      configErrors.value = [await parseError(res)];
      return;
    }

    closeConfigModal();
    resetConfigForm();
    const nextConfigs = await loadConfigs(configHostId);
    onSuccess(nextConfigs);
  }

  async function deleteConfig(
    config: EngineConfig,
    configHostId: string | null,
    onSuccess: (nextConfigs: EngineConfig[]) => void
  ) {
    if (!configHostId) {
      return;
    }
    if (!confirm(`Delete config "${config.name}"? This cannot be undone.`)) {
      return;
    }
    const res = await fetch(
      `/api/hosts/${configHostId}/configs/${encodeURIComponent(config.id)}`,
      { method: "DELETE" }
    );
    if (!res.ok) {
      configErrors.value = [`Delete failed (HTTP ${res.status}).`];
      return;
    }
    const nextConfigs = await loadConfigs(configHostId);
    onSuccess(nextConfigs);
  }

  async function deleteConfigFromModal(
    configHostId: string | null,
    onSuccess: (nextConfigs: EngineConfig[]) => void
  ) {
    if (!configForm.value.id.trim()) {
      return;
    }
    await deleteConfig(
      { id: configForm.value.id, name: configForm.value.name } as EngineConfig,
      configHostId,
      onSuccess
    );
    closeConfigModal();
  }

  return {
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
    editConfig,
    openConfigModal,
    openConfigTemplateModal,
    closeConfigModal,
    openModelPicker,
    closeModelPicker,
    selectModelFromPicker,
    loadConfigs,
    loadModels,
    saveConfig,
    deleteConfig,
    deleteConfigFromModal
  };
}
