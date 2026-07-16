import { parseConstantPrefix } from "./csharp-expression-constant-evaluator.js";

// Keep the evaluator available through the original internal module path for
// callers that imported it before the implementation was split.
export { foldConstantExpression } from "./csharp-expression-constant-evaluator.js";

const CONTROL_PREFIXES = new Set([
  "abort",
  "assert",
  "case",
  "if",
  "return",
  "throw",
  "while",
]);

/**
 * Fold complete constant subexpressions embedded in a high-level source line.
 * The scanner preserves quoted strings and comments, and only accepts a
 * candidate at a statement/expression boundary so `value + 1 + 2` is not
 * partially regrouped.
 */
export function rewriteConstantExpressions(line) {
  const replacements = [];
  let blockComment = false;
  for (let index = 0; index < line.length; index += 1) {
    if (blockComment) {
      if (line.startsWith("*/", index)) {
        blockComment = false;
        index += 1;
      }
      continue;
    }
    if (line.startsWith("//", index)) break;
    if (line.startsWith("/*", index)) {
      blockComment = true;
      index += 1;
      continue;
    }
    if (line[index] === '"' || line[index] === "'") {
      index = quotedStringEnd(line, index);
      continue;
    }
    if (!isCandidateStart(line, index)) continue;

    const parsed = parseConstantPrefix(line, index);
    if (!parsed || parsed.end <= index || !hasOperator(line.slice(index, parsed.end))) continue;
    if (!validCandidateStart(line, index) || !validCandidateEnd(line, parsed.end)) continue;
    replacements.push({
      start: index,
      end: parsed.end,
      text: parsed.text,
    });
    index = parsed.end - 1;
  }

  if (replacements.length === 0) return line;
  let output = "";
  let cursor = 0;
  for (const replacement of replacements) {
    if (replacement.start < cursor) continue;
    output += line.slice(cursor, replacement.start) + replacement.text;
    cursor = replacement.end;
  }
  return output + line.slice(cursor);
}

function isCandidateStart(line, index) {
  const character = line[index];
  if (/[0-9]/.test(character) || character === "(") return true;
  if (character === "-" || character === "+" || character === "!" || character === "~") {
    return /[\s(0-9]/.test(line[index + 1] ?? "");
  }
  return line.startsWith("true", index) || line.startsWith("false", index);
}

function validCandidateStart(line, index) {
  let cursor = index - 1;
  while (cursor >= 0 && /\s/.test(line[cursor])) cursor -= 1;
  if (cursor < 0) return true;
  if ("=(:,[{};".includes(line[cursor])) return true;
  if (line[cursor] === "?") return true;
  if (/[+*/%&|^<>!-]/.test(line[cursor])) return false;
  let end = cursor + 1;
  while (cursor >= 0 && /[A-Za-z_]/.test(line[cursor])) cursor -= 1;
  const word = line.slice(cursor + 1, end);
  return CONTROL_PREFIXES.has(word);
}

function validCandidateEnd(line, end) {
  let cursor = end;
  while (cursor < line.length && /\s/.test(line[cursor])) cursor += 1;
  if (cursor >= line.length || line.startsWith("//", cursor) || line.startsWith("/*", cursor)) return true;
  return ";,)]}:".includes(line[cursor]) || line[cursor] === "{";
}

function hasOperator(source) {
  return /[()+\-*/%<>=!&|^]/.test(source);
}

function quotedStringEnd(line, start) {
  const quote = line[start];
  for (let index = start + 1; index < line.length; index += 1) {
    if (line[index] === "\\") {
      index += 1;
    } else if (line[index] === quote) {
      return index;
    }
  }
  return line.length - 1;
}
