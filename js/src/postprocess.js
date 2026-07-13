/**
 * Post-processing passes for high-level decompiler output.
 *
 * Ported from Rust's decompiler/high_level/emitter/postprocess/ module.
 * These passes rewrite raw lifted statements into cleaner pseudo-code.
 *
 * Pass order MUST match the Rust finish() method in core.rs.
 */

import {
  braceDelta,
  containsIdentifier,
  escapeRegex,
  extractAnyIfCondition,
  extractElseIfCondition,
  extractIfCondition,
  extractWhileCondition,
  findBlockEnd,
  findMatchingBrace,
  findMatchingClose,
  isBlank,
  isComment,
  isElseIfOpen,
  isElseOpen,
  isIfOpen,
  isValidIdentifier,
  isTempIdent,
  leadingWhitespace,
  negateCondition,
  nextCodeLine,
  parseAssignment,
  parseForParts,
  prevCodeLine,
  replaceIdentifier,
} from "./postprocess/helpers.js";

// ─── Pass 1: rewrite_else_if_chains ────────────────────────────────────────

function rewriteElseIfChains(statements) {
  let i = 0;
  while (i + 1 < statements.length) {
    if (isElseOpen(statements[i]) && isIfOpen(statements[i + 1])) {
      const condition = extractIfCondition(statements[i + 1]);
      if (condition !== null) {
        statements[i] = `} else if ${condition} {`;
        statements.splice(i + 1, 1);
        // Find and remove extra closing brace
        const closeIdx = findMatchingClose(statements, i);
        if (closeIdx >= 0) {
          removeOneCloser(statements, closeIdx);
        }
        continue;
      }
    }
    i++;
  }
}

function removeOneCloser(statements, closeIdx) {
  if (closeIdx + 1 < statements.length) {
    if (statements[closeIdx].trim() === "}" && statements[closeIdx + 1].trim() === "}") {
      statements.splice(closeIdx + 1, 1);
      return;
    }
  }
  if (statements[closeIdx].trim() === "}" && closeIdx > 0) {
    for (let i = closeIdx - 1; i >= 0; i--) {
      const prev = statements[i].trim();
      if (prev !== "") {
        if (prev === "}") statements.splice(closeIdx, 1);
        break;
      }
    }
  }
}

// ─── Pass 2: collapse_overflow_checks ──────────────────────────────────────

const OVERFLOW_BOUNDS = [
  "-2147483648",
  "0",
  "-9223372036854775808",
  "2147483647",
  "4294967295",
  "9223372036854775807",
  "18446744073709551615",
];

// Hot postprocess regex literals: hoisted to module level so each pass
// reuses the same compiled instance instead of relying on per-call interning.
const GOTO_LABEL_RE = /^goto\s+(label_0x[\da-f]+);$/i;
const LEAVE_LABEL_RE = /^leave\s+(label_0x[\da-f]+);$/i;
const LABEL_LINE_RE = /^(label_0x[\da-f]+):$/i;
const IF_GOTO_RE = /^if\s+.+\{\s*goto\s+(label_0x[\da-f]+);\s*\}$/i;
const DO_WHILE_END_RE = /^}\s+while\s+(?:!\((.+)\)|(.+))\);?$/;
const TEMP_TOKEN_RE = /\bt\d+\b/g;

function collapseOverflowChecks(statements) {
  let index = 0;
  while (index < statements.length) {
    const collapse = tryMatchOverflow(statements, index);
    if (collapse) {
      applyOverflowCollapse(statements, collapse);
      continue;
    }
    index++;
  }
}

function tryMatchOverflow(statements, idx) {
  const line0 = statements[idx].trim();
  if (line0 === "" || line0.startsWith("//")) return null;
  const a1 = parseLetAssignment(line0);
  if (!a1) return null;

  const dupIdx = nextCodeLine(statements, idx + 1);
  if (dupIdx < 0) return null;
  const a2 = parseLetAssignment(statements[dupIdx].trim());
  if (!a2 || a2.rhs !== a1.lhs) return null;

  const boundIdx = nextCodeLine(statements, dupIdx + 1);
  if (boundIdx < 0) return null;
  const a3 = parseLetAssignment(statements[boundIdx].trim());
  if (!a3 || !OVERFLOW_BOUNDS.includes(a3.rhs)) return null;

  const ifIdx = nextCodeLine(statements, boundIdx + 1);
  if (ifIdx < 0) return null;
  const ifLine = statements[ifIdx].trim();
  if (!ifLine.startsWith("if ") || !ifLine.endsWith("{")) return null;
  if (!ifLine.includes(`${a2.lhs} <`) && !ifLine.includes(`${a2.lhs} ==`) && !ifLine.includes(`${a2.lhs} >`)) {
    return null;
  }

  const ifBlockEnd = findMatchingBrace(statements, ifIdx);
  if (ifBlockEnd < 0) return null;

  // Check for checked vs unchecked
  let firstBody = null;
  for (let i = ifIdx + 1; i < statements.length; i++) {
    const t = statements[i].trim();
    if (!isBlank(t)) {
      firstBody = t;
      break;
    }
  }
  const isChecked = firstBody !== null && firstBody.startsWith("throw(");

  let blankEnd = ifBlockEnd;
  let elseUnwrap = null;

  if (!isChecked) {
    // Unchecked: consume entire if+else block
    const next = nextCodeLine(statements, ifBlockEnd + 1);
    if (next >= 0) {
      const nt = statements[next].trim();
      if (nt === "else {" || nt === "} else {") {
        const elseEnd = findMatchingBrace(statements, next);
        if (elseEnd >= 0) blankEnd = elseEnd;
      }
    }
  } else {
    // Checked: check for else block to unwrap
    const next = nextCodeLine(statements, ifBlockEnd + 1);
    if (next >= 0) {
      const nt = statements[next].trim();
      if (nt === "else {" || nt === "} else {") {
        const elseEnd = findMatchingBrace(statements, next);
        if (elseEnd >= 0) elseUnwrap = [next, elseEnd];
      }
    }
  }

  return {
    opLine: idx,
    expr: a1.rhs,
    resultVar: a1.lhs,
    blankStart: idx + 1,
    blankEnd,
    isChecked,
    elseUnwrap,
  };
}

function applyOverflowCollapse(statements, c) {
  if (c.isChecked) {
    const wrapped = c.expr.startsWith("checked(") ? c.expr : `checked(${c.expr})`;
    statements[c.opLine] = `${leadingWhitespace(statements[c.opLine])}let ${c.resultVar} = ${wrapped};`;
  }
  for (let i = c.blankStart; i <= c.blankEnd; i++) {
    statements[i] = "";
  }
  if (c.elseUnwrap) {
    statements[c.elseUnwrap[0]] = "";
    statements[c.elseUnwrap[1]] = "";
  }
  // Fixup downstream reference
  if (!c.isChecked) {
    const next = nextCodeLine(statements, c.blankEnd + 1);
    if (next >= 0) {
      const line = statements[next].trim();
      if (!line.startsWith("let ") && !line.startsWith("if ") && !line.startsWith("//")) {
        const a = parseAssignment(line);
        if (a && isTempIdent(a.rhs) && a.rhs !== c.resultVar) {
          statements[next] = `${leadingWhitespace(statements[next])}${a.lhs} = ${c.resultVar};`;
        }
      }
    }
  }
}

function parseLetAssignment(line) {
  if (!line.startsWith("let ")) return null;
  const rest = line.slice(4);
  const semiPos = rest.indexOf(";");
  if (semiPos < 0) return null;
  const body = rest.slice(0, semiPos);
  const eqPos = body.indexOf(" = ");
  if (eqPos < 0) return null;
  return { lhs: body.slice(0, eqPos).trim(), rhs: body.slice(eqPos + 3).trim() };
}

// ─── Pass 3: rewrite_goto_do_while ─────────────────────────────────────────

function rewriteGotoDoWhile(statements) {
  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();
    const labelMatch = GOTO_LABEL_RE.exec(trimmed);
    if (!labelMatch) {
      index++;
      continue;
    }
    const label = labelMatch[1];

    const doIdx = nextCodeLine(statements, index);
    if (doIdx < 0 || statements[doIdx].trim() !== "do {") {
      index++;
      continue;
    }

    const endIdx = findBlockEnd(statements, doIdx);
    if (endIdx < 0) {
      index++;
      continue;
    }
    const endTrimmed = statements[endIdx].trim();
    const condMatch = DO_WHILE_END_RE.exec(endTrimmed);
    if (!condMatch) {
      index++;
      continue;
    }
    const condition = condMatch[1] ? `!(${condMatch[1]})` : condMatch[2];

    const labelLine = `${label}:`;
    let labelIdx = -1;
    for (let i = doIdx + 1; i < endIdx; i++) {
      if (statements[i].trim() === labelLine) {
        labelIdx = i;
        break;
      }
    }
    if (labelIdx < 0) {
      index++;
      continue;
    }

    // Collect setup lines between label and } while
    const setupLines = [];
    for (let i = labelIdx + 1; i < endIdx; i++) {
      if (!isBlank(statements[i])) setupLines.push(i);
    }

    statements[index] = ""; // remove goto
    statements[doIdx] = `while ${condition} {`;
    statements[labelIdx] = ""; // remove label

    if (setupLines.length === 0) {
      statements[endIdx] = "}";
    } else {
      const copies = setupLines.map((i) => statements[i]);
      for (let j = 0; j < copies.length; j++) {
        statements.splice(doIdx + j, 0, copies[j]);
      }
      statements[endIdx + copies.length] = "}";
    }
    index++;
  }
}

// ─── Pass 4: rewrite_if_goto_to_while ──────────────────────────────────────

function rewriteIfGotoToWhile(statements) {
  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();
    const labelMatch = LABEL_LINE_RE.exec(trimmed);
    if (!labelMatch) {
      index++;
      continue;
    }
    const label = labelMatch[1];

    // Find the next `if ... {` after the label. Mirror the Rust port's
    // `next_if_line`, which scans unconditionally: any setup statements between
    // the label and the loop condition are duplicated into the lifted while
    // loop by the code below, so a non-assignment setup line must not abort the
    // scan (which previously left such loops unstructured, diverging from Rust).
    let ifIdx = -1;
    for (let i = index + 1; i < statements.length; i++) {
      const t = statements[i].trim();
      if (isIfOpen(t)) {
        ifIdx = i;
        break;
      }
    }
    if (ifIdx < 0) {
      index++;
      continue;
    }

    const endIdx = findBlockEnd(statements, ifIdx);
    if (endIdx < 0 || statements[endIdx].trim() !== "}") {
      index++;
      continue;
    }

    // Find goto label inside if-block
    const gotoTarget = `goto ${label};`;
    let gotoIdx = -1;
    for (let i = ifIdx + 1; i < endIdx; i++) {
      if (statements[i].trim() === gotoTarget) {
        gotoIdx = i;
        break;
      }
    }
    if (gotoIdx < 0) {
      index++;
      continue;
    }

    // Collect setup lines between label and if
    const setupLines = [];
    for (let i = index + 1; i < ifIdx; i++) {
      if (!isBlank(statements[i])) setupLines.push(statements[i]);
    }

    statements[index] = ""; // remove label
    statements[ifIdx] = statements[ifIdx].trim().replace(/^if /, "while ");
    statements[gotoIdx] = ""; // remove goto

    if (setupLines.length > 0) {
      for (let j = 0; j < setupLines.length; j++) {
        statements.splice(endIdx + j, 0, setupLines[j]);
      }
    }
    index++;
  }
}

// ─── Pass 5: eliminate_fallthrough_gotos ────────────────────────────────────

function eliminateFallthroughGotos(statements) {
  // Also strips the try-context `leave label_X;` form (lifted from
  // ENDTRY). The transfer is "dead" whenever the resume target sits on
  // the next executable line *or* one or more close-braces past it
  // (e.g. a `leave LABEL;` that is the last statement of a catch body
  // whose closing `}` is immediately followed by `LABEL:`). In that
  // case the C#/Rust backends would emit identical control flow either
  // way, so the explicit transfer is just noise.
  for (let i = 0; i < statements.length; i++) {
    const trimmed = statements[i].trim();
    const labelMatch =
      GOTO_LABEL_RE.exec(trimmed) ?? LEAVE_LABEL_RE.exec(trimmed);
    if (!labelMatch) continue;
    const label = labelMatch[1];
    const labelLine = `${label}:`;
    // Walk forward past blank/comment/close-brace lines to find the
    // next executable statement. If it is the matching label, the
    // transfer is dead — control would have reached the label through
    // structural fall-out anyway.
    let probe = i + 1;
    while (probe < statements.length) {
      const t = statements[probe].trim();
      if (t === "" || t.startsWith("//") || t === "}") {
        probe++;
        continue;
      }
      if (t === labelLine) {
        statements[i] = "";
      }
      break;
    }
  }
}

// ─── Pass 5a: rewrite_label_goto_to_loop ───────────────────────────────────
// Lifts `label_X: ... goto label_X;` (with no other references to label_X)
// into a `loop { ... }` block — the canonical Neo C# compiler shape for an
// unconditional infinite loop. Mirrors the Rust `rewrite_label_goto_to_loop`
// pass; runs after fallthrough-goto elimination and before orphan-label
// removal so that downstream passes see only the structured loop.

function rewriteLabelGotoToLoop(statements) {
  // Pre-collect the standalone `goto label_X;` lines once. A `label_X:` can only
  // fold into a `loop { }` when a matching standalone goto exists, so labels
  // without one are skipped in O(1) instead of each running the forward
  // goto-search to the end of the vector — without this the pass is
  // O(labels × N), a decompiler-hang DoS for guarded-goto-dense output. Mirrors
  // the Rust port (rewrite_label_goto_to_loop).
  const standaloneGotos = new Set();
  for (const stmt of statements) {
    const t = stmt.trim();
    if (t.startsWith("goto label_") && t.endsWith(";")) standaloneGotos.add(t);
  }
  if (standaloneGotos.size === 0) return;

  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();
    const labelMatch = LABEL_LINE_RE.exec(trimmed);
    if (!labelMatch) {
      index++;
      continue;
    }
    const label = labelMatch[1];
    const gotoTarget = `goto ${label};`;
    // Fast-skip: no matching standalone goto -> the forward search can only fail.
    if (!standaloneGotos.has(gotoTarget)) {
      index++;
      continue;
    }
    let depth = 0;
    let gotoIdx = -1;
    for (let j = index + 1; j < statements.length; j++) {
      const t = statements[j].trim();
      if (t === gotoTarget && depth === 0) {
        gotoIdx = j;
        break;
      }
      if (t.endsWith("{")) depth++;
      if (t === "}" || t.startsWith("} ")) {
        depth--;
        if (depth < 0) break;
      }
    }
    if (gotoIdx < 0) {
      index++;
      continue;
    }
    // Bail if there are other references to the label anywhere — a second
    // goto means the label is a structured-jump target, not just a back-edge.
    // (`labelDecl` is hoisted out of the scan; the loop short-circuits on the
    // first extra reference.)
    const labelDecl = `${label}:`;
    let hasOtherReference = false;
    for (let i = 0; i < statements.length; i++) {
      if (i === index || i === gotoIdx) continue;
      const t = statements[i].trim();
      if (t === labelDecl || t.includes(gotoTarget)) {
        hasOtherReference = true;
        break;
      }
    }
    if (hasOtherReference) {
      index++;
      continue;
    }
    const labelIndent = statements[index].slice(0, statements[index].length - statements[index].trimStart().length);
    const gotoIndent = statements[gotoIdx].slice(0, statements[gotoIdx].length - statements[gotoIdx].trimStart().length);
    statements[index] = `${labelIndent}loop {`;
    statements[gotoIdx] = `${gotoIndent}}`;
    index++;
  }
}

// ─── Pass 5b: remove_orphaned_labels ───────────────────────────────────────
// Remove labels that have no matching goto. These are artifacts from
// liftStructuredSlice falling through to liftStraightLineMethodBody.

function removeOrphanedLabels(statements) {
  // Collect all label references (goto, leave, and if { goto } patterns)
  const referenced = new Set();
  for (const stmt of statements) {
    const t = stmt.trim();
    const gotoM = GOTO_LABEL_RE.exec(t);
    if (gotoM) referenced.add(gotoM[1]);
    const leaveM = LEAVE_LABEL_RE.exec(t);
    if (leaveM) referenced.add(leaveM[1]);
    const ifGotoM = IF_GOTO_RE.exec(t);
    if (ifGotoM) referenced.add(ifGotoM[1]);
  }
  // Remove labels not referenced
  for (let i = 0; i < statements.length; i++) {
    const m = LABEL_LINE_RE.exec(statements[i].trim());
    if (m && !referenced.has(m[1])) {
      statements[i] = "";
    }
  }
}

// ─── Pass 6: rewrite_for_loops (enhanced) ──────────────────────────────────

function rewriteForLoops(statements) {
  let index = 0;
  while (index < statements.length) {
    const condition = extractWhileCondition(statements[index]);
    if (condition === null) {
      index++;
      continue;
    }

    const end = findBlockEnd(statements, index);
    if (end < 0) {
      index++;
      continue;
    }

    const initIdx = findInitializerIndex(statements, index);
    if (initIdx < 0) {
      index++;
      continue;
    }
    const initAssign = parseAssignment(statements[initIdx]);
    if (!initAssign) {
      index++;
      continue;
    }

    const increment = findIncrementAssignment(statements, index, end, initAssign.lhs);
    if (!increment) {
      index++;
      continue;
    }

    statements[index] = `for (${initAssign.full}; ${condition}; ${increment.expr}) {`;
    statements[initIdx] = "";
    statements[increment.incrementIdx] = "";
    if (increment.tempIdx !== null) {
      statements[increment.tempIdx] = "";
    }
    index++;
  }
}

function findInitializerIndex(statements, start) {
  let index = start;
  while (index > 0) {
    index--;
    const line = statements[index].trim();
    if (isBlank(line)) continue;
    if (line === "}" || line.endsWith("{")) break;
    if (line.includes("=") && line.endsWith(";")) {
      const a = parseAssignment(line);
      if (a && (a.lhs.startsWith("loc") || a.lhs.startsWith("arg") || a.lhs.startsWith("static"))) {
        return index;
      }
    }
  }
  return -1;
}

function findIncrementAssignment(statements, start, end, varName) {
  let index = end;
  while (index > start) {
    index--;
    const line = statements[index].trim();
    if (isBlank(line) || line === "}") continue;
    const assign = parseAssignment(line);
    if (!assign || assign.lhs !== varName) return null;

    if (assign.rhs.startsWith(varName)) {
      return { incrementIdx: index, tempIdx: null, expr: assign.full };
    }
    const prevIdx = prevCodeLine(statements, index);
    if (prevIdx < 0) return null;
    const prevAssign = parseAssignment(statements[prevIdx]);
    if (!prevAssign) return null;

    if (prevAssign.lhs === assign.rhs) {
      return {
        incrementIdx: index,
        tempIdx: prevIdx,
        expr: `${varName} = ${prevAssign.rhs}`,
      };
    }
    if (containsIdentifier(assign.rhs, prevAssign.lhs)) {
      const replaced = replaceIdentifier(assign.rhs, prevAssign.lhs, prevAssign.rhs);
      return {
        incrementIdx: index,
        tempIdx: prevIdx,
        expr: `${varName} = ${replaced}`,
      };
    }
    return null;
  }
  return null;
}

// ─── Pass 7: inline_condition_temps ────────────────────────────────────────

// Resolve the inlined condition text for a single-use temp whose value feeds a
// loop/if header. A bare `t` inlines to its rhs; a negated `!t` inlines to
// `!(rhs)` so the `!` binds the whole expression. Mirrors the Rust port's
// condition_inline_candidate. Returns null when the condition is neither form.
function inlinedCondition(condition, assign) {
  if (assign.lhs === condition) {
    return assign.rhs;
  }
  if (condition.startsWith("!") && condition.slice(1).trim() === assign.lhs) {
    return `!(${assign.rhs})`;
  }
  return null;
}

function inlineConditionTemps(statements) {
  let index = 0;
  while (index < statements.length) {
    const trimmed = statements[index].trim();

    let cond = null;
    let forParts = null;
    let kind = null;

    if (trimmed.endsWith(" {")) {
      if (trimmed.startsWith("while ")) {
        cond = trimmed.slice(6, -2).trim();
        kind = "while";
      } else if (trimmed.startsWith("for (")) {
        let inner = trimmed.slice(5, -2).trim();
        if (inner.endsWith(")")) inner = inner.slice(0, -1).trim();
        const parts = inner.split(";");
        if (parts.length === 3) {
          forParts = {
            init: parts[0].trim(),
            condition: parts[1].trim(),
            increment: parts[2].trim(),
          };
          kind = "for";
        }
      } else if (trimmed.startsWith("if ")) {
        cond = trimmed.slice(3, -2).trim();
        kind = "if";
      }
    }

    if (kind !== null) {
      const idx = prevCodeLine(statements, index);
      if (idx >= 0) {
        const assign = parseAssignment(statements[idx]);
        if (assign && shouldInlineCondition(assign.rhs)) {
          if (kind === "while") {
            const inlined = inlinedCondition(cond, assign);
            if (inlined !== null) {
              statements[index] = `while ${inlined} {`;
              statements[idx] = "";
            }
          } else if (kind === "for") {
            const inlined = inlinedCondition(forParts.condition, assign);
            if (inlined !== null) {
              statements[index] = `for (${forParts.init}; ${inlined}; ${forParts.increment}) {`;
              statements[idx] = "";
            }
          } else if (kind === "if") {
            const inlined = inlinedCondition(cond, assign);
            if (inlined !== null) {
              statements[index] = `if ${inlined} {`;
              statements[idx] = "";
            }
          }
        }
      }
    }

    index++;
  }
}

function shouldInlineCondition(rhs) {
  if (rhs === "true" || rhs === "false") return true;
  if (rhs.includes(" ")) return true;
  return /[<>!=+\-*\/&|]/.test(rhs);
}

// ─── Pass 8: inline_for_increment_temps ────────────────────────────────────

function inlineForIncrementTemps(statements) {
  let index = 0;
  while (index < statements.length) {
    const forParts = parseForParts(statements[index]);
    if (forParts === null) {
      index++;
      continue;
    }

    let depth = 1;
    let cursor = index + 1;
    while (cursor < statements.length && depth > 0) {
      depth += braceDelta(statements[cursor]);
      if (depth <= 0) break;
      const line = statements[cursor].trim();
      if (line.startsWith("let ")) {
        const assign = parseAssignment(line);
        if (assign && containsIdentifier(forParts.increment, assign.lhs)) {
          // Only inline+delete the definition when the temp is used nowhere
          // else (besides its own def and the for-header increment) AND its
          // value is pure (no call). Otherwise clearing the definition would
          // dangle a still-live reference, and moving a side-effecting RHS into
          // the increment would change evaluation order. Mirrors the Rust port.
          const usedElsewhere = statements.some(
            (stmt, i) => i !== cursor && i !== index && containsIdentifier(stmt, assign.lhs),
          );
          if (usedElsewhere || assign.rhs.includes("(")) {
            cursor++;
            continue;
          }
          const replaced = replaceIdentifier(forParts.increment, assign.lhs, assign.rhs);
          statements[index] = `for (${forParts.init}; ${forParts.condition}; ${replaced}) {`;
          statements[cursor] = "";
          break;
        }
      }
      cursor++;
    }
    index++;
  }
}

// ─── Pass 9: rewrite_compound_assignments ──────────────────────────────────

function rewriteCompoundAssignments(statements) {
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

// ─── Pass 10: rewrite_indexing_syntax ──────────────────────────────────────

function rewriteIndexingSyntax(statements) {
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
          `for (${rewriteExpr(parts.init)}; ${rewriteExpr(parts.condition)}; ${rewriteExpr(parts.increment)}) {`;
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

  // Find the leftmost " get " / " has_key " operator that sits OUTSIDE a string
  // literal, so a literal like "a get b" is not mistaken for an index/has_key.
  const op = findExprOp(expr);
  if (!op) return expr;
  const { pos, kind } = op;

  // `left` cannot contain another top-level (non-string) " get " / " has_key "
  // (else it would have been the leftmost match), so skip a recursive scan.
  const left = expr.slice(0, pos).trim();
  const right = expr.slice(pos + (kind === "get" ? 5 : 10));
  if (kind === "get") return `${left}[${rewriteExpr(right)}]`;
  return `has_key(${left}, ${rewriteExpr(right)})`;
}

// Locate the leftmost ` get ` / ` has_key ` operator not enclosed in a string
// literal. Returns `{ pos, kind }` or `null`.
function findExprOp(s) {
  let inString = false;
  for (let i = 0; i < s.length; i += 1) {
    const c = s[i];
    if (inString) {
      if (c === "\\") {
        i += 1; // skip the escaped character
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
  let depth = 0,
    current = "";
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
    } else current += ch;
  }
  if (current.trim()) out.push(current.trim());
  return out;
}

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

// ─── Pass 17: rewrite_switch_statements ────────────────────────────────────

function rewriteSwitchStatements(statements) {
  let index = 0;
  while (index < statements.length) {
    const result = tryBuildSwitch(statements, index);
    if (result) {
      statements.splice(index, result.end - index + 1, ...result.replacement);
      index++;
      continue;
    }
    index++;
  }
}

function tryBuildSwitch(statements, start) {
  const header = statements[start]?.trim();
  if (!isIfOpen(header)) return null;

  const cases = [];
  let defaultBody = null;
  let overallEnd = start;
  let hasElseLink = false;
  let currentHeader = start;
  let scrutinee = null;

  while (true) {
    const headerLine = statements[currentHeader]?.trim();
    const condition = extractAnyIfCondition(headerLine);
    if (condition === null) break;

    const resolved = resolveConditionExpression(statements, currentHeader, condition);
    if (resolved === null) break;

    const parsed = parseCaseSides(resolved);
    if (!parsed) break;
    const { scrutinee: nextScrutinee, caseToken } = parsed;

    const caseValue = resolveCaseValue(statements, currentHeader, caseToken);
    if (caseValue === null || !isLiteral(caseValue)) break;

    if (scrutinee !== null && scrutinee !== nextScrutinee) break;
    if (scrutinee === null) scrutinee = nextScrutinee;

    const blockEnd = findBlockEnd(statements, currentHeader);
    if (blockEnd < 0) break;
    overallEnd = Math.max(overallEnd, blockEnd);

    const body = statements.slice(currentHeader + 1, blockEnd);
    cases.push({ value: caseValue, body });
    scrutinee = nextScrutinee;

    // Look for next branch
    const { nextIndex } = collectTrivia(statements, blockEnd + 1);
    if (nextIndex >= statements.length) break;

    const nextLine = statements[nextIndex].trim();
    if (isElseIfOpen(nextLine)) {
      hasElseLink = true;
      currentHeader = nextIndex;
      continue;
    }
    if (isElseOpen(nextLine)) {
      hasElseLink = true;
      const elseEnd = findBlockEnd(statements, nextIndex);
      if (elseEnd < 0) break;
      overallEnd = Math.max(overallEnd, elseEnd);
      defaultBody = statements.slice(nextIndex + 1, elseEnd);
      break;
    }

    // Consecutive standalone if comparing same scrutinee
    const nextIfHeader = findNextIfAfterCasePrelude(statements, blockEnd + 1);
    if (nextIfHeader >= 0) {
      const nextIfLine = statements[nextIfHeader].trim();
      const nextCond = extractAnyIfCondition(nextIfLine);
      if (nextCond !== null) {
        const nextResolved = resolveConditionExpression(statements, nextIfHeader, nextCond);
        if (nextResolved !== null) {
          const peek = parseCaseSides(nextResolved);
          if (peek && peek.scrutinee === scrutinee) {
            currentHeader = nextIfHeader;
            continue;
          }
        }
      }
    }
    break;
  }

  if (scrutinee === null) return null;
  const minCases = hasElseLink ? 2 : 3;
  if (cases.length < minCases) return null;

  // Check case value uniqueness
  const seen = new Set();
  for (const c of cases) {
    if (seen.has(c.value)) return null;
    seen.add(c.value);
  }

  // Consecutive standalone `if` blocks (no `else` links) only match `switch`
  // semantics when at most one case can run. If a case body reassigns the
  // scrutinee, a later `scrutinee == k` could also fire (a state-machine
  // step), so a switch would assert wrong exclusivity. Require every case
  // body to be provably exclusive — ends in a terminator or never reassigns
  // the scrutinee — else leave the if-chain intact. Mirrors the Rust port.
  if (!hasElseLink && !cases.every((c) => caseBodyIsSwitchSafe(c.body, scrutinee))) {
    return null;
  }

  const output = [`switch ${scrutinee} {`];
  for (const c of cases) {
    output.push(`case ${c.value} {`);
    output.push(...c.body);
    output.push("}");
  }
  if (defaultBody !== null) {
    output.push("default {");
    output.push(...defaultBody);
    output.push("}");
  }
  output.push("}");
  return { replacement: output, end: overallEnd };
}

// A standalone-`if` case body is safe to fold into a `switch` only when it
// cannot fall through into a later case's comparison: either it ends in a
// terminator, or it never reassigns the scrutinee. Mirrors the Rust port's
// `case_body_is_switch_safe`.
function caseBodyIsSwitchSafe(body, scrutinee) {
  if (bodyEndsWithTerminator(body)) return true;
  return !body.some((line) => statementReassigns(line, scrutinee));
}

function bodyEndsWithTerminator(body) {
  for (let i = body.length - 1; i >= 0; i--) {
    const trimmed = body[i].trim();
    if (trimmed === "" || trimmed.startsWith("//") || trimmed === "{" || trimmed === "}") {
      continue;
    }
    return isTerminatorStatement(trimmed);
  }
  return false;
}

function isTerminatorStatement(line) {
  const t = line.trim();
  return (
    t === "return;" ||
    t.startsWith("return ") ||
    t.startsWith("throw") ||
    t.startsWith("abort") ||
    t.startsWith("goto ") ||
    t === "break;" ||
    t === "continue;"
  );
}

function statementReassigns(line, scrutinee) {
  const assign = parseAssignment(line);
  return assign !== null && assign.lhs === scrutinee;
}

function resolveConditionExpression(statements, headerIndex, condition) {
  if (condition.includes("==")) return condition.trim();
  let c = condition.trim();
  if (c.startsWith("!")) c = c.slice(1).trim();
  const prev = prevCodeLine(statements, headerIndex);
  if (prev < 0) return null;
  const assign = parseAssignment(statements[prev]);
  if (!assign || assign.lhs !== c) return null;
  return assign.rhs;
}

function parseCaseSides(condition) {
  const eqPos = condition.indexOf("==");
  if (eqPos < 0) return null;
  const left = condition.slice(0, eqPos).trim();
  const right = condition.slice(eqPos + 2).trim();
  if (isLiteral(left) && !isLiteral(right)) return { scrutinee: right, caseToken: left };
  if (!isLiteral(left) && isLiteral(right)) return { scrutinee: left, caseToken: right };
  if (isTemp(left) && !isTemp(right)) return { scrutinee: right, caseToken: left };
  if (!isTemp(left) && isTemp(right)) return { scrutinee: left, caseToken: right };
  return null;
}

function resolveCaseValue(statements, headerIndex, token) {
  if (isLiteral(token)) return token.trim();
  if (!isTemp(token)) return null;
  let cursor = headerIndex;
  while (true) {
    const prev = prevCodeLine(statements, cursor);
    if (prev < 0) return null;
    cursor = prev;
    const assign = parseAssignment(statements[prev]);
    if (!assign || assign.lhs !== token) continue;
    return isLiteral(assign.rhs) ? assign.rhs.trim() : null;
  }
}

function isLiteral(value) {
  const v = value.trim();
  if (v === "") return false;
  if (v.startsWith('"') && v.endsWith('"') && v.length >= 2) return true;
  if (v.startsWith("'") && v.endsWith("'") && v.length >= 3) return true;
  if (v === "true" || v === "false" || v === "null") return true;
  if (v.startsWith("0x") && v.length > 2) return /^[\da-f]+$/i.test(v.slice(2));
  return /^-?\d+$/.test(v);
}

function isTemp(value) {
  const v = value.trim();
  return v.startsWith("t") && v.length > 1 && /^\d+$/.test(v.slice(1));
}

function collectTrivia(statements, start) {
  let index = start;
  while (index < statements.length) {
    if (!isBlank(statements[index])) break;
    index++;
  }
  return { nextIndex: index };
}

function findNextIfAfterCasePrelude(statements, start) {
  let index = start;
  while (index < statements.length) {
    const t = statements[index].trim();
    if (isBlank(t)) {
      index++;
      continue;
    }
    if (isIfOpen(t)) return index;
    // Only skip a genuine case-value temp that feeds the upcoming comparison
    // (referenced by the next code statement). Any other inter-case statement
    // must block the fold so it is not spliced away and silently dropped — a
    // real local assignment (`loc5 = effect();`) AND a temp that captures a
    // discarded side-effecting call (`let t7 = Foo(arg);`).
    const assign = parseAssignment(t);
    if (
      assign !== null &&
      isTempIdent(assign.lhs) &&
      tempConsumedByNextCode(statements, index, assign.lhs)
    ) {
      index++;
      continue;
    }
    return -1;
  }
  return -1;
}

// A case-value temp prelude (`tN = <value>;`) feeds the upcoming comparison, so
// `tN` is referenced by the next code statement. A temp capturing a discarded
// side-effecting call is not referenced; treat it as a real statement so the
// switch fold is blocked and it is preserved. Mirrors the Rust port.
function tempConsumedByNextCode(statements, index, temp) {
  for (let i = index + 1; i < statements.length; i++) {
    const t = statements[i].trim();
    if (t === "" || t.startsWith("//")) continue;
    return containsIdentifier(statements[i], temp);
  }
  return false;
}

// ─── Pass 18: rewrite_switch_break_gotos ───────────────────────────────────

function rewriteSwitchBreakGotos(statements) {
  let index = 0;
  while (index < statements.length) {
    if (!statements[index].trim().startsWith("switch ")) {
      index++;
      continue;
    }
    const end = findBlockEnd(statements, index);
    if (end < 0) {
      index++;
      continue;
    }

    const labelIdx = nextCodeLine(statements, end + 1);
    if (labelIdx < 0) {
      index = end + 1;
      continue;
    }
    const labelTrimmed = statements[labelIdx].trim();
    if (!labelTrimmed.endsWith(":")) {
      index = end + 1;
      continue;
    }
    const label = labelTrimmed.slice(0, -1);
    if (!label.startsWith("label_")) {
      index = end + 1;
      continue;
    }

    const gotoTarget = `goto ${label};`;
    for (let i = index + 1; i < end; i++) {
      if (statements[i].trim() === gotoTarget) {
        const indent = leadingWhitespace(statements[i]);
        statements[i] = `${indent}break;`;
      }
    }
    index = end + 1;
  }
}

// ─── Pass 8b: inline_single_use_temps (optional) ─────────────────────────────

function isSafeToInline(expr) {
  if (expr.includes("(")) {
    const trimmed = expr.trim();
    if (trimmed.startsWith("(") && trimmed.endsWith(")")) return true;
    return false;
  }
  return true;
}

function needsParens(expr) {
  // Check for operator characters outside of string literals and bracket indexing.
  let depth = 0;
  let inString = false;
  let quote = "";
  for (let i = 0; i < expr.length; i++) {
    const ch = expr[i];
    if (inString) {
      if (ch === "\\" && i + 1 < expr.length) {
        i++;
      } else if (ch === quote) {
        inString = false;
      }
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
    if (
      ch === "+" || ch === "-" || ch === "*" ||
      ch === "/" || ch === "%" ||
      ch === "<" || ch === ">"
    ) return true;
    if (ch === "&" && i + 1 < expr.length && expr[i + 1] === "&") return true;
    if (ch === "|" && i + 1 < expr.length && expr[i + 1] === "|") return true;
    if (ch === "=" && i + 1 < expr.length && expr[i + 1] === "=") return true;
    if (ch === "!" && i + 1 < expr.length && expr[i + 1] === "=") return true;
  }
  return false;
}

function isControlFlowCondition(statement) {
  const t = statement.trim();
  return (
    t.startsWith("if ") ||
    t.startsWith("while ") ||
    t.startsWith("for ") ||
    t.startsWith("} else if ")
  );
}

function isNumericLiteral(text) {
  const t = text.startsWith("-") ? text.slice(1) : text;
  if (t.startsWith("0x") || t.startsWith("0X")) {
    const hex = t.slice(2);
    return hex.length > 0 && /^[0-9a-fA-F]+$/.test(hex);
  }
  return t.length > 0 && /^\d+$/.test(t);
}

function isStringLiteral(text) {
  if (text.length < 2) return false;
  return (
    (text[0] === '"' && text[text.length - 1] === '"') ||
    (text[0] === "'" && text[text.length - 1] === "'")
  );
}

function isSimpleIdentifier(text) {
  if (text.length === 0) return false;
  if (!/^[A-Za-z_]/.test(text)) return false;
  return /^\w+$/.test(text);
}

function isTrivialInlineRhs(expr) {
  const t = expr.trim();
  if (t === "") return false;
  if (t === "true" || t === "false" || t === "null") return true;
  if (isNumericLiteral(t) || isStringLiteral(t)) return true;
  return isSimpleIdentifier(t);
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
      if (matches !== null) {
        for (let m = 0; m < matches.length; m++) {
          const v = matches[m];
          if (known.has(v)) {
            useCounts.set(v, (useCounts.get(v) || 0) + 1);
          }
        }
      }
    }

    if (assign) {
      if (!isTempIdent(assign.lhs)) continue;

      if (assign.hasLet) {
        if (definitions.has(assign.lhs)) {
          reassigned.add(assign.lhs);
        } else {
          known.add(assign.lhs);
          definitions.set(assign.lhs, { defLine: idx, rhs: assign.rhs });
        }
      } else {
        reassigned.add(assign.lhs);
      }
    }
  }

  const candidates = [];
  for (const [name, { defLine, rhs }] of definitions) {
    const count = useCounts.get(name) || 0;
    if (count === 1 && !reassigned.has(name) && isSafeToInline(rhs)) {
      candidates.push({ name, defLine, rhs });
    }
  }

  candidates.sort((a, b) => b.defLine - a.defLine);
  return candidates;
}

function applyInlining(statements, candidates) {
  for (const candidate of candidates) {
    let inlined = false;
    for (let i = candidate.defLine + 1; i < statements.length; i++) {
      if (!containsIdentifier(statements[i], candidate.name)) continue;

      if (
        isControlFlowCondition(statements[i]) &&
        !isTrivialInlineRhs(candidate.rhs)
      ) {
        break;
      }

      const replacement = needsParens(candidate.rhs)
        ? `(${candidate.rhs})`
        : candidate.rhs;

      const updated = replaceIdentifier(
        statements[i],
        candidate.name,
        replacement,
      );
      if (updated !== statements[i]) {
        statements[i] = updated;
        inlined = true;
        break;
      }
    }

    if (inlined) {
      statements[candidate.defLine] = "";
    }
  }
}

function inlineSingleUseTemps(statements) {
  const candidates = collectInlineCandidates(statements);
  applyInlining(statements, candidates);
}

// ─── Main entry point ──────────────────────────────────────────────────────

/**
 * Apply all post-processing passes to lifted statements.
 * Pass order MUST match Rust's HighLevelEmitter::finish().
 *
 * @param {string[]} statements - flat list of pseudo-code lines (mutated in place)
 * @param {object} [options] - optional settings
 * @param {boolean} [options.inlineSingleUseTemps] - enable single-use temp inlining (default: false)
 */
export function postprocess(statements, options = {}) {
  // Pass 1
  rewriteElseIfChains(statements);
  // Pass 2
  collapseOverflowChecks(statements);
  // Pass 3
  rewriteGotoDoWhile(statements);
  // Pass 4
  rewriteIfGotoToWhile(statements);
  // Pass 5
  eliminateFallthroughGotos(statements);
  // Pass 5a: lift `label_X: ... goto label_X;` to `loop { ... }`
  rewriteLabelGotoToLoop(statements);
  // Pass 5b: remove orphaned labels (labels with no matching goto)
  removeOrphanedLabels(statements);
  // Pass 6
  rewriteForLoops(statements);
  // Pass 7
  inlineConditionTemps(statements);
  // Pass 8
  inlineForIncrementTemps(statements);
  // Pass 8b (optional, matches Rust inline_single_use_temps)
  if (options.inlineSingleUseTemps) {
    inlineSingleUseTemps(statements);
    // Inlining can collapse the body that was sitting between a
    // `leave/goto LABEL;` and its `LABEL:` target, turning the
    // previously-preserved transfer into a now-eliminable
    // fallthrough. Re-run elimination + orphan-label cleanup so the
    // pair drops out instead of sticking around in clean output.
    // Mirrors the Rust core.rs pass order.
    eliminateFallthroughGotos(statements);
    removeOrphanedLabels(statements);
  }
  // Pass 9
  rewriteCompoundAssignments(statements);
  // Pass 10
  rewriteIndexingSyntax(statements);
  // Pass 11
  collapseIfTrue(statements);
  // Pass 12
  invertEmptyIfElse(statements);
  // Pass 13
  removeEmptyIf(statements);
  // Pass 14
  stripStackComments(statements);
  // Pass 15
  eliminateIdentityTemps(statements);
  // Pass 16
  collapseTempIntoStore(statements);
  // Pass 17
  rewriteSwitchStatements(statements);
  // Pass 18
  rewriteSwitchBreakGotos(statements);

  // Final cleanup: remove blank lines
  // (matching Rust: self.statements.retain(|line| !line.trim().is_empty()))
  let write = 0;
  for (let read = 0; read < statements.length; read++) {
    if (statements[read].trim() !== "") {
      statements[write++] = statements[read];
    }
  }
  statements.length = write;
}
