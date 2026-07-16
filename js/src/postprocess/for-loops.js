import {
  containsIdentifier,
  extractIfCondition,
  extractWhileCondition,
  findBlockEnd,
  isBlank,
  leadingWhitespace,
  parseAssignment,
  prevCodeLine,
  replaceIdentifier,
} from "./helpers.js";

// Rewrite lifted while loops with a nearby initializer and increment into for loops.
export function rewriteForLoops(statements) {
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

    statements[index] =
      `${leadingWhitespace(statements[index])}for (${initAssign.full}; ${condition}; ${increment.expr}) {`;
    statements[initIdx] = "";
    statements[increment.incrementIdx] = "";
    if (increment.tempIdx !== null) {
      statements[increment.tempIdx] = "";
    }
    index++;
  }
}

/**
 * Lift `loop { let x = c; if cond(x) { … update x … } }` into
 * `let x = c; while cond(x) { … }` so rewriteForLoops can promote counting
 * shapes (LoopIf-class back-edges that re-enter the initializer).
 */
export function rewriteHeaderInitLoops(statements) {
  let index = 0;
  while (index < statements.length) {
    if (statements[index].trim() !== "loop {") {
      index++;
      continue;
    }
    const loopEnd = findBlockEnd(statements, index);
    if (loopEnd < 0) {
      index++;
      continue;
    }

    const code = [];
    for (let i = index + 1; i < loopEnd; i++) {
      const t = statements[i].trim();
      if (t !== "" && !t.startsWith("//")) code.push(i);
    }
    if (code.length < 3) {
      index++;
      continue;
    }

    // Prefer a loc/arg/static zero/constant init; allow pure constant lets after it.
    let initIdx = -1;
    let initPos = -1;
    for (let p = 0; p < code.length; p++) {
      const a = parseAssignment(statements[code[p]]);
      if (
        a &&
        isNumericLiteral(a.rhs) &&
        (a.lhs.startsWith("loc") || a.lhs.startsWith("arg") || a.lhs.startsWith("static"))
      ) {
        initIdx = code[p];
        initPos = p;
        break;
      }
    }
    if (initIdx < 0) {
      index++;
      continue;
    }

    let ifIdx = -1;
    for (let p = initPos + 1; p < code.length; p++) {
      if (statements[code[p]].trim().startsWith("if ")) {
        ifIdx = code[p];
        break;
      }
    }
    if (ifIdx < 0) {
      index++;
      continue;
    }

    // Everything between init and if must be pure constant lets.
    let betweenOk = true;
    for (let p = initPos + 1; p < code.length && code[p] !== ifIdx; p++) {
      const a = parseAssignment(statements[code[p]]);
      if (!a || !isNumericLiteral(a.rhs)) {
        betweenOk = false;
        break;
      }
    }
    if (!betweenOk) {
      index++;
      continue;
    }

    const init = parseAssignment(statements[initIdx]);
    const condition = extractIfCondition(statements[ifIdx]);
    if (!init || condition === null || !conditionMentionsIdent(condition, init.lhs)) {
      index++;
      continue;
    }

    const ifEnd = findBlockEnd(statements, ifIdx);
    if (ifEnd < 0 || code[code.length - 1] !== ifEnd) {
      index++;
      continue;
    }

    // Body of the if must update the induction variable.
    let bodyUpdates = false;
    for (let i = ifIdx + 1; i < ifEnd; i++) {
      const t = statements[i].trim();
      const a = parseAssignment(t);
      if (a && a.lhs === init.lhs) {
        bodyUpdates = true;
        break;
      }
      if (
        t.startsWith(`${init.lhs} +=`) ||
        t.startsWith(`${init.lhs} -=`) ||
        t.startsWith(`${init.lhs}++`) ||
        t.startsWith(`${init.lhs}--`)
      ) {
        bodyUpdates = true;
        break;
      }
    }
    if (!bodyUpdates) {
      index++;
      continue;
    }

    const indent = statements[index].match(/^\s*/)?.[0] ?? "";
    const initLine = statements[initIdx];
    statements[index] = initLine;
    statements[initIdx] = "";
    statements[ifIdx] = `${indent}while ${condition} {`;
    statements[loopEnd] = "";
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

    // Compound forms: loc0 += 1; loc0++;
    const plusEq = line.startsWith(`${varName} += `) && line.endsWith(";");
    if (plusEq) {
      const rest = line.slice(varName.length + 4, -1).trim();
      if (rest) return { incrementIdx: index, tempIdx: null, expr: `${varName} += ${rest}` };
    }
    const minusEq = line.startsWith(`${varName} -= `) && line.endsWith(";");
    if (minusEq) {
      const rest = line.slice(varName.length + 4, -1).trim();
      if (rest) return { incrementIdx: index, tempIdx: null, expr: `${varName} -= ${rest}` };
    }
    if (line === `${varName}++;` || line === `++${varName};`) {
      return { incrementIdx: index, tempIdx: null, expr: `${varName}++` };
    }
    if (line === `${varName}--;` || line === `--${varName};`) {
      return { incrementIdx: index, tempIdx: null, expr: `${varName}--` };
    }

    const assign = parseAssignment(line);
    if (!assign || assign.lhs !== varName) return null;

    // Prefer folding pure constant temps before accepting raw form.
    const prevIdx = prevCodeLine(statements, index);
    if (prevIdx >= 0) {
      const prevAssign = parseAssignment(statements[prevIdx]);
      if (prevAssign) {
        if (prevAssign.lhs === assign.rhs) {
          return {
            incrementIdx: index,
            tempIdx: prevIdx,
            expr: `${varName} = ${prevAssign.rhs}`,
          };
        }
        if (
          containsIdentifier(assign.rhs, prevAssign.lhs) &&
          isNumericLiteral(prevAssign.rhs)
        ) {
          const replaced = replaceIdentifier(assign.rhs, prevAssign.lhs, prevAssign.rhs);
          return {
            incrementIdx: index,
            tempIdx: prevIdx,
            expr: `${varName} = ${replaced}`,
          };
        }
      }
    }

    if (assign.rhs.startsWith(varName)) {
      return { incrementIdx: index, tempIdx: null, expr: assign.full };
    }
    return null;
  }
  return null;
}

function isNumericLiteral(value) {
  const v = String(value ?? "").trim();
  if (!v) return false;
  const digits = v.startsWith("-") ? v.slice(1) : v;
  return digits.length > 0 && /^\d+$/.test(digits);
}

function conditionMentionsIdent(condition, ident) {
  return condition
    .split(/[^A-Za-z0-9_]+/)
    .some((token) => token === ident);
}
