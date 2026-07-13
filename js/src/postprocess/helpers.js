// Shared parsing and identifier helpers for postprocess passes.
export function isBlank(line) {
  const t = line.trim();
  return t === "" || t.startsWith("//");
}

export function isComment(line) {
  return line.trim().startsWith("//");
}

export function nextCodeLine(statements, start) {
  for (let i = start; i < statements.length; i++) {
    if (!isBlank(statements[i])) return i;
  }
  return -1;
}

export function prevCodeLine(statements, start) {
  for (let i = start - 1; i >= 0; i--) {
    const t = statements[i].trim();
    if (!isBlank(t) && t !== "}") return i;
  }
  return -1;
}

export function braceDelta(line) {
  let opens = 0,
    closes = 0;
  let inString = false;
  let quote = "";
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inString) {
      if (ch === "\\" && i + 1 < line.length) {
        i++; // skip escaped character
      } else if (ch === quote) {
        inString = false;
      }
    } else if (ch === '"' || ch === "'") {
      inString = true;
      quote = ch;
    } else if (ch === "{") {
      opens++;
    } else if (ch === "}") {
      closes++;
    }
  }
  return opens - closes;
}

export function findBlockEnd(statements, start) {
  let depth = braceDelta(statements[start]);
  for (let i = start + 1; i < statements.length; i++) {
    depth += braceDelta(statements[i]);
    if (depth === 0) return i;
  }
  return -1;
}

export function isIfOpen(line) {
  const t = line.trim();
  return t.startsWith("if ") && t.endsWith(" {");
}

export function isElseOpen(line) {
  const t = line.trim();
  return t === "else {" || t === "} else {";
}

export function isElseIfOpen(line) {
  const t = line.trim();
  const stripped = t.startsWith("} ") ? t.slice(2) : t;
  return stripped.startsWith("else if ") && stripped.endsWith(" {");
}

export function extractIfCondition(line) {
  const t = line.trim();
  if (!t.startsWith("if ") || !t.endsWith(" {")) return null;
  return t.slice(3, -2).trim();
}

export function extractElseIfCondition(line) {
  const t = line.trim();
  const stripped = t.startsWith("} ") ? t.slice(2) : t;
  if (!stripped.startsWith("else if ") || !stripped.endsWith(" {")) return null;
  return stripped.slice(8, -2).trim();
}

export function extractAnyIfCondition(line) {
  return extractIfCondition(line) ?? extractElseIfCondition(line);
}

export function extractWhileCondition(line) {
  const t = line.trim();
  if (!t.startsWith("while ") || !t.endsWith(" {")) return null;
  return t.slice(6, -2).trim();
}

export function parseAssignment(line) {
  const trimmed = line.trim();
  if (trimmed === "" || !trimmed.endsWith(";")) return null;
  const body = trimmed.slice(0, -1).trim();
  const eqIdx = body.indexOf("=");
  if (eqIdx < 0) return null;
  const lhsRaw = body.slice(0, eqIdx).trim();
  const rhs = body.slice(eqIdx + 1).trim();
  if (lhsRaw === "" || rhs === "" || rhs.startsWith("=")) return null;
  // Reject compound operators: lhs ending with !, <, >
  if (/[!<>]$/.test(lhsRaw)) return null;
  const hasLet = lhsRaw.startsWith("let ");
  const lhs = hasLet ? lhsRaw.slice(4).trim() : lhsRaw;
  if (!isValidIdentifier(lhs)) return null;
  return { lhs, rhs, hasLet, full: body };
}

export function parseForParts(line) {
  const t = line.trim();
  if (!t.startsWith("for (") || !t.endsWith(" {")) return null;
  let inner = t.slice(5, -2).trim();
  if (inner.endsWith(")")) inner = inner.slice(0, -1).trim();
  const parts = inner.split(";");
  if (parts.length !== 3) return null;
  return {
    init: parts[0].trim(),
    condition: parts[1].trim(),
    increment: parts[2].trim(),
  };
}

export function isValidIdentifier(s) {
  if (s.length === 0) return false;
  if (!/^[A-Za-z_]/.test(s)) return false;
  return /^\w+$/.test(s);
}

export function isTempIdent(s) {
  return /^t\d+$/.test(s);
}

// Regex cache: avoids recompiling the same pattern on every call.
// Key = identifier string, value = { test: RegExp, global: RegExp }.
const identRegexCache = new Map();

export function getIdentRegex(ident) {
  let cached = identRegexCache.get(ident);
  if (!cached) {
    const pattern = `(?<![\\w])${escapeRegex(ident)}(?![\\w])`;
    cached = { test: new RegExp(pattern), global: new RegExp(pattern, "g") };
    identRegexCache.set(ident, cached);
  }
  return cached;
}

export function containsIdentifier(text, ident) {
  if (!ident) return false;
  return getIdentRegex(ident).test.test(text);
}

export function replaceIdentifier(text, ident, replacement) {
  if (!ident) return text;
  const re = getIdentRegex(ident).global;
  re.lastIndex = 0;
  return text.replace(re, replacement);
}

export function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function findMatchingClose(statements, start) {
  let depth = 0;
  for (let i = start; i < statements.length; i++) {
    const t = statements[i].trim();
    if (t.endsWith("{")) depth++;
    if (t === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

export function findMatchingBrace(statements, openIdx) {
  let depth = 1;
  for (let i = openIdx + 1; i < statements.length; i++) {
    const t = statements[i].trim();
    if (isBlank(t)) continue;
    // Close before open so a combined `} else {` line first closes the current
    // block (mirrors the Rust find_matching_brace). For open-only or close-only
    // lines the order is irrelevant.
    if (t === "}" || t.startsWith("} ")) {
      depth--;
      if (depth === 0) return i;
    }
    if (t.endsWith("{")) depth++;
  }
  return -1;
}

export function negateCondition(cond) {
  const c = cond.trim();
  if (c.includes(" && ") || c.includes(" || ")) {
    return `!(${c})`;
  }
  const ops = [
    [" === ", " !== "],
    [" !== ", " === "],
    [" == ", " != "],
    [" != ", " == "],
    [" >= ", " < "],
    [" <= ", " > "],
    [" > ", " <= "],
    [" < ", " >= "],
  ];
  for (const [op, neg] of ops) {
    const pos = c.indexOf(op);
    if (pos >= 0) {
      return `${c.slice(0, pos)}${neg}${c.slice(pos + op.length)}`;
    }
  }
  if (c.startsWith("!")) return c.slice(1);
  return `!(${c})`;
}

export function leadingWhitespace(line) {
  return line.slice(0, line.length - line.trimStart().length);
}
