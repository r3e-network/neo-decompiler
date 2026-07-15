import { inferDeclarationTypes } from "./csharp-types.js";

// C# does not permit a local in a nested block to reuse a name from an
// enclosing block, while VM slot names can be reused after control-flow joins.
// Normalize only the names whose lexical lifetime cannot be represented by the
// source-shaped C# body; ordinary one-scope `let` declarations stay inline.
export function buildCSharpScopePlans(lines, depths, typedDeclarations = true) {
  const plansByLine = new Map();
  const declarationsByStart = new Map();

  for (let start = 0; start < lines.length; start += 1) {
    if (!/^\s*fn\s+.*\{\s*$/.test(lines[start])) continue;
    const methodDepth = depths[start];
    const end = findMethodEnd(lines, depths, start, methodDepth);
    if (end < 0) continue;

    const parameterNames = methodParameterNames(lines[start]);
    const methodTypes = inferDeclarationTypes(lines.slice(start, end + 1));
    const scopeEnds = computeScopeEnds(depths, start, end, methodDepth);
    const declarations = collectDeclarations(lines, depths, start, end, scopeEnds);
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
      const declared = trimmed.match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)\s*=/)?.[1];
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
      const declaration = trimmed.match(/^(let\s+)([A-Za-z_][A-Za-z0-9_]*)(\s*=)/);
      if (!declaration || !hoistedNames.has(declaration[2])) continue;
      plansByLine.set(line, lines[line].replace(declaration[0], `${declaration[2]}${declaration[3]}`));
    }
    start = end;
  }

  return { plansByLine, declarationsByStart };
}

function collectDeclarations(lines, depths, start, end, scopeEnds) {
  const declarations = new Map();
  for (let line = start + 1; line < end; line += 1) {
    const match = lines[line].trim().match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)\s*=/);
    if (!match) continue;
    const name = match[1];
    const entry = {
      line,
      depth: depths[line],
      scopeEnd: scopeEnds[line] ?? end,
    };
    if (!declarations.has(name)) declarations.set(name, []);
    declarations.get(name).push(entry);
  }
  return declarations;
}

function computeScopeEnds(depths, start, end, methodDepth) {
  const scopeEnds = new Map();
  for (let line = start + 1; line < end; line += 1) {
    const depth = depths[line];
    let scopeEnd = end;
    for (let cursor = line + 1; cursor <= end; cursor += 1) {
      if (cursor === end || depths[cursor] < depth) {
        scopeEnd = cursor;
        break;
      }
    }
    scopeEnds.set(line, scopeEnd);
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
