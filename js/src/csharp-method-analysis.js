import { csharpType } from "./csharp-type-map.js";
import { inferDeclarationTypes } from "./csharp-types.js";

// High-level lifting may recover a non-void method's body without a source
// `return` at the method boundary. This is common when a VM path terminates
// in ABORT/THROW or when a try/branch target could not be structured. Keep the
// generated C# valid while making the uncertainty explicit and fail-closed.
export function analyzeNonVoidMethods(lines, depths) {
  const guards = new Set();
  const bodyLines = new Set();
  for (let start = 0; start < lines.length; start += 1) {
    const header = lines[start].match(
      /^\s*fn\s+[A-Za-z_][A-Za-z0-9_]*\(.*\)(?:\s*->\s*([^\s{]+))?\s*\{\s*$/,
    );
    if (!header || String(header[1] ?? "void").toLowerCase() === "void") continue;

    const methodDepth = depths[start];
    const end = findMethodEnd(lines, depths, start, methodDepth);
    if (end < 0) continue;

    for (let index = start + 1; index < end; index += 1) bodyLines.add(index);

    let lastTopLevelStatement = null;
    for (let index = start + 1; index < end; index += 1) {
      if (depths[index] !== methodDepth + 1) continue;
      const trimmed = lines[index].trim();
      if (!trimmed || trimmed.startsWith("//")) continue;
      lastTopLevelStatement = trimmed;
    }
    if (!isTerminalHighLevelStatement(lastTopLevelStatement)) guards.add(end);
    start = end;
  }
  return { guards, bodyLines };
}

export function findMethodEnd(lines, depths, start, methodDepth = depths[start]) {
  for (let index = start + 1; index < lines.length; index += 1) {
    if (depths[index] === methodDepth + 1 && /^\s*}\s*$/.test(lines[index])) {
      return index;
    }
  }
  return -1;
}

function isTerminalHighLevelStatement(statement) {
  if (!statement) return false;
  return /^(?:return\b|throw\s*\(|abort(?:\s*\(|\s*;))/.test(statement);
}

export function inferDeclarationTypesByLine(lines, depths, knownCallTypes = new Map()) {
  const typesByLine = new Map();
  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    let end = start + 1;
    while (end < lines.length) {
      if (depths[end] === methodDepth + 1 && /^\s*}\s*$/.test(lines[end])) break;
      end += 1;
    }
    const methodTypes = inferDeclarationTypes(
      lines.slice(start, end + 1),
      knownCallTypes,
    );
    for (let index = start + 1; index < end; index += 1) {
      typesByLine.set(index, methodTypes);
    }
    start = end;
  }
  return typesByLine;
}

export function inferReturnTypesByLine(lines, depths) {
  const returnTypes = new Map();
  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    const end = findMethodEnd(lines, depths, start, methodDepth);
    if (end < 0) continue;
    const raw = lines[start].match(/->\s*([^\s{]+)/)?.[1] ?? "void";
    const expected = raw.toLowerCase() === "any" ? "object" : csharpType(raw);
    for (let line = start + 1; line < end; line += 1) returnTypes.set(line, expected);
    start = end;
  }
  return returnTypes;
}
