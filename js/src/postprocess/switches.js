// Switch reconstruction passes for the high-level postprocessor.

import {
  containsIdentifier,
  extractAnyIfCondition,
  findBlockEnd,
  isBlank,
  isElseIfOpen,
  isElseOpen,
  isIfOpen,
  isTempIdent,
  leadingWhitespace,
  nextCodeLine,
  parseAssignment,
  prevCodeLine,
} from "./helpers.js";

export function rewriteSwitchStatements(statements) {
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
    cases.push({ value: caseValue, body: statements.slice(currentHeader + 1, blockEnd) });
    scrutinee = nextScrutinee;

    const nextIndex = skipTrivia(statements, blockEnd + 1);
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

    const nextIfHeader = findNextIfAfterCasePrelude(statements, blockEnd + 1);
    if (nextIfHeader >= 0) {
      const nextCond = extractAnyIfCondition(statements[nextIfHeader].trim());
      if (nextCond !== null) {
        const nextResolved = resolveConditionExpression(statements, nextIfHeader, nextCond);
        const peek = nextResolved === null ? null : parseCaseSides(nextResolved);
        if (peek && peek.scrutinee === scrutinee) {
          currentHeader = nextIfHeader;
          continue;
        }
      }
    }
    break;
  }

  if (scrutinee === null) return null;
  const minCases = hasElseLink ? 2 : 3;
  if (cases.length < minCases) return null;
  const seen = new Set();
  for (const c of cases) {
    if (seen.has(c.value)) return null;
    seen.add(c.value);
  }
  if (!hasElseLink && !cases.every((c) => caseBodyIsSwitchSafe(c.body, scrutinee))) return null;

  const indent = leadingWhitespace(statements[start]);
  const output = [`${indent}switch ${scrutinee} {`];
  for (const c of cases) {
    output.push(`${indent}    case ${c.value} {`, ...indentBlock(c.body), `${indent}    }`);
  }
  if (defaultBody !== null) {
    output.push(`${indent}    default {`, ...indentBlock(defaultBody), `${indent}    }`);
  }
  output.push(`${indent}}`);
  return { replacement: output, end: overallEnd };
}

function indentBlock(lines) {
  return lines.map((line) => (line.trim() === "" ? line : `    ${line}`));
}

function caseBodyIsSwitchSafe(body, scrutinee) {
  if (bodyEndsWithTerminator(body)) return true;
  return !body.some((line) => statementReassigns(line, scrutinee));
}

function bodyEndsWithTerminator(body) {
  for (let i = body.length - 1; i >= 0; i--) {
    const trimmed = body[i].trim();
    if (trimmed === "" || trimmed.startsWith("//") || trimmed === "{" || trimmed === "}") continue;
    return isTerminatorStatement(trimmed);
  }
  return false;
}

function isTerminatorStatement(line) {
  const t = line.trim();
  return t === "return;" || t.startsWith("return ") || t.startsWith("throw") || t.startsWith("abort") ||
    t.startsWith("goto ") || t === "break;" || t === "continue;";
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
  return assign && assign.lhs === c ? assign.rhs : null;
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
    if (assign && assign.lhs === token) return isLiteral(assign.rhs) ? assign.rhs.trim() : null;
  }
}

function isLiteral(value) {
  const v = value.trim();
  if (v === "") return false;
  if ((v.startsWith('"') && v.endsWith('"') && v.length >= 2) ||
      (v.startsWith("'") && v.endsWith("'") && v.length >= 3)) return true;
  if (v === "true" || v === "false" || v === "null") return true;
  if (v.startsWith("0x") && v.length > 2) return /^[\da-f]+$/i.test(v.slice(2));
  return /^-?\d+$/.test(v);
}

function isTemp(value) {
  const v = value.trim();
  return v.startsWith("t") && v.length > 1 && /^\d+$/.test(v.slice(1));
}

function skipTrivia(statements, start) {
  let index = start;
  while (index < statements.length && isBlank(statements[index])) index++;
  return index;
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
    const assign = parseAssignment(t);
    if (assign !== null && isTempIdent(assign.lhs) && tempConsumedByNextCode(statements, index, assign.lhs)) {
      index++;
      continue;
    }
    return -1;
  }
  return -1;
}

function tempConsumedByNextCode(statements, index, temp) {
  for (let i = index + 1; i < statements.length; i++) {
    const t = statements[i].trim();
    if (t === "" || t.startsWith("//")) continue;
    return containsIdentifier(statements[i], temp);
  }
  return false;
}

export function rewriteSwitchBreakGotos(statements) {
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
        statements[i] = `${leadingWhitespace(statements[i])}break;`;
      }
    }
    index = end + 1;
  }
}
