// Optional single-use temporary inlining pass.

import {
  containsIdentifier,
  isTempIdent,
  parseAssignment,
  replaceIdentifier,
} from "./helpers.js";

const TEMP_TOKEN_RE = /\bt\d+\b/g;

function isSafeToInline(expr) {
  if (expr.includes("(")) {
    const trimmed = expr.trim();
    if (trimmed.startsWith("(") && trimmed.endsWith(")")) return true;
    return false;
  }
  return true;
}

function needsParens(expr) {
  let depth = 0;
  let inString = false;
  let quote = "";
  for (let i = 0; i < expr.length; i++) {
    const ch = expr[i];
    if (inString) {
      if (ch === "\\" && i + 1 < expr.length) i++;
      else if (ch === quote) inString = false;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inString = true;
      quote = ch;
      continue;
    }
    if (ch === "[" || ch === "(") { depth++; continue; }
    if (ch === "]" || ch === ")") { depth--; continue; }
    if (depth > 0) continue;
    if ("+-*/%<>".includes(ch)) return true;
    if ((ch === "&" || ch === "|") && expr[i + 1] === ch) return true;
    if ((ch === "=" || ch === "!") && expr[i + 1] === "=") return true;
  }
  return false;
}

function isControlFlowCondition(statement) {
  const t = statement.trim();
  return t.startsWith("if ") || t.startsWith("while ") || t.startsWith("for ") || t.startsWith("} else if ");
}

function isNumericLiteral(text) {
  const t = text.startsWith("-") ? text.slice(1) : text;
  if (t.startsWith("0x") || t.startsWith("0X")) return t.length > 2 && /^[0-9a-fA-F]+$/.test(t.slice(2));
  return t.length > 0 && /^\d+$/.test(t);
}

function isStringLiteral(text) {
  return text.length >= 2 && ((text[0] === '"' && text.at(-1) === '"') || (text[0] === "'" && text.at(-1) === "'"));
}

function isSimpleIdentifier(text) {
  return text.length > 0 && /^[A-Za-z_]/.test(text) && /^\w+$/.test(text);
}

function isTrivialInlineRhs(expr) {
  const t = expr.trim();
  if (t === "" || t === "true" || t === "false" || t === "null") return t !== "";
  return isNumericLiteral(t) || isStringLiteral(t) || isSimpleIdentifier(t);
}

function collectInlineCandidates(statements) {
  const definitions = new Map();
  const useCounts = new Map();
  const reassigned = new Set();
  const known = new Set();
  for (let idx = 0; idx < statements.length; idx++) {
    const trimmed = statements[idx].trim();
    const assign = parseAssignment(trimmed);
    const scanText = assign ? assign.rhs : trimmed;
    if (known.size > 0 && scanText !== "") {
      const matches = scanText.match(TEMP_TOKEN_RE);
      if (matches) for (const v of matches) if (known.has(v)) useCounts.set(v, (useCounts.get(v) || 0) + 1);
    }
    if (!assign || !isTempIdent(assign.lhs)) continue;
    if (assign.hasLet) {
      if (definitions.has(assign.lhs)) reassigned.add(assign.lhs);
      else {
        known.add(assign.lhs);
        definitions.set(assign.lhs, { defLine: idx, rhs: assign.rhs });
      }
    } else reassigned.add(assign.lhs);
  }
  const candidates = [];
  for (const [name, { defLine, rhs }] of definitions) {
    if ((useCounts.get(name) || 0) === 1 && !reassigned.has(name) && isSafeToInline(rhs)) candidates.push({ name, defLine, rhs });
  }
  candidates.sort((a, b) => b.defLine - a.defLine);
  return candidates;
}

function applyInlining(statements, candidates) {
  for (const candidate of candidates) {
    let inlined = false;
    for (let i = candidate.defLine + 1; i < statements.length; i++) {
      if (!containsIdentifier(statements[i], candidate.name)) continue;
      if (isControlFlowCondition(statements[i]) && !isTrivialInlineRhs(candidate.rhs)) break;
      const replacement = needsParens(candidate.rhs) ? `(${candidate.rhs})` : candidate.rhs;
      const updated = replaceIdentifier(statements[i], candidate.name, replacement);
      if (updated !== statements[i]) {
        statements[i] = updated;
        inlined = true;
        break;
      }
    }
    if (inlined) statements[candidate.defLine] = "";
  }
}

export function inlineSingleUseTemps(statements) {
  applyInlining(statements, collectInlineCandidates(statements));
}
