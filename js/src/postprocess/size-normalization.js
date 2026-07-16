// Collapse compiler-generated SIZE-guarded signed integer normalization.
//
// The linear lifter represents the small-value path as an unassigned `null`
// temporary and only initializes it inside the `SIZE` branch.  The compiler
// sequence is an unconditional width mask/sign extension; keeping that
// meaning in the postprocessed C# surface also removes the dead `null` path.

import {
  findMatchingBrace,
  leadingWhitespace,
  nextCodeLine,
  parseAssignment,
  prevCodeLine,
} from "./helpers.js";

const NORMALIZATIONS = new Map([
  ["4", { mask: "4294967295", maximum: "2147483647", modulus: "4294967296" }],
  ["8", {
    mask: "18446744073709551615",
    maximum: "9223372036854775807",
    modulus: "18446744073709551616",
  }],
]);

/**
 * Rewrite exact `let result = null; if len(value) > width { ... }` wrappers.
 * The matcher requires the complete mask/sign-extension shape so ordinary
 * length checks and user-authored branches are left untouched.
 */
export function collapseSizeNormalizations(statements) {
  let index = 0;
  while (index < statements.length) {
    const match = tryMatchSizeNormalization(statements, index);
    if (!match) {
      index += 1;
      continue;
    }

    const { declarationIndex, branchIndex, branchEnd, destination, value, normalization } = match;
    const declarationIndent = leadingWhitespace(statements[declarationIndex]);
    statements[declarationIndex] =
      `${declarationIndent}let ${destination} = ${value} & ${normalization.mask};`;

    const branchIndent = leadingWhitespace(statements[branchIndex]);
    const bodyIndent = `${branchIndent}    `;
    statements.splice(
      branchIndex,
      branchEnd - branchIndex + 1,
      `${branchIndent}if ${destination} > ${normalization.maximum} {`,
      `${bodyIndent}${destination} = ${destination} - ${normalization.modulus};`,
      `${branchIndent}}`,
    );
    index = declarationIndex + 1;
  }
}

function tryMatchSizeNormalization(statements, branchIndex) {
  const branch = statements[branchIndex]?.trim() ?? "";
  const header = branch.match(/^if\s+len\(([^()]+)\)\s*>\s*(4|8)\s*\{$/);
  if (!header) return null;

  const value = header[1].trim();
  const normalization = NORMALIZATIONS.get(header[2]);
  if (!normalization || !/^[A-Za-z_]\w*$/u.test(value)) return null;

  const declarationIndex = prevCodeLine(statements, branchIndex);
  if (declarationIndex < 0) return null;
  const declaration = parseAssignment(statements[declarationIndex]);
  if (!declaration || !declaration.hasLet || declaration.rhs !== "null") return null;
  const destination = declaration.lhs;
  if (destination === value) return null;

  const branchEnd = findMatchingBrace(statements, branchIndex);
  if (branchEnd < 0) return null;

  let cursor = nextCodeLine(statements, branchIndex + 1);
  const mask = cursor >= 0 ? parseAssignment(statements[cursor]) : null;
  if (!mask || !mask.hasLet || mask.rhs !== `${value} & ${normalization.mask}`) return null;
  const maskVariable = mask.lhs;

  cursor = nextCodeLine(statements, cursor + 1);
  const signed = cursor >= 0 ? parseAssignment(statements[cursor]) : null;
  if (!signed || !signed.hasLet || signed.rhs !== "null") return null;
  const signedVariable = signed.lhs;

  cursor = nextCodeLine(statements, cursor + 1);
  const innerHeader = cursor >= 0 ? statements[cursor].trim() : "";
  if (innerHeader !== `if ${maskVariable} > ${normalization.maximum} {`) return null;
  const innerEnd = findMatchingBrace(statements, cursor);
  if (innerEnd < 0 || innerEnd >= branchEnd) return null;

  const subtractIndex = nextCodeLine(statements, cursor + 1);
  const subtract = subtractIndex >= 0 ? parseAssignment(statements[subtractIndex]) : null;
  if (
    !subtract ||
    subtract.hasLet ||
    subtract.lhs !== signedVariable ||
    subtract.rhs !== `${maskVariable} - ${normalization.modulus}`
  ) {
    return null;
  }
  const innerClose = nextCodeLine(statements, subtractIndex + 1);
  if (innerClose !== innerEnd) return null;

  const copyIndex = nextCodeLine(statements, innerEnd + 1);
  const copy = copyIndex >= 0 ? parseAssignment(statements[copyIndex]) : null;
  if (!copy || copy.hasLet || copy.lhs !== destination || copy.rhs !== signedVariable) return null;
  if (nextCodeLine(statements, copyIndex + 1) !== branchEnd) return null;

  return {
    declarationIndex,
    branchIndex,
    branchEnd,
    destination,
    value,
    normalization,
  };
}
