/**
 * Find generated local/temporary copies that have no observable consumer.
 *
 * The high-level view intentionally keeps these definitions for analysis,
 * but a C# contract does not need a dead `locN`/`tN` alias. Restricting this
 * pass to generated slot names and direct identifier copies keeps source
 * variables, static slots, and side-effecting expressions out of it.
 */
export function findUnusedCopyLines(lines, start, end, declarationTypes) {
  const definitionsByTarget = new Map();
  const definitionLines = new Map();

  for (let line = start + 1; line < end; line += 1) {
    const code = stripLineComment(lines[line]);
    const declaration = code.trim().match(
      /^let\s+(@?[A-Za-z_][A-Za-z0-9_]*)\s*=\s*(@?[A-Za-z_][A-Za-z0-9_]*)\s*;$/u,
    );
    if (!declaration) continue;
    const target = withoutAt(declaration[1]);
    const source = withoutAt(declaration[2]);
    if (!isGeneratedCopyName(target) || target === source) continue;
    const entry = { line, source };
    if (!definitionsByTarget.has(target)) definitionsByTarget.set(target, []);
    definitionsByTarget.get(target).push(entry);
    if (!definitionLines.has(line)) definitionLines.set(line, { target, source });
  }

  const skippedLines = new Set();
  const skippedNames = new Set();
  for (const [target, copies] of definitionsByTarget) {
    if (!hasCompatibleTypes(target, copies, declarationTypes)) continue;
    if (hasNonCopyDefinition(lines, start, end, target, copies)) continue;
    if (isReadOutsideOwnCopies(lines, start, end, target, copies, definitionLines)) continue;
    skippedNames.add(target);
    for (const copy of copies) skippedLines.add(copy.line);
  }

  return { skippedLines, skippedNames };
}

function isGeneratedCopyName(name) {
  return /^(?:loc|t)\d+$/u.test(name);
}

function hasCompatibleTypes(target, copies, declarationTypes) {
  const targetType = declarationTypes?.get(target) ?? "dynamic";
  for (const { source } of copies) {
    const sourceType = declarationTypes?.get(source) ?? "dynamic";
    if (targetType === "dynamic" && sourceType === "dynamic") continue;
    if (targetType !== sourceType) return false;
  }
  return true;
}

function hasNonCopyDefinition(lines, start, end, target, copies) {
  const ownCopyLines = new Set(copies.map(({ line }) => line));
  const declarationPattern = new RegExp(
    `^let\\s+@?${escapeRegExp(target)}\\b`,
    "u",
  );
  for (let line = start + 1; line < end; line += 1) {
    const code = stripLineComment(lines[line]).trim();
    if (!declarationPattern.test(code)) continue;
    if (!ownCopyLines.has(line)) return true;
  }
  return false;
}

function isReadOutsideOwnCopies(lines, start, end, target, copies, definitionLines) {
  const ownCopyLines = new Set(copies.map(({ line }) => line));
  const token = new RegExp(`\\b${escapeRegExp(target)}\\b`, "gu");
  for (let line = start + 1; line < end; line += 1) {
    const code = stripLineComment(lines[line]);
    for (const match of code.matchAll(token)) {
      if (ownCopyLines.has(line)) {
        const definition = definitionLines.get(line);
        if (definition?.target === target && match.index < code.indexOf("=")) continue;
      }
      return true;
    }
  }
  return false;
}

function stripLineComment(line) {
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "/" && line[index + 1] === "/") {
      return line.slice(0, index);
    }
  }
  return line;
}

function withoutAt(name) {
  return name.replace(/^@/u, "");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}
