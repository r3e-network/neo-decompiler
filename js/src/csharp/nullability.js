function splitParameters(parameters) {
  const parts = [];
  let start = 0;
  let angleDepth = 0;
  for (let index = 0; index < parameters.length; index += 1) {
    const character = parameters[index];
    if (character === "<") angleDepth += 1;
    if (character === ">") angleDepth = Math.max(0, angleDepth - 1);
    if (character === "," && angleDepth === 0) {
      parts.push(parameters.slice(start, index).trim());
      start = index + 1;
    }
  }
  const tail = parameters.slice(start).trim();
  if (tail) parts.push(tail);
  return parts;
}

function escapeRegex(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Find manifest parameters whose direct aliases are tested for null. */
export function nullableParametersForMethod(lines, signatureIndex) {
  const signature = lines[signatureIndex].match(/^\s*fn\s+[^\(]+\((.*?)\)/);
  if (!signature) return new Set();
  const parameterNames = new Set(
    splitParameters(signature[1])
      .map((parameter) => parameter.split(":", 1)[0].trim())
      .filter(Boolean),
  );
  const aliases = new Map();
  const nullable = new Set();
  let depth = (lines[signatureIndex].match(/\{/g) ?? []).length;
  for (let index = signatureIndex + 1; index < lines.length && depth > 0; index++) {
    const line = lines[index];
    const assignment = line.match(
      /\b(?:let\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([A-Za-z_][A-Za-z0-9_]*)\s*;/,
    );
    if (assignment && parameterNames.has(assignment[2])) {
      aliases.set(assignment[1], assignment[2]);
    }
    for (const [alias, parameter] of aliases) {
      if (containsNullCheck(line, alias)) {
        nullable.add(parameter);
      }
    }
    for (const parameter of parameterNames) {
      if (containsNullCheck(line, parameter)) {
        nullable.add(parameter);
      }
    }
    depth += (line.match(/\{/g) ?? []).length;
    depth -= (line.match(/\}/g) ?? []).length;
  }
  return nullable;
}

function containsNullCheck(line, identifier) {
  const escaped = escapeRegex(identifier);
  return new RegExp(
    `(?:\\b${escaped}\\s+is\\s+null\\b|\\bis_null\\s*\\(\\s*${escaped}\\s*\\))`,
  ).test(line);
}
