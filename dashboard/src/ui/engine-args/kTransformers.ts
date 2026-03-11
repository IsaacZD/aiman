import { buildArgs, consumeFlag } from "./shared";

export type KTransformersArgsForm = {
  modelPath: string;
  port: string;
  extraArgsText: string;
};

export function createKTransformersArgsForm(): KTransformersArgsForm {
  return {
    modelPath: "",
    port: "",
    extraArgsText: ""
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
    extraArgsText: rest.join("\n")
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

  return buildArgs(base, form.extraArgsText);
}
