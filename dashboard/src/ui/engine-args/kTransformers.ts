import { buildArgs, consumeFlag } from "./shared";

export type KTransformersArgsForm = {
  modelPath: string;
  port: string;
  extraArgs: string[];
};

export function createKTransformersArgsForm(): KTransformersArgsForm {
  return {
    modelPath: "",
    port: "",
    extraArgs: []
  };
}

export function parseKTransformersArgs(args: string[]): KTransformersArgsForm {
  let rest = [...args];
  const model = consumeFlag(rest, "--model");
  rest = model.rest;
  const port = consumeFlag(rest, "--port");
  rest = port.rest;

  return {
    modelPath: model.value,
    port: port.value,
    extraArgs: rest
  };
}

export function buildKTransformersArgs(form: KTransformersArgsForm) {
  const base: string[] = [];
  if (form.modelPath.trim()) {
    base.push("--model", form.modelPath.trim());
  }
  if (form.port.trim()) {
    base.push("--port", form.port.trim());
  }

  return buildArgs(base, form.extraArgs);
}
