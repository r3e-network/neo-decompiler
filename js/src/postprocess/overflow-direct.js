// Collapse the compact overflow branch chain emitted by the linear lifter.

import {
  escapeRegex,
  leadingWhitespace,
  nextCodeLine,
  parseAssignment,
} from "./helpers.js";

const SIGNED_OVERFLOW_RANGES = new Map([
  ["-2147483648", { upper: "2147483647", mask: "4294967295", subtract: "4294967296" }],
  ["-9223372036854775808", {
    upper: "9223372036854775807",
    mask: "18446744073709551615",
    subtract: "18446744073709551616",
  }],
]);

/**
 * Match the compact branch chain produced by the linear JS lifter when a
 * compiler-generated integer normalization is not recognized as a structured
 * branch. The chain is equivalent to a range guard followed by the mask and,
 * for signed ranges, sign extension. Require every label and assignment in
 * the chain so ordinary user gotos remain untouched.
 */
export function tryMatchDirectOverflow(statements, idx) {
  const operation = parseAssignment(statements[idx]);
  if (!operation || !operation.hasLet || !isIdentifier(operation.lhs)) return null;

  let firstIdx = nextCodeLine(statements, idx + 1);
  // A DUP of an ambient stack value can leave one unrelated alias between the
  // arithmetic result and its range checks (common inside finally blocks).
  // Keep that declaration in place, but allow the proven chain after it to be
  // structured. Do not skip more than one line or any control statement.
  const alias = firstIdx === -1 ? null : parseAssignment(statements[firstIdx]);
  if (alias?.hasLet && alias.lhs !== operation.lhs) {
    const candidate = nextCodeLine(statements, firstIdx + 1);
    if (candidate !== -1 && readConditionalGoto(statements, candidate)) {
      firstIdx = candidate;
    }
  }
  const first = firstIdx === -1 ? null : readConditionalGoto(statements, firstIdx);
  if (!first) return null;
  const lower = parseComparison(first.condition, operation.lhs);
  if (!lower || lower.operator !== ">=") return null;

  const signedRange = SIGNED_OVERFLOW_RANGES.get(lower.bound);
  const secondIdx = nextCodeLine(statements, first.end + 1);
  const bypass = secondIdx === -1 ? null : parseGoto(statements[secondIdx]);
  if (!bypass) return null;
  const firstLabelIdx = nextCodeLine(statements, secondIdx + 1);
  if (firstLabelIdx === -1 || statements[firstLabelIdx].trim() !== `${first.target}:`) {
    return null;
  }

  const upperCheckIdx = nextCodeLine(statements, firstLabelIdx + 1);
  const upperCheck = upperCheckIdx === -1 ? null : readConditionalGoto(statements, upperCheckIdx);
  if (!upperCheck) return null;
  const upper = parseComparison(upperCheck.condition, operation.lhs);
  if (!upper || upper.operator !== "<=") return null;
  if (signedRange && upper.bound !== signedRange.upper) return null;

  const bypassLabelIdx = nextCodeLine(statements, upperCheck.end + 1);
  if (bypassLabelIdx === -1 || statements[bypassLabelIdx].trim() !== `${bypass.target}:`) {
    return null;
  }

  const maskIdx = nextCodeLine(statements, bypassLabelIdx + 1);
  if (maskIdx === -1) return null;
  const maskAssignment = parseAssignment(statements[maskIdx]);
  if (!maskAssignment) return null;
  const mask = parseMaskExpression(maskAssignment.rhs, operation.lhs);
  if (!mask) return null;

  const range = signedRange ?? {
    upper: upper.bound,
    mask: upper.bound,
    subtract: null,
  };
  if (mask.mask !== range.mask) return null;

  let end;
  let replacement;
  if (!maskAssignment.hasLet) {
    if (maskAssignment.lhs !== operation.lhs) return null;
    end = nextCodeLine(statements, maskIdx + 1);
    if (end === -1 || statements[end].trim() !== `${upperCheck.target}:`) return null;
    replacement = {
      kind: "direct",
      mode: "mask",
      operation,
      lower: lower.bound,
      upper: range.upper,
      maskLine: statements[maskIdx].trim(),
      chainStart: firstIdx,
      end,
    };
  } else {
    if (!isIdentifier(maskAssignment.lhs)) return null;
    const copyIdx = nextCodeLine(statements, maskIdx + 1);
    const copy = copyIdx === -1 ? null : parseAssignment(statements[copyIdx]);
    if (!copy || copy.hasLet || copy.lhs !== operation.lhs || copy.rhs !== maskAssignment.lhs) {
      return null;
    }
    const maskedUpperIdx = nextCodeLine(statements, copyIdx + 1);
    const maskedUpper = maskedUpperIdx === -1 ? null : readConditionalGoto(statements, maskedUpperIdx);
    if (!maskedUpper) return null;
    const maskedComparison = parseComparison(maskedUpper.condition, maskAssignment.lhs);
    if (
      !maskedComparison ||
      maskedComparison.operator !== "<=" ||
      maskedComparison.bound !== range.upper ||
      maskedUpper.target !== upperCheck.target
    ) {
      return null;
    }
    if (!range.subtract) return null;
    const subtractIdx = nextCodeLine(statements, maskedUpper.end + 1);
    const subtract = subtractIdx === -1 ? null : parseAssignment(statements[subtractIdx]);
    if (!subtract || subtract.hasLet || subtract.lhs !== operation.lhs) return null;
    if (subtract.rhs !== `${maskAssignment.lhs} - ${range.subtract}`) return null;
    end = nextCodeLine(statements, subtractIdx + 1);
    if (end === -1 || statements[end].trim() !== `${upperCheck.target}:`) return null;
    replacement = {
      kind: "direct",
      mode: "signed",
      operation,
      lower: lower.bound,
      upper: range.upper,
      maskLine: statements[maskIdx].trim(),
      copyLine: statements[copyIdx].trim(),
      maskedUpper: maskAssignment.lhs,
      subtractLine: statements[subtractIdx].trim(),
      chainStart: firstIdx,
      end,
    };
  }

  if (
    countGotoReferences(statements, first.target) !== 1 ||
    countGotoReferences(statements, bypass.target) !== 1 ||
    countGotoReferences(statements, upperCheck.target) !== (replacement.mode === "signed" ? 2 : 1)
  ) {
    return null;
  }
  return replacement;
}

export function applyDirectOverflowCollapse(statements, collapse) {
  const indent = leadingWhitespace(statements[collapse.chainStart]);
  const lines = [
    `${indent}if ${collapse.operation.lhs} < ${collapse.lower} || ${collapse.operation.lhs} > ${collapse.upper} {`,
    `${indent}    ${collapse.maskLine}`,
  ];
  if (collapse.mode === "signed") {
    lines.push(
      `${indent}    ${collapse.copyLine}`,
      `${indent}    if ${collapse.maskedUpper} > ${collapse.upper} {`,
      `${indent}        ${collapse.subtractLine}`,
      `${indent}    }`,
    );
  }
  lines.push(`${indent}}`);
  statements.splice(collapse.chainStart, collapse.end - collapse.chainStart + 1, ...lines);
}

function readConditionalGoto(statements, start) {
  const line = statements[start]?.trim() ?? "";
  const inline = line.match(/^if\s+(.+?)\s*\{\s*goto\s+(label_0x[\da-f]+);\s*\}$/i);
  if (inline) {
    return { condition: inline[1].trim(), target: inline[2], end: start };
  }
  if (!line.startsWith("if ") || !line.endsWith("{")) return null;
  const gotoIdx = nextCodeLine(statements, start + 1);
  if (gotoIdx === -1) return null;
  const goto = parseGoto(statements[gotoIdx]);
  if (!goto) return null;
  const closeIdx = nextCodeLine(statements, gotoIdx + 1);
  if (closeIdx === -1 || statements[closeIdx].trim() !== "}") return null;
  return {
    condition: line.slice(3, -1).trim(),
    target: goto.target,
    end: closeIdx,
  };
}

function parseGoto(line) {
  const match = line.trim().match(/^goto\s+(label_0x[\da-f]+);$/i);
  return match ? { target: match[1] } : null;
}

function parseComparison(condition, variable) {
  const escaped = escapeRegex(variable);
  const match = condition.match(
    new RegExp(`^\\(*\\s*${escaped}\\s*\\)*\\s*(>=|<=|>|<|==|!=)\\s*(-?\\d+)\\s*$`),
  );
  return match ? { operator: match[1], bound: match[2] } : null;
}

function parseMaskExpression(rhs, variable) {
  const escaped = escapeRegex(variable);
  const match = rhs.match(new RegExp(`^\\(*\\s*${escaped}\\s*\\)*\\s*&\\s*(\\d+)\\s*$`));
  return match ? { mask: match[1] } : null;
}

function countGotoReferences(statements, label) {
  const escaped = escapeRegex(label);
  const pattern = new RegExp(`\\bgoto\\s+${escaped};`, "gi");
  return statements.reduce((count, line) => count + (line.match(pattern)?.length ?? 0), 0);
}

function isIdentifier(value) {
  return /^[A-Za-z_]\w*$/.test(value);
}
