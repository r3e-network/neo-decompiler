// Recover a compiler-generated signed increment that was split across a
// `for` header and its body by the linear loop lifter.

import {
  findBlockEnd,
  leadingWhitespace,
  parseAssignment,
} from "./helpers.js";

const NORMALIZATIONS = new Map([
  ["4294967296", { mask: "4294967295", maximum: "2147483647" }],
  ["18446744073709551616", {
    mask: "18446744073709551615",
    maximum: "9223372036854775807",
  }],
]);

/**
 * Rewrite only `for (...; ...; index = masked - modulus)` loops whose body
 * computes `masked = index + 1 & mask`. The explicit update belongs in the
 * loop body so the sign-extension subtraction remains conditional.
 */
export function collapseLoopNormalizations(statements) {
  let index = 0;
  while (index < statements.length) {
    const match = tryMatchLoopNormalization(statements, index);
    if (!match) {
      index += 1;
      continue;
    }

    const bodyIndent = leadingWhitespace(statements[match.maskIndex]);
    statements.splice(
      match.maskIndex + 1,
      0,
      `${bodyIndent}if ${match.temporary} > ${match.normalization.maximum} {`,
      `${bodyIndent}    ${match.temporary} -= ${match.modulus};`,
      `${bodyIndent}}`,
      `${bodyIndent}${match.indexVariable} = ${match.temporary};`,
    );

    const indent = leadingWhitespace(statements[index]);
    statements[index] = `${indent}let ${match.indexVariable} = ${match.initializer};`;
    statements.splice(index + 1, 0, `${indent}while ${match.condition} {`);
    index += 2;
  }
}

function tryMatchLoopNormalization(statements, index) {
  const line = statements[index]?.trim() ?? "";
  if (!line.startsWith("for (") || !line.endsWith(" {") || !line.endsWith(") {")) {
    return null;
  }
  let inner = line.slice(5, -2).trim();
  if (inner.endsWith(")")) inner = inner.slice(0, -1).trim();
  const parts = inner.split(";");
  if (parts.length !== 3) return null;

  const init = parts[0].trim().match(/^let\s+([A-Za-z_]\w*)\s*=\s*(.+)$/u);
  if (!init) return null;
  const indexVariable = init[1];
  const condition = parts[1].trim();
  const increment = parts[2].trim();
  const incrementMatch = increment.match(
    new RegExp(`^${escapeRegex(indexVariable)}\\s*=\\s*([A-Za-z_]\\w*)\\s*-\\s*(\\d+)$`, "u"),
  );
  if (!incrementMatch) return null;

  const temporary = incrementMatch[1];
  const modulus = incrementMatch[2];
  const normalization = NORMALIZATIONS.get(modulus);
  if (!normalization) return null;

  const end = findBlockEnd(statements, index);
  if (end < 0) return null;

  let operationIndex = -1;
  let maskIndex = -1;
  for (let cursor = index + 1; cursor < end; cursor += 1) {
    const assignment = parseAssignment(statements[cursor]);
    if (!assignment) continue;
    if (
      operationIndex < 0 &&
      assignment.hasLet &&
      assignment.rhs === `${indexVariable} + 1`
    ) {
      operationIndex = cursor;
      continue;
    }
    if (
      operationIndex >= 0 &&
      assignment.lhs === temporary &&
      assignment.hasLet &&
      assignment.rhs === `${parseAssignment(statements[operationIndex]).lhs} & ${normalization.mask}`
    ) {
      maskIndex = cursor;
      break;
    }
  }
  if (operationIndex < 0 || maskIndex < 0) return null;

  const operation = parseAssignment(statements[operationIndex]);
  if (!operation || statements.slice(index + 1, end).some((statement) =>
    statement.trim() === `if ${temporary} > ${normalization.maximum} {`
  )) {
    return null;
  }

  return {
    indexVariable,
    initializer: init[2],
    condition,
    temporary,
    modulus,
    normalization,
    maskIndex,
  };
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
