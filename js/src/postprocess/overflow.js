import {
  findMatchingBrace,
  isBlank,
  leadingWhitespace,
  nextCodeLine,
  parseAssignment,
  isTempIdent,
} from "./helpers.js";

const OVERFLOW_BOUNDS = [
  "-2147483648",
  "0",
  "-9223372036854775808",
  "2147483647",
  "4294967295",
  "9223372036854775807",
  "18446744073709551615",
];

export function collapseOverflowChecks(statements) {
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
  let firstBody = null;
  for (let i = ifIdx + 1; i < statements.length; i++) {
    const text = statements[i].trim();
    if (!isBlank(text)) {
      firstBody = text;
      break;
    }
  }
  const isChecked = firstBody !== null && firstBody.startsWith("throw(");
  let blankEnd = ifBlockEnd;
  let elseUnwrap = null;
  const next = nextCodeLine(statements, ifBlockEnd + 1);
  if (next >= 0) {
    const nextText = statements[next].trim();
    if (nextText === "else {" || nextText === "} else {") {
      const elseEnd = findMatchingBrace(statements, next);
      if (elseEnd >= 0) {
        if (isChecked) elseUnwrap = [next, elseEnd];
        else blankEnd = elseEnd;
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

function applyOverflowCollapse(statements, collapse) {
  if (collapse.isChecked) {
    const wrapped = collapse.expr.startsWith("checked(")
      ? collapse.expr
      : `checked(${collapse.expr})`;
    statements[collapse.opLine] = `${leadingWhitespace(statements[collapse.opLine])}let ${collapse.resultVar} = ${wrapped};`;
  }
  for (let i = collapse.blankStart; i <= collapse.blankEnd; i++) statements[i] = "";
  if (collapse.elseUnwrap) {
    statements[collapse.elseUnwrap[0]] = "";
    statements[collapse.elseUnwrap[1]] = "";
  }
  if (!collapse.isChecked) {
    const next = nextCodeLine(statements, collapse.blankEnd + 1);
    if (next >= 0) {
      const line = statements[next].trim();
      if (!line.startsWith("let ") && !line.startsWith("if ") && !line.startsWith("//")) {
        const assignment = parseAssignment(line);
        if (assignment && isTempIdent(assignment.rhs) && assignment.rhs !== collapse.resultVar) {
          statements[next] = `${leadingWhitespace(statements[next])}${assignment.lhs} = ${collapse.resultVar};`;
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
