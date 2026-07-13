import {
  extractElseIfCondition,
  extractIfCondition,
  extractWhileCondition,
  isComment,
  isValidIdentifier,
  leadingWhitespace,
  parseAssignment,
  parseForParts,
} from "./helpers.js";

// Pass 9: rewrite_compound_assignments

export function rewriteCompoundAssignments(statements) {
  for (let i = 0; i < statements.length; i++) {
    const trimmed = statements[i].trim();
    if (trimmed === "" || isComment(trimmed)) continue;

    // For-loop header
    if (trimmed.startsWith("for (") && trimmed.endsWith("{")) {
      const parts = parseForParts(trimmed);
      if (parts) {
        const rewritten = rewriteIncrement(parts.increment);
        if (rewritten) {
          statements[i] = `for (${parts.init}; ${parts.condition}; ${rewritten}) {`;
        }
      }
      continue;
    }

    // Skip let bindings
    if (trimmed.startsWith("let ")) continue;

    const assign = parseAssignment(trimmed);
    if (!assign) continue;

    const result = rewriteRhs(assign.lhs, assign.rhs);
    if (result) {
      const indent = leadingWhitespace(statements[i]);
      statements[i] = `${indent}${assign.lhs} ${result.op} ${result.tail};`;
    }
  }
}

function rewriteIncrement(increment) {
  const eqIdx = increment.indexOf(" = ");
  if (eqIdx < 0) return null;
  const lhs = increment.slice(0, eqIdx).trim();
  const rhs = increment.slice(eqIdx + 3).trim();
  if (!isValidIdentifier(lhs)) return null;
  const result = rewriteRhs(lhs, rhs);
  if (!result) return null;
  return `${lhs} ${result.op} ${result.tail}`;
}

function rewriteRhs(lhs, rhs) {
  const plusPrefix = `${lhs} + `;
  if (rhs.startsWith(plusPrefix)) return { op: "+=", tail: rhs.slice(plusPrefix.length) };
  const minusPrefix = `${lhs} - `;
  if (rhs.startsWith(minusPrefix)) return { op: "-=", tail: rhs.slice(minusPrefix.length) };
  return null;
}

// Pass 10: rewrite_indexing_syntax

export function rewriteIndexingSyntax(statements) {
  for (let i = 0; i < statements.length; i++) {
    const trimmed = statements[i].trim();
    if (trimmed === "" || isComment(trimmed)) continue;

    // set_item(expr) -> expr[key] = value
    const setItem = rewriteSetItem(trimmed);
    if (setItem) {
      statements[i] = setItem;
      continue;
    }

    // For-loop header
    if (trimmed.startsWith("for (") && trimmed.endsWith("{")) {
      const parts = parseForParts(trimmed);
      if (parts) {
        statements[i] =
          `for (${parts.init}; ${rewriteExpr(parts.condition)}; ${rewriteExpr(parts.increment)}) {`;
      }
      continue;
    }

    // If condition
    const ifCond = extractIfCondition(trimmed);
    if (ifCond !== null) {
      statements[i] = `if ${rewriteExpr(ifCond)} {`;
      continue;
    }

    // Else-if condition
    const elseIfCond = extractElseIfCondition(trimmed);
    if (elseIfCond !== null) {
      const prefix = trimmed.startsWith("} ") ? "} " : "";
      statements[i] = `${prefix}else if ${rewriteExpr(elseIfCond)} {`;
      continue;
    }

    // While condition
    const whileCond = extractWhileCondition(trimmed);
    if (whileCond !== null) {
      statements[i] = `while ${rewriteExpr(whileCond)} {`;
      continue;
    }

    // Assignment
    const assign = parseAssignment(trimmed);
    if (assign) {
      const indent = leadingWhitespace(statements[i]);
      const prefix = assign.hasLet ? "let " : "";
      statements[i] = `${indent}${prefix}${assign.lhs} = ${rewriteExpr(assign.rhs)};`;
      continue;
    }

    // Generic statement ending with ;
    if (trimmed.endsWith(";")) {
      const indent = leadingWhitespace(statements[i]);
      statements[i] = `${indent}${rewriteExpr(trimmed.slice(0, -1))};`;
    }
  }
}

function rewriteSetItem(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith("set_item(") || !trimmed.endsWith(");")) return null;
  const body = trimmed.slice(9, -2);
  const args = splitArgs(body);
  if (args.length !== 3) return null;
  return `${rewriteExpr(args[0])}[${rewriteExpr(args[1])}] = ${rewriteExpr(args[2])};`;
}

function rewriteExpr(expr) {
  expr = expr.trim();
  if (expr === "") return "";

  // Find the leftmost operator outside a string literal.
  const op = findExprOp(expr);
  if (!op) return expr;
  const { pos, kind } = op;

  const left = expr.slice(0, pos).trim();
  const right = expr.slice(pos + (kind === "get" ? 5 : 10));
  if (kind === "get") return `${left}[${rewriteExpr(right)}]`;
  return `has_key(${left}, ${rewriteExpr(right)})`;
}

function findExprOp(s) {
  let inString = false;
  for (let i = 0; i < s.length; i += 1) {
    const c = s[i];
    if (inString) {
      if (c === "\\") {
        i += 1;
        continue;
      }
      if (c === '"') inString = false;
      continue;
    }
    if (c === '"') {
      inString = true;
      continue;
    }
    if (s.startsWith(" get ", i)) return { pos: i, kind: "get" };
    if (s.startsWith(" has_key ", i)) return { pos: i, kind: "has_key" };
  }
  return null;
}

function splitArgs(text) {
  const out = [];
  let depth = 0;
  let current = "";
  for (const ch of text) {
    if ("([{".includes(ch)) {
      depth++;
      current += ch;
    } else if (")]}".includes(ch)) {
      depth = Math.max(0, depth - 1);
      current += ch;
    } else if (ch === "," && depth === 0) {
      out.push(current.trim());
      current = "";
    } else {
      current += ch;
    }
  }
  if (current.trim()) out.push(current.trim());
  return out;
}
