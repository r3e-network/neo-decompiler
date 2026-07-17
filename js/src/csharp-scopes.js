import { inferDeclarationTypes } from "./csharp-types.js";
import { findUnusedCopyLines } from "./csharp-copies.js";

// C# does not permit a local in a nested block to reuse a name from an
// enclosing block, while VM slot names can be reused after control-flow joins.
// Normalize only the names whose lexical lifetime cannot be represented by the
// source-shaped C# body; ordinary one-scope `let` declarations stay inline.
export function buildCSharpScopePlans(
  lines,
  depths,
  typedDeclarations = true,
  knownCallTypes = new Map(),
) {
  const plansByLine = new Map();
  const declarationsByStart = new Map();
  const skippedLines = new Set();

  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    const end = findMethodEnd(lines, depths, start, methodDepth);
    if (end < 0) continue;

    const parameterNames = methodParameterNames(lines[start]);
    const methodTypes = inferDeclarationTypes(
      lines.slice(start, end + 1),
      knownCallTypes,
    );
    const unusedCopies = findUnusedCopyLines(lines, start, end, methodTypes);
    for (const line of unusedCopies.skippedLines) skippedLines.add(line);
    const scopeEnds = computeScopeEnds(depths, start, end);
    const braceCloseLines = computeBraceCloseLines(lines, start, end);
    const declarations = collectDeclarations(
      lines,
      depths,
      start,
      end,
      scopeEnds,
      braceCloseLines,
    );
    const hoistedNames = new Set();

    for (const [name, entries] of declarations) {
      if (parameterNames.has(name)) continue;
      if (entries.some((entry) => entries.length > 1 && entry.depth === entries[0].depth)) {
        hoistedNames.add(name);
      }
      if (entries.some((entry) => entries.some((other) =>
        entry !== other
        && entry.depth < other.depth
        && other.line > entry.line
        && other.line < entry.scopeEnd
      ))) {
        hoistedNames.add(name);
      }
    }

    // A slot assignment or use may occur on a path where its `let` was
    // emitted in a different branch. Hoist that generated slot so C# sees a
    // declaration even when the source-shaped branch does not.
    for (let line = start + 1; line < end; line += 1) {
      const trimmed = lines[line].trim();
      if (!trimmed || trimmed.startsWith("//") || /^fn\s+/.test(trimmed)) continue;
      const declared = declarationName(trimmed);
      const tokens = trimmed.match(/\b(?:loc|t)\d+\b/g) ?? [];
      for (const name of new Set(tokens)) {
        if (name === declared || parameterNames.has(name)) continue;
        if (/^}\s*while\b/.test(trimmed) && declarations.has(name)) {
          hoistedNames.add(name);
          continue;
        }
        const active = declarations.get(name)?.some((entry) =>
          line > entry.line && line < entry.scopeEnd && entry.depth <= depths[line]
        );
        if (!active) hoistedNames.add(name);
      }
    }

    for (const name of unusedCopies.skippedNames) hoistedNames.delete(name);

    if (hoistedNames.size === 0) {
      start = end;
      continue;
    }

    const hoisted = [...hoistedNames]
      .sort()
      .map((name) => ({
        name,
        type: typedDeclarations ? (methodTypes.get(name) ?? "dynamic") : "dynamic",
      }));
    declarationsByStart.set(start, hoisted);

    for (let line = start + 1; line < end; line += 1) {
      const trimmed = lines[line].trim();
      const declaration = declarationName(trimmed);
      if (!declaration || !hoistedNames.has(declaration)) continue;
      plansByLine.set(line, rewriteHoistedDeclaration(lines[line], declaration));
    }
    start = end;
  }

  return { plansByLine, declarationsByStart, skippedLines };
}

function collectDeclarations(lines, depths, start, end, scopeEnds, braceCloseLines) {
  const declarations = new Map();
  for (let line = start + 1; line < end; line += 1) {
    const name = declarationName(lines[line].trim());
    if (!name) continue;
    const entry = {
      line,
      depth: depths[line],
      scopeEnd: forDeclarationScopeEnd(lines, line, end, braceCloseLines)
        ?? scopeEnds.get(line)
        ?? end,
    };
    if (!declarations.has(name)) declarations.set(name, []);
    declarations.get(name).push(entry);
  }
  return declarations;
}

function forDeclarationScopeEnd(lines, line, end, braceCloseLines) {
  if (!/^for\s*\(/.test(lines[line].trim())) return null;

  if (lines[line].includes("{")) {
    return braceCloseLines.get(line) ?? end;
  }

  // The high-level emitter normally braces loop bodies, but keep single-line
  // loops scoped correctly if a future lowering pass emits one.
  return Math.min(line + 2, end);
}

function computeBraceCloseLines(lines, start, end) {
  const openLines = [];
  const closeLines = new Map();
  for (let line = start; line <= end; line += 1) {
    for (const character of lines[line]) {
      if (character === "{") {
        openLines.push(line);
      } else if (character === "}") {
        const openLine = openLines.pop();
        if (openLine !== undefined) closeLines.set(openLine, line);
      }
    }
  }
  return closeLines;
}

function declarationName(line) {
  return line.match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)\s*=/)?.[1]
    ?? line.match(/^for\s*\(\s*let\s+([A-Za-z_][A-Za-z0-9_]*)\s*=/)?.[1]
    ?? null;
}

function rewriteHoistedDeclaration(line, name) {
  const direct = line.match(/^(\s*)let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=)/);
  if (direct?.[2] === name) {
    return line.replace(direct[0], `${direct[1]}${name}${direct[3]}`);
  }
  const loop = line.match(/^(\s*for\s*\(\s*)let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=)/);
  if (loop?.[2] === name) {
    return line.replace(loop[0], `${loop[1]}${name}${loop[3]}`);
  }
  return line;
}

function computeScopeEnds(depths, start, end) {
  const scopeEnds = new Map();
  const openLines = [];
  for (let line = start + 1; line < end; line += 1) {
    const depth = depths[line];
    while (openLines.length > 0) {
      const openLine = openLines[openLines.length - 1];
      if (depths[openLine] <= depth) break;
      scopeEnds.set(openLine, line);
      openLines.pop();
    }
    openLines.push(line);
  }
  for (const line of openLines) {
    scopeEnds.set(line, end);
  }
  scopeEnds.set(start, end);
  scopeEnds.set(end, end);
  return scopeEnds;
}

function methodParameterNames(signature) {
  const parameters = signature.match(/^\s*fn\s+[^\s(]+\((.*?)\)/)?.[1] ?? "";
  return new Set(parameters.split(",").map((parameter) => {
    const separator = parameter.indexOf(":");
    return (separator < 0 ? parameter : parameter.slice(0, separator)).trim();
  }).filter(Boolean));
}

function findMethodEnd(lines, depths, start, methodDepth) {
  for (let line = start + 1; line < lines.length; line += 1) {
    if (depths[line] === methodDepth + 1 && /^\s*}\s*$/.test(lines[line])) return line;
  }
  return -1;
}
