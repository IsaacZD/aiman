import { ref } from "vue";
import type { DockerImage, EnvVar } from "../types";
import { createDockerImageForm } from "../engine-args/docker";
import { buildEnvEntries, cleanStringList } from "./useConfigs";

function generateImageId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `img-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function useDockerImages() {
  const images = ref<DockerImage[]>([]);
  const imageErrors = ref<string[]>([]);
  const imageMode = ref<"create" | "edit">("create");
  const imageForm = ref(createDockerImageForm());
  const imageOriginalId = ref<string | null>(null);
  const showImageModal = ref(false);

  function resetImageForm() {
    imageMode.value = "create";
    imageForm.value = {
      ...createDockerImageForm(),
      id: generateImageId()
    };
    imageOriginalId.value = null;
  }

  function openImageModal(image?: DockerImage, configHostId?: string | null) {
    if (!configHostId) {
      imageErrors.value = ["Select a host before creating an image."];
      return;
    }
    if (image) {
      editImage(image);
    } else {
      resetImageForm();
    }
    showImageModal.value = true;
  }

  function closeImageModal() {
    showImageModal.value = false;
    imageErrors.value = [];
  }

  function editImage(image: DockerImage) {
    imageMode.value = "edit";
    imageOriginalId.value = image.id;
    imageForm.value = {
      id: image.id,
      name: image.name ?? "",
      image: image.image ?? "",
      ports: image.ports ? [...image.ports] : [],
      volumes: image.volumes ? [...image.volumes] : [],
      env: image.env ? [...image.env] : [],
      run_args: image.run_args ? [...image.run_args] : [],
      gpus: image.gpus ?? "",
      user: image.user ?? "",
      command: image.command ?? "",
      args: image.args ? [...image.args] : [],
      pull: Boolean(image.pull),
      remove: image.remove !== false,
      build: {
        enabled: Boolean(image.build),
        dockerfile_content: image.build?.dockerfile_content ?? "",
        pull: Boolean(image.build?.pull),
        no_cache: Boolean(image.build?.no_cache),
        build_args: image.build?.build_args ? [...image.build.build_args] : []
      }
    };
  }

  async function loadImages(configHostId: string | null): Promise<DockerImage[]> {
    imageErrors.value = [];
    if (!configHostId) {
      images.value = [];
      return [];
    }

    try {
      const res = await fetch(`/api/hosts/${configHostId}/images`);
      if (!res.ok) {
        imageErrors.value = [`Failed to load images (HTTP ${res.status})`];
        images.value = [];
        return [];
      }
      const body = (await res.json()) as { images: DockerImage[] };
      const next = body.images ?? [];
      next.sort((a, b) => (a.name || a.id).localeCompare(b.name || b.id));
      images.value = next;
      return images.value;
    } catch (err) {
      imageErrors.value = [(err as Error).message];
      return [];
    }
  }

  async function saveImage(configHostId: string | null) {
    imageErrors.value = [];
    if (!configHostId) {
      imageErrors.value = ["Select a host before saving."];
      return;
    }

    const errors: string[] = [];
    const imageId = imageForm.value.id.trim();
    if (!imageId) {
      errors.push("Image ID is required.");
    }
    if (!imageForm.value.name.trim()) {
      errors.push("Display name is required.");
    }
    if (!imageForm.value.image.trim()) {
      errors.push("Image reference is required.");
    }
    if (imageForm.value.build.enabled && !imageForm.value.build.dockerfile_content.trim()) {
      errors.push("Dockerfile content is required.");
    }

    const envEntries = buildEnvEntries(imageForm.value.env, errors);
    const buildArgs = imageForm.value.build.enabled
      ? buildEnvEntries(imageForm.value.build.build_args, errors)
      : [];
    if (errors.length) {
      imageErrors.value = errors;
      return;
    }

    if (
      imageMode.value === "edit" &&
      imageOriginalId.value &&
      imageOriginalId.value !== imageId
    ) {
      imageErrors.value = ["Image ID cannot be changed."];
      return;
    }

    const payload: DockerImage = {
      id: imageId,
      name: imageForm.value.name.trim(),
      image: imageForm.value.image.trim(),
      ports: cleanStringList(imageForm.value.ports),
      volumes: cleanStringList(imageForm.value.volumes),
      env: envEntries,
      run_args: cleanStringList(imageForm.value.run_args),
      gpus: imageForm.value.gpus.trim() || null,
      user: imageForm.value.user.trim() || null,
      command: imageForm.value.command.trim() || null,
      args: cleanStringList(imageForm.value.args),
      pull: Boolean(imageForm.value.pull),
      remove: Boolean(imageForm.value.remove),
      build: imageForm.value.build.enabled
        ? {
            dockerfile_content: imageForm.value.build.dockerfile_content.trim() || null,
            build_args: buildArgs,
            pull: Boolean(imageForm.value.build.pull),
            no_cache: Boolean(imageForm.value.build.no_cache)
          }
        : null
    };

    const actionLabel = imageMode.value === "create" ? "Create image" : "Save changes to image";
    if (!confirm(`${actionLabel} "${payload.name}"?`)) {
      return;
    }

    const method = imageMode.value === "create" ? "POST" : "PUT";
    const url =
      imageMode.value === "create"
        ? `/api/hosts/${configHostId}/images`
        : `/api/hosts/${configHostId}/images/${encodeURIComponent(imageId)}`;

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

    const res = await fetch(url, {
      method,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload)
    });

    if (!res.ok) {
      imageErrors.value = [await parseError(res)];
      return;
    }

    closeImageModal();
    resetImageForm();
    await loadImages(configHostId);
  }

  async function deleteImage(image: DockerImage, configHostId: string | null) {
    if (!configHostId) {
      return;
    }
    if (!confirm(`Delete image "${image.name}"? This cannot be undone.`)) {
      return;
    }
    const res = await fetch(
      `/api/hosts/${configHostId}/images/${encodeURIComponent(image.id)}`,
      { method: "DELETE" }
    );
    if (!res.ok) {
      imageErrors.value = [`Delete failed (HTTP ${res.status}).`];
      return;
    }
    await loadImages(configHostId);
  }

  async function pruneImages(configHostId: string | null): Promise<void> {
    if (!configHostId) return;
    imageErrors.value = [];
    const res = await fetch(`/api/hosts/${configHostId}/images/prune`, { method: "POST" });
    if (!res.ok) {
      imageErrors.value = [`Prune failed (HTTP ${res.status}).`];
      return;
    }
    const body = (await res.json()) as { removed: string[] };
    if (body.removed.length === 0) {
      imageErrors.value = ["No unused images found."];
    }
    await loadImages(configHostId);
  }

  async function deleteImageFromModal(configHostId: string | null) {
    if (!imageForm.value.id.trim()) {
      return;
    }
    await deleteImage(
      { id: imageForm.value.id, name: imageForm.value.name } as DockerImage,
      configHostId
    );
    closeImageModal();
  }

  return {
    images,
    imageErrors,
    imageMode,
    imageForm,
    imageOriginalId,
    showImageModal,
    resetImageForm,
    openImageModal,
    closeImageModal,
    editImage,
    loadImages,
    saveImage,
    deleteImage,
    deleteImageFromModal,
    pruneImages
  };
}
