export function parseArgsLines(raw: string) {
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

type ConsumeResult = {
  value: string;
  rest: string[];
};

export function consumeFlag(args: string[], flag: string, shortFlag?: string): ConsumeResult {
  const next = [...args];
  const flags = [flag, shortFlag].filter(Boolean) as string[];

  for (const f of flags) {
    const eqPrefix = `${f}=`;
    const index = next.findIndex((arg) => arg === f || arg.startsWith(eqPrefix));
    if (index === -1) {
      continue;
    }

    const arg = next[index];
    if (arg.startsWith(eqPrefix)) {
      next.splice(index, 1);
      return { value: arg.slice(eqPrefix.length), rest: next };
    }

    const value = next[index + 1] ?? "";
    next.splice(index, value ? 2 : 1);
    return { value, rest: next };
  }

  return { value: "", rest: next };
}

export function buildArgs(base: string[], extraArgsText: string) {
  return [...base, ...parseArgsLines(extraArgsText)];
}
