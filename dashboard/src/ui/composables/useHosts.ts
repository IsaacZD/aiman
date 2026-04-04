import { ref } from "vue";
import type { Host, HardwareInfo, EngineConfig, EnginesResult } from "../types";

export function useHosts() {
  const hosts = ref<Host[]>([]);
  const hostErrors = ref<string[]>([]);
  const hostMode = ref<"create" | "edit">("create");
  const showHostModal = ref(false);
  const hardwareByHost = ref<Record<string, HardwareInfo | null>>({});
  const hardwareErrorsByHost = ref<Record<string, string>>({});
  const engineResultsByHost = ref<Record<string, EnginesResult>>({});

  function generateHostId(): string {
    if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
      return crypto.randomUUID();
    }
    return `host-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
  }

  function createEmptyHostForm() {
    return {
      id: "",
      name: "",
      base_url: "",
      api_key: "",
      model_libraries_text: ""
    };
  }

  const hostForm = ref({
    ...createEmptyHostForm(),
    id: generateHostId()
  });

  function resetHostForm() {
    hostMode.value = "create";
    hostForm.value = {
      ...createEmptyHostForm(),
      id: generateHostId()
    };
  }

  function openHostModal(host?: Host) {
    if (host) {
      editHost(host);
    } else {
      resetHostForm();
      if (!hostForm.value.id) {
        hostForm.value.id = generateHostId();
      }
    }
    showHostModal.value = true;
  }

  function closeHostModal() {
    showHostModal.value = false;
    hostErrors.value = [];
  }

  function editHost(host: Host) {
    hostMode.value = "edit";
    hostForm.value = {
      id: host.id,
      name: host.name,
      base_url: host.base_url,
      api_key: host.api_key ?? "",
      model_libraries_text: (host.model_libraries ?? []).join("\n")
    };
  }

  async function saveHost(onSuccess: () => Promise<void>) {
    hostErrors.value = [];
    let hostId = (hostForm.value.id ?? "").toString().trim();
    if (!hostId) {
      hostId = generateHostId();
      hostForm.value.id = hostId;
    }
    if (!hostForm.value.name.trim()) {
      hostErrors.value = ["Host name is required."];
      return;
    }
    if (!hostForm.value.base_url.trim()) {
      hostErrors.value = ["Base URL is required."];
      return;
    }

    const apiKey = hostForm.value.api_key.trim();
    const modelLibraries = hostForm.value.model_libraries_text
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    const payload = {
      id: hostId,
      name: hostForm.value.name.trim(),
      base_url: hostForm.value.base_url.trim(),
      ...(apiKey ? { api_key: apiKey } : {}),
      ...(modelLibraries.length ? { model_libraries: modelLibraries } : {})
    };

    const isEdit = hostMode.value === "edit" && hosts.value.some((host) => host.id === payload.id);
    const method = isEdit ? "PUT" : "POST";
    const url = isEdit ? `/api/hosts/${encodeURIComponent(payload.id)}` : "/api/hosts";

    if (isEdit && !confirm(`Save changes to host "${payload.name}"?`)) {
      return;
    }

    const res = await fetch(url, {
      method,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    if (!res.ok) {
      const body = await res.json().catch(() => null);
      if (res.status === 409 && !isEdit && payload.id) {
        if (confirm(`Host "${payload.id}" already exists. Edit it instead?`)) {
          const existing = hosts.value.find((host) => host.id === payload.id);
          if (existing) {
            openHostModal(existing);
          }
        }
        return;
      }
      hostErrors.value = [
        body?.error ? `Save failed: ${body.error}` : `Save failed (HTTP ${res.status}).`
      ];
      return;
    }

    closeHostModal();
    resetHostForm();
    await onSuccess();
  }

  async function deleteHost(host: Host, onSuccess: () => Promise<void>) {
    if (!confirm(`Delete host "${host.name}"? This cannot be undone.`)) {
      return;
    }
    const res = await fetch(`/api/hosts/${encodeURIComponent(host.id)}`, { method: "DELETE" });
    if (!res.ok) {
      hostErrors.value = [`Delete failed (HTTP ${res.status}).`];
      return;
    }
    await onSuccess();
  }

  async function deleteHostFromModal(onSuccess: () => Promise<void>) {
    const hostId = (hostForm.value.id ?? "").toString().trim();
    if (!hostId) {
      return;
    }
    await deleteHost({ id: hostId, name: hostForm.value.name } as Host, onSuccess);
    closeHostModal();
  }

  async function fetchHostsAndHardware(
    hosts_: Host[],
    getConfigNames: (hostId: string) => Promise<Record<string, string>>
  ) {
    const nextHardwareByHost: Record<string, HardwareInfo | null> = {};
    const nextHardwareErrors: Record<string, string> = {};
    await Promise.all(
      hosts_.map(async (host) => {
        try {
          const res = await fetch(`/api/hosts/${host.id}/hardware`);
          if (!res.ok) {
            nextHardwareErrors[host.id] = `Hardware unavailable (HTTP ${res.status}).`;
            nextHardwareByHost[host.id] = null;
            return;
          }
          const body = (await res.json()) as { hardware?: HardwareInfo };
          nextHardwareByHost[host.id] = body.hardware ?? null;
        } catch (err) {
          nextHardwareErrors[host.id] = (err as Error).message;
          nextHardwareByHost[host.id] = null;
        }
      })
    );
    hardwareByHost.value = nextHardwareByHost;
    hardwareErrorsByHost.value = nextHardwareErrors;
  }

  return {
    hosts,
    hostErrors,
    hostMode,
    hostForm,
    showHostModal,
    hardwareByHost,
    hardwareErrorsByHost,
    engineResultsByHost,
    generateHostId,
    resetHostForm,
    openHostModal,
    closeHostModal,
    editHost,
    saveHost,
    deleteHost,
    deleteHostFromModal,
    fetchHostsAndHardware
  };
}
