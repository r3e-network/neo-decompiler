import { isInsideQuotedString } from "./csharp-expression-scanner.js";
import { sourceBraceDelta } from "./csharp-source.js";

export function renderCSharpCatchClause(line, exceptionName = "exception") {
  return line.replace(
    /^(\s*\}\s*)catch\s*\{\s*$/u,
    `$1catch (Exception ${exceptionName}) {`,
  );
}

export function buildCatchScopes(lines, depths) {
  const scopes = [];
  let ordinal = 0;
  let methodParameters = new Set();
  for (let index = 0; index < lines.length; index += 1) {
    if (/^\s*fn\s+/.test(lines[index])) {
      ordinal = 0;
      methodParameters = methodParameterNames(lines[index]);
    }
    if (!/^\s*\}\s*catch\s*\{\s*$/u.test(lines[index]) &&
        !/^\s*catch\s*\{\s*$/u.test(lines[index])) {
      continue;
    }
    let variable;
    do {
      variable = ordinal === 0 ? "exception" : `exception_${ordinal}`;
      ordinal += 1;
    } while (methodParameters.has(variable));
    const bodyDepth = depths[index] + sourceBraceDelta(lines[index]);
    let endLine = lines.length;
    for (let cursor = index + 1; cursor < lines.length; cursor += 1) {
      const trimmed = lines[cursor].trim();
      if (depths[cursor] < bodyDepth ||
          (depths[cursor] === bodyDepth && /^\}\s*(?:catch|finally|else)\s*\{/u.test(trimmed))) {
        endLine = cursor;
        break;
      }
    }
    scopes.push({ headerLine: index, startLine: index + 1, endLine, variable });
  }
  return scopes;
}

function methodParameterNames(signature) {
  const parameters = signature.match(/^\s*fn\s+[^\s(]+\((.*?)\)/)?.[1] ?? "";
  return new Set(parameters.split(",").map((parameter) => {
    const separator = parameter.indexOf(":");
    return (separator < 0 ? parameter : parameter.slice(0, separator)).trim();
  }).filter(Boolean));
}

export function replaceExceptionReference(line, variable) {
  if (variable === "exception" || line.trim().startsWith("//")) return line;
  const pattern = /\bexception\b/gu;
  let cursor = 0;
  let output = "";
  let match;
  while ((match = pattern.exec(line)) !== null) {
    if (isInsideQuotedString(line, match.index)) continue;
    output += line.slice(cursor, match.index) + variable;
    cursor = match.index + match[0].length;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}
