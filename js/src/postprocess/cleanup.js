/** Final syntax-cleanup passes for lifted statements. */
import {
  containsIdentifier,
  findBlockEnd,
  isBlank,
  isComment,
  isIfOpen,
  isTempIdent,
  leadingWhitespace,
  negateCondition,
  nextCodeLine,
  parseAssignment,
  replaceIdentifier,
} from "./helpers.js";
const TEMP_TOKEN_RE = /\bt\d+\b/g;
// ─── Pass 11: collapse_if_true ─────────────────────────────────────────────

function collapseIfTrue(statements) {
  let index = 0;
  while (index < statements.length) {
    if (statements[index].trim() !== "if true {") {
      index++;
      continue;
    }
    const end = findBlockEnd(statements, index);
    if (end < 0 || statements[end].trim() !== "}") {
      index++;
      continue;
    }
    statements.splice(end, 1);
    statements.splice(index, 1);
  }
}

// ─── Pass 12: invert_empty_if_else ─────────────────────────────────────────

function invertEmptyIfElse(statements) {
  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();
    if (!isIfOpen(trimmed)) {
      index++;
      continue;
    }

    // Check if body is empty (only comments between if and })
    let j = index + 1;
    while (j < statements.length) {
      if (!isBlank(statements[j])) break;
      j++;
    }
    if (j >= statements.length || statements[j].trim() !== "}") {
      index++;
      continue;
    }

    const closeIf = j;
    if (closeIf + 1 >= statements.length || statements[closeIf + 1].trim() !== "else {") {
      index++;
      continue;
    }

    const elseLine = closeIf + 1;
    const elseEnd = findBlockEnd(statements, elseLine);
    if (elseEnd < 0) {
      index++;
      continue;
    }

    const cond = trimmed.slice(3, -2);
    const negated = negateCondition(cond);
    const indent = leadingWhitespace(statements[index]);
    statements[index] = `${indent}if ${negated} {`;
    // Drop the empty if-body `}` and the `else {` opener. The else block's own
    // closing `}` (at elseEnd) is intentionally retained — it becomes the closer
    // for the inverted `if`, keeping brace balance intact. (elseEnd is still used
    // above as a well-formedness guard.)
    statements.splice(closeIf, 2);
  }
}

// ─── Pass 13: remove_empty_if ──────────────────────────────────────────────

function removeEmptyIf(statements) {
  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();
    if (!isIfOpen(trimmed)) {
      index++;
      continue;
    }

    let j = index + 1;
    while (j < statements.length) {
      if (!isBlank(statements[j])) break;
      j++;
    }
    if (j >= statements.length || statements[j].trim() !== "}") {
      index++;
      continue;
    }

    // Must NOT be followed by else
    if (j + 1 < statements.length && statements[j + 1].trim().startsWith("else")) {
      index++;
      continue;
    }
    statements.splice(index, j - index + 1);
  }
}

// ─── Pass 14: strip_stack_comments ─────────────────────────────────────────

function stripStackComments(statements) {
  for (let i = 0; i < statements.length; i++) {
    const trimmed = statements[i].trim();
    if (
      trimmed.startsWith("// drop ") ||
      trimmed.startsWith("// remove second") ||
      // `// xdrop stack[N] (removed X)` and the dynamic-index variant
      // are accurate but read as VM-internal noise — Rust strips them
      // already; mirror that so the two ports stay byte-identical on
      // contracts that hit XDROP.
      trimmed.startsWith("// xdrop stack") ||
      // ROT / TUCK / REVERSEN / CLEAR also leave purely descriptive
      // VM-mechanics annotations; the data-flow is captured in the
      // subsequent variable references, so the comment is redundant.
      trimmed.startsWith("// rotate top") ||
      trimmed.startsWith("// tuck top") ||
      trimmed.startsWith("// reverse top") ||
      trimmed === "// clear stack"
    ) {
      statements[i] = "";
      continue;
    }
    for (const suffix of [" // duplicate top of stack", " // copy second stack value"]) {
      const pos = statements[i].indexOf(suffix);
      if (pos >= 0) {
        statements[i] = statements[i].slice(0, pos);
      }
    }
  }
}

// ─── Pass 15: eliminate_identity_temps ─────────────────────────────────────

function eliminateIdentityTemps(statements) {
  // Pre-scan: record first and last occurrence of each temp identifier.
  // This avoids O(n) backward/forward scans per temp.
  const firstSeen = new Map(); // temp → first line index
  const lastSeen = new Map();  // temp → last line index
  for (let i = 0; i < statements.length; i++) {
    const t = statements[i].trim();
    if (t === "") continue;
    const matches = t.match(TEMP_TOKEN_RE);
    if (matches) {
      for (const m of matches) {
        if (!firstSeen.has(m)) firstSeen.set(m, i);
        lastSeen.set(m, i);
      }
    }
  }

  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();
    if (!trimmed.startsWith("let ")) {
      index++;
      continue;
    }
    const assign = parseAssignment(trimmed);
    if (!assign || !isTempIdent(assign.lhs) || !isTempIdent(assign.rhs)) {
      index++;
      continue;
    }

    // Self-assignment is dead code
    if (assign.lhs === assign.rhs) {
      statements[index] = "";
      index++;
      continue;
    }

    // Don't substitute if lhs appeared earlier (O(1) check via pre-scan)
    const first = firstSeen.get(assign.lhs);
    if (first !== undefined && first < index) {
      index++;
      continue;
    }

    // Substitute lhs -> rhs in all subsequent lines
    for (let j = index + 1; j < statements.length; j++) {
      if (containsIdentifier(statements[j], assign.lhs)) {
        statements[j] = replaceIdentifier(statements[j], assign.lhs, assign.rhs);
      }
    }
    statements[index] = "";
    index++;
  }
}

// ─── Pass 16: collapse_temp_into_store ─────────────────────────────────────

function collapseTempIntoStore(statements) {
  // Pre-scan: count how many lines each temp appears on.
  // If a temp appears on exactly 2 lines (definition + single use), it's safe
  // to collapse without scanning forward. This replaces O(n) per-temp scans.
  const tempLineCounts = new Map();
  for (let i = 0; i < statements.length; i++) {
    const t = statements[i].trim();
    if (t === "") continue;
    const matches = t.match(TEMP_TOKEN_RE);
    if (matches) {
      const seen = new Set(matches); // dedupe within same line
      for (const m of seen) {
        tempLineCounts.set(m, (tempLineCounts.get(m) || 0) + 1);
      }
    }
  }

  let index = 0;
  while (index + 1 < statements.length) {
    const trimmed = statements[index].trim();
    if (!trimmed.startsWith("let ")) {
      index++;
      continue;
    }
    const a1 = parseAssignment(trimmed);
    if (!a1 || !isTempIdent(a1.lhs)) {
      index++;
      continue;
    }

    const next = nextCodeLine(statements, index + 1);
    if (next < 0) {
      index++;
      continue;
    }

    const temp = a1.lhs;
    const trimmedNext = statements[next].trim();

    // Assignment pattern: [let] X = tN;
    const a2 = parseAssignment(trimmedNext);
    if (a2 && a2.rhs === temp) {
      // temp on exactly 2 lines (definition + this use) means not used later
      const usedLater = (tempLineCounts.get(temp) || 0) > 2;
      if (!usedLater) {
        const indent = leadingWhitespace(statements[next]);
        const prefix = a2.hasLet ? "let " : "";
        statements[next] = `${indent}${prefix}${a2.lhs} = ${a1.rhs};`;
        statements[index] = "";
        index = next + 1;
        continue;
      }
    }

    // Return pattern: return tN;
    if (trimmedNext === `return ${temp};`) {
      const usedLater = (tempLineCounts.get(temp) || 0) > 2;
      if (!usedLater) {
        const indent = leadingWhitespace(statements[next]);
        statements[next] = `${indent}return ${a1.rhs};`;
        statements[index] = "";
        index = next + 1;
        continue;
      }
    }
    index++;
  }
}

export {
  collapseIfTrue,
  invertEmptyIfElse,
  removeEmptyIf,
  stripStackComments,
  eliminateIdentityTemps,
  collapseTempIntoStore,
};
