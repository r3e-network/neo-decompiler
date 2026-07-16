// Collapse the tail of unchecked signed-negation normalization.
//
// The linear lifter can split the compiler's `DUP; compare min; JMPNE;
// RET; NEGATE; RET` tail at a detached method boundary. Recover the exact
// value-based branch while the destination variable is still visible.

import {
  findMatchingBrace,
  leadingWhitespace,
  nextCodeLine,
} from "./helpers.js";

const SIGNED_MINIMUMS = new Set([
  "-2147483648",
  "-9223372036854775808",
]);

export function collapseNegateNormalization(statements) {
  let index = 0;
  while (index < statements.length) {
    const match = tryMatchNegateNormalization(statements, index);
    if (!match) {
      index += 1;
      continue;
    }

    const indent = leadingWhitespace(statements[index]);
    const bodyIndent = `${indent}    `;
    statements.splice(
      index,
      match.end - index + 1,
      `${indent}if ${match.variable} == ${match.minimum} {`,
      `${bodyIndent}return ${match.variable};`,
      `${indent}}`,
      `${indent}return -${match.variable};`,
    );
    index += 4;
  }
}

function tryMatchNegateNormalization(statements, index) {
  const line = statements[index]?.trim() ?? "";
  const header = line.match(/^if\s+([A-Za-z_]\w*)\s*!=\s*(-\d+)\s*\{$/u);
  if (!header || !SIGNED_MINIMUMS.has(header[2])) return null;

  const variable = header[1];
  const branchEnd = findMatchingBrace(statements, index);
  if (branchEnd < 0) return null;

  const gotoIndex = nextCodeLine(statements, index + 1);
  if (gotoIndex < 0 || !/^goto\s+label_0x[\da-f]+;$/iu.test(statements[gotoIndex].trim())) {
    return null;
  }
  if (nextCodeLine(statements, gotoIndex + 1) !== branchEnd) return null;

  const unchangedReturnIndex = nextCodeLine(statements, branchEnd + 1);
  if (
    unchangedReturnIndex < 0 ||
    statements[unchangedReturnIndex].trim() !== `return ${variable};`
  ) {
    return null;
  }

  const negatedReturnIndex = nextCodeLine(statements, unchangedReturnIndex + 1);
  if (
    negatedReturnIndex < 0 ||
    !/^return\s+-\(\?\?\?\);$/u.test(statements[negatedReturnIndex].trim())
  ) {
    return null;
  }

  return {
    end: negatedReturnIndex,
    variable,
    minimum: header[2],
  };
}
