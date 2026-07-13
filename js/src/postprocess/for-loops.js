import {
  containsIdentifier,
  extractWhileCondition,
  findBlockEnd,
  isBlank,
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

    statements[index] = `for (${initAssign.full}; ${condition}; ${increment.expr}) {`;
    statements[initIdx] = "";
    statements[increment.incrementIdx] = "";
    if (increment.tempIdx !== null) {
      statements[increment.tempIdx] = "";
    }
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
    const assign = parseAssignment(line);
    if (!assign || assign.lhs !== varName) return null;

    if (assign.rhs.startsWith(varName)) {
      return { incrementIdx: index, tempIdx: null, expr: assign.full };
    }
    const prevIdx = prevCodeLine(statements, index);
    if (prevIdx < 0) return null;
    const prevAssign = parseAssignment(statements[prevIdx]);
    if (!prevAssign) return null;

    if (prevAssign.lhs === assign.rhs) {
      return {
        incrementIdx: index,
        tempIdx: prevIdx,
        expr: `${varName} = ${prevAssign.rhs}`,
      };
    }
    if (containsIdentifier(assign.rhs, prevAssign.lhs)) {
      const replaced = replaceIdentifier(assign.rhs, prevAssign.lhs, prevAssign.rhs);
      return {
        incrementIdx: index,
        tempIdx: prevIdx,
        expr: `${varName} = ${replaced}`,
      };
    }
    return null;
  }
  return null;
}
