type ConsumeResult = {
  value: string;
  rest: string[];
};

export function splitArgsLine(raw: string) {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | "\"" | null = null;

  for (let i = 0; i < raw.length; i += 1) {
    const char = raw[i];
    if (quote) {
      if (char === quote) {
        quote = null;
        continue;
      }
      if (char === "\\" && quote === "\"" && i + 1 < raw.length) {
        current += raw[i + 1];
        i += 1;
        continue;
      }
      current += char;
      continue;
    }

    if (char === "'" || char === "\"") {
      quote = char;
      continue;
    }

    if (char === "\\" && i + 1 < raw.length) {
      current += raw[i + 1];
      i += 1;
      continue;
    }

    if (/\s/.test(char)) {
      if (current) {
        tokens.push(current);
        current = "";
      }
      continue;
    }

    current += char;
  }

  if (current) {
    tokens.push(current);
  }

  return tokens;
}

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

export function buildArgs(base: string[], extraArgs: string[]) {
  const cleaned = extraArgs
    .map((arg) => arg.trim())
    .filter(Boolean)
    .flatMap((arg) => splitArgsLine(arg));
  return [...base, ...cleaned];
}
