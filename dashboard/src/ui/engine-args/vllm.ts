import { buildArgs, consumeFlag } from "./shared";

export type VllmArgsForm = {
  modelPath: string;
  port: string;
  tensorParallelSize: string;
  extraArgsText: string;
};

export function createVllmArgsForm(): VllmArgsForm {
  return {
    modelPath: "",
    port: "",
    tensorParallelSize: "",
    extraArgsText: ""
  };
}

export function parseVllmArgs(args: string[]): VllmArgsForm {
  let rest = [...args];
  const model = consumeFlag(rest, "--model");
  rest = model.rest;
  const port = consumeFlag(rest, "--port");
  rest = port.rest;
  const tps = consumeFlag(rest, "--tensor-parallel-size");
  rest = tps.rest;

  return {
    modelPath: model.value,
    port: port.value,
    tensorParallelSize: tps.value,
    extraArgsText: rest.join("\n")
  };
}

export function buildVllmArgs(form: VllmArgsForm) {
  const base: string[] = [];
  if (form.modelPath.trim()) {
    base.push("--model", form.modelPath.trim());
  }
  if (form.port.trim()) {
    base.push("--port", form.port.trim());
  }
  if (form.tensorParallelSize.trim()) {
    base.push("--tensor-parallel-size", form.tensorParallelSize.trim());
  }

  return buildArgs(base, form.extraArgsText);
}
