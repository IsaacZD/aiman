import { buildArgs, consumeFlag } from "./shared";

export type FastllmArgsForm = {
  modelPath: string;
  port: string;
  extraArgs: string[];
};

export function createFastllmArgsForm(): FastllmArgsForm {
  return {
    modelPath: "",
    port: "",
    extraArgs: []
  };
}

export function parseFastllmArgs(args: string[]): FastllmArgsForm {
  let rest = [...args];
  // FastLLM CLI uses a "server" subcommand, so treat it as implicit.
  if (rest[0] === "server") {
    rest = rest.slice(1);
  }

  // The model path/name is positional (first non-flag argument).
  let modelPath = "";
  const firstArg = rest[0];
  if (firstArg !== undefined && !firstArg.startsWith("-")) {
    modelPath = firstArg;
    rest = rest.slice(1);
  }

  const port = consumeFlag(rest, "--port");
  rest = port.rest;

  return {
    modelPath,
    port: port.value,
    extraArgs: rest
  };
}

export function buildFastllmArgs(form: FastllmArgsForm) {
  const base: string[] = ["server"];
  if (form.modelPath.trim()) {
    base.push(form.modelPath.trim());
  }
  if (form.port.trim()) {
    base.push("--port", form.port.trim());
  }

  return buildArgs(base, form.extraArgs);
}
