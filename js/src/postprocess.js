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
  findMatchingBrace,
  negateCondition,
  parseAssignment,
  parseForParts,
  prevCodeLine,
  replaceIdentifier,
} from "./postprocess/helpers.js";
import {
  rewriteSwitchBreakGotos,
  rewriteSwitchStatements,
} from "./postprocess/switches.js";
import { inlineSingleUseTemps } from "./postprocess/inlining.js";
import { rewriteElseIfChains } from "./postprocess/conditionals.js";
import { collapseOverflowChecks } from "./postprocess/overflow.js";
import { rewriteGotoDoWhile, rewriteIfGotoToWhile } from "./postprocess/loops.js";
import {
  collapseIfTrue,
  invertEmptyIfElse,
  removeEmptyIf,
  stripStackComments,
  eliminateIdentityTemps,
  collapseTempIntoStore,
} from "./postprocess/cleanup.js";
import {
  rewriteCompoundAssignments,
  rewriteIndexingSyntax,
} from "./postprocess/syntax.js";

import {
  eliminateFallthroughGotos,
  expandInlineConditionalGotos,
  removeOrphanedLabels,
  rewriteLabelGotoToLoop,
} from "./postprocess/labels.js";
import { rewriteForLoops, rewriteHeaderInitLoops } from "./postprocess/for-loops.js";

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

export function postprocess(statements, options = {}) {
  // Pass 0: normalize compact conditional transfers before any pass that
  // relies on brace matching (overflow, loops, or else-if recovery).
  expandInlineConditionalGotos(statements);
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
  // Pass 16b: after temps are cleaned, lift LoopIf-class header-init loops
  // and re-run for promotion (mirrors Rust core.rs finish order).
  rewriteHeaderInitLoops(statements);
  rewriteForLoops(statements);
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
