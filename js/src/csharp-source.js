export function sourceBraceDelta(line) {
  let delta = 0;
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === "/" && line[index + 1] === "/") break;
    if (character === '"' || character === "'") quote = character;
    else if (character === "{") delta += 1;
    else if (character === "}") delta -= 1;
  }
  return delta;
}

export function isContractHeaderLine(line) {
  const trimmed = line.trim();
  return (
    trimmed === "" ||
    trimmed.startsWith("//") ||
    /^(?:supported_standards|features|groups|permissions|trusts)\b/.test(trimmed) ||
    trimmed.startsWith("pubkey=") ||
    /^fn\s+.*;(?:\s*\/\/.*)?$/.test(trimmed) ||
    /^event\s+/.test(trimmed)
  );
}
