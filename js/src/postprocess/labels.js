// Hot postprocess regex literals: hoisted to module level so each pass
// reuses the same compiled instance instead of relying on per-call interning.
export const GOTO_LABEL_RE = /^goto\s+(label_0x[\da-f]+);$/i;
export const LEAVE_LABEL_RE = /^leave\s+(label_0x[\da-f]+);$/i;
export const LABEL_LINE_RE = /^(label_0x[\da-f]+):$/i;
export const IF_GOTO_RE = /^if\s+.+\{\s*goto\s+(label_0x[\da-f]+);\s*\}$/i;
const INLINE_IF_GOTO_RE = /^(\s*)if\s+(.+?)\s*\{\s*goto\s+(label_0x[\da-f]+);\s*\}$/i;

// Expand the compact branch form emitted by the linear lifter into the
// multiline representation consumed by brace-aware postprocess passes. This
// is deliberately limited to a single conditional goto; ordinary inline C#
// source supplied to the renderer is handled by csharp-body.js instead.
export function expandInlineConditionalGotos(statements) {
  for (let index = 0; index < statements.length; index += 1) {
    const match = INLINE_IF_GOTO_RE.exec(statements[index]);
    if (!match) continue;
    const [, indent, condition, label] = match;
    statements.splice(
      index,
      1,
      `${indent}if ${condition} {`,
      `${indent}    goto ${label};`,
      `${indent}}`,
    );
    index += 2;
  }
}

// ─── Pass 5: eliminate_fallthrough_gotos ────────────────────────────────────

export function eliminateFallthroughGotos(statements) {
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

export function rewriteLabelGotoToLoop(statements) {
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

export function removeOrphanedLabels(statements) {
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

