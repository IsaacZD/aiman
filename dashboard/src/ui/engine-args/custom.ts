import { parseArgsLines } from "./shared";

export type CustomArgsForm = {
  argsText: string;
};

export function createCustomArgsForm(): CustomArgsForm {
  return { argsText: "" };
}

export function parseCustomArgs(args: string[]): CustomArgsForm {
  return { argsText: args.join("\n") };
}

export function buildCustomArgs(form: CustomArgsForm) {
  return parseArgsLines(form.argsText);
}
