import { splitArgsLine } from "./shared";

export type CustomArgsForm = {
  args: string[];
};

export function createCustomArgsForm(): CustomArgsForm {
  return { args: [] };
}

export function parseCustomArgs(args: string[]): CustomArgsForm {
  return { args: [...args] };
}

export function buildCustomArgs(form: CustomArgsForm) {
  return form.args
    .map((arg) => arg.trim())
    .filter(Boolean)
    .flatMap((arg) => splitArgsLine(arg));
}
