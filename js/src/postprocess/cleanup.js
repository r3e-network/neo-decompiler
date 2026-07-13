/** Final syntax-cleanup passes for lifted statements. */
import {
  findBlockEnd,
  isBlank,
  isComment,
  isIfOpen,
  leadingWhitespace,
  negateCondition,
} from "./helpers.js";
import { eliminateIdentityTemps, collapseTempIntoStore } from "./temps.js";
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

export {
  collapseIfTrue,
  invertEmptyIfElse,
  removeEmptyIf,
  stripStackComments,
  eliminateIdentityTemps,
  collapseTempIntoStore,
};
