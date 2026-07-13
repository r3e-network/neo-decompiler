/** Temporary-value cleanup passes shared by the final postprocessor. */
import {
  containsIdentifier,
  isTempIdent,
  leadingWhitespace,
  nextCodeLine,
  parseAssignment,
  replaceIdentifier,
} from "./helpers.js";

const TEMP_TOKEN_RE = /\bt\d+\b/g;

export function eliminateIdentityTemps(statements) {
  const firstSeen = new Map();
  for (let i = 0; i < statements.length; i++) {
    const matches = statements[i].trim().match(TEMP_TOKEN_RE);
    if (!matches) continue;
    for (const token of matches) {
      if (!firstSeen.has(token)) firstSeen.set(token, i);
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
    if (assign.lhs === assign.rhs) {
      statements[index] = "";
      index++;
      continue;
    }
    if (firstSeen.get(assign.lhs) < index) {
      index++;
      continue;
    }
    for (let j = index + 1; j < statements.length; j++) {
      if (containsIdentifier(statements[j], assign.lhs)) {
        statements[j] = replaceIdentifier(statements[j], assign.lhs, assign.rhs);
      }
    }
    statements[index] = "";
    index++;
  }
}

export function collapseTempIntoStore(statements) {
  const tempLineCounts = new Map();
  for (const statement of statements) {
    const matches = statement.trim().match(TEMP_TOKEN_RE);
    if (!matches) continue;
    for (const token of new Set(matches)) {
      tempLineCounts.set(token, (tempLineCounts.get(token) || 0) + 1);
    }
  }

  let index = 0;
  while (index + 1 < statements.length) {
    const trimmed = statements[index].trim();
    if (!trimmed.startsWith("let ")) {
      index++;
      continue;
    }
    const first = parseAssignment(trimmed);
    if (!first || !isTempIdent(first.lhs)) {
      index++;
      continue;
    }
    const next = nextCodeLine(statements, index + 1);
    if (next < 0) {
      index++;
      continue;
    }
    const temp = first.lhs;
    const nextText = statements[next].trim();
    const second = parseAssignment(nextText);
    if (second && second.rhs === temp && (tempLineCounts.get(temp) || 0) <= 2) {
      const indent = leadingWhitespace(statements[next]);
      const prefix = second.hasLet ? "let " : "";
      statements[next] = `${indent}${prefix}${second.lhs} = ${first.rhs};`;
      statements[index] = "";
      index = next + 1;
      continue;
    }
    if (nextText === `return ${temp};` && (tempLineCounts.get(temp) || 0) <= 2) {
      const indent = leadingWhitespace(statements[next]);
      statements[next] = `${indent}return ${first.rhs};`;
      statements[index] = "";
      index = next + 1;
      continue;
    }
    index++;
  }
}
