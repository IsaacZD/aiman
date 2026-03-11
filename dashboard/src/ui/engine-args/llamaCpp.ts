import { buildArgs, consumeFlag } from "./shared";

export type LlamaCppArgsForm = {
  modelPath: string;
  port: string;
  gpuLayers: string;
  ctxSize: string;
  extraArgsText: string;
};

export function createLlamaCppArgsForm(): LlamaCppArgsForm {
  return {
    modelPath: "",
    port: "",
    gpuLayers: "",
    ctxSize: "",
    extraArgsText: ""
  };
}

export function parseLlamaCppArgs(args: string[]): LlamaCppArgsForm {
  let rest = [...args];
  const model = consumeFlag(rest, "--model", "-m");
  rest = model.rest;
  const port = consumeFlag(rest, "--port");
  rest = port.rest;
  const gpuLayers = consumeFlag(rest, "--n-gpu-layers");
  rest = gpuLayers.rest;
  const ctxSize = consumeFlag(rest, "--ctx-size");
  rest = ctxSize.rest;

  return {
    modelPath: model.value,
    port: port.value,
    gpuLayers: gpuLayers.value,
    ctxSize: ctxSize.value,
    extraArgsText: rest.join("\n")
  };
}

export function buildLlamaCppArgs(form: LlamaCppArgsForm) {
  const base: string[] = [];
  if (form.modelPath.trim()) {
    base.push("--model", form.modelPath.trim());
  }
  if (form.port.trim()) {
    base.push("--port", form.port.trim());
  }
  if (form.gpuLayers.trim()) {
    base.push("--n-gpu-layers", form.gpuLayers.trim());
  }
  if (form.ctxSize.trim()) {
    base.push("--ctx-size", form.ctxSize.trim());
  }

  return buildArgs(base, form.extraArgsText);
}
