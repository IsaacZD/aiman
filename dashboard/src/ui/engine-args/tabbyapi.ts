import { buildArgs, consumeFlag } from "./shared";

export type TabbyAPIArgsForm = {
  modelDir: string;
  port: string;
  gpuSplit: string;
  extraArgs: string[];
};

export function createTabbyAPIArgsForm(): TabbyAPIArgsForm {
  return {
    modelDir: "",
    port: "",
    gpuSplit: "",
    extraArgs: []
  };
}

export function parseTabbyAPIArgs(args: string[]): TabbyAPIArgsForm {
  let rest = [...args];
  const modelDir = consumeFlag(rest, "--model-dir", "-md");
  rest = modelDir.rest;
  const port = consumeFlag(rest, "--port", "-p");
  rest = port.rest;
  const gpuSplit = consumeFlag(rest, "--gpu-split", "-gs");
  rest = gpuSplit.rest;

  return {
    modelDir: modelDir.value,
    port: port.value,
    gpuSplit: gpuSplit.value,
    extraArgs: rest
  };
}

export function buildTabbyAPIArgs(form: TabbyAPIArgsForm) {
  const base: string[] = [];
  if (form.modelDir.trim()) {
    base.push("--model-dir", form.modelDir.trim());
  }
  if (form.port.trim()) {
    base.push("--port", form.port.trim());
  }
  if (form.gpuSplit.trim()) {
    base.push("--gpu-split", form.gpuSplit.trim());
  }

  return buildArgs(base, form.extraArgs);
}
