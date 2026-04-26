import { stripOuterParens, wrapExpression } from "./high-level-utils.js";

const SIMPLE_CONDITIONAL_SET = new Set([
  "JMPIF",
  "JMPIF_L",
  "JMPIFNOT",
  "JMPIFNOT_L",
  "JMPEQ",
  "JMPEQ_L",
  "JMPNE",
  "JMPNE_L",
  "JMPGT",
  "JMPGT_L",
  "JMPGE",
  "JMPGE_L",
  "JMPLT",
  "JMPLT_L",
  "JMPLE",
  "JMPLE_L",
]);

export function isSimpleConditional(mnemonic) {
  return SIMPLE_CONDITIONAL_SET.has(mnemonic);
}

export function isUnconditionalJump(mnemonic) {
  return mnemonic === "JMP" || mnemonic === "JMP_L";
}

export function containsUnsupportedBranchStructure(instructions) {
  return instructions.some(
    (instruction) =>
      isSimpleConditional(instruction.opcode.mnemonic) ||
      instruction.opcode.mnemonic === "JMP" ||
      instruction.opcode.mnemonic === "JMP_L",
  );
}

export function branchTerminates(instructions) {
  const last = instructions.at(-1)?.opcode.mnemonic;
  return ["RET", "THROW", "ABORT", "ABORTMSG"].includes(last);
}

export function popConditionForBranch(stack, mnemonic) {
  switch (mnemonic) {
    case "JMPIFNOT":
    case "JMPIFNOT_L": {
      const value = stack.pop();
      return value === undefined ? null : stripOuterParens(value);
    }
    case "JMPIF":
    case "JMPIF_L": {
      const value = stack.pop();
      return value === undefined ? null : `!${wrapExpression(value)}`;
    }
    default: {
      const right = stack.pop();
      const left = stack.pop();
      if (left === undefined || right === undefined) {
        return null;
      }
      const operator = negateComparisonMnemonic(mnemonic);
      if (operator === null) {
        return null;
      }
      return `${wrapExpression(left)} ${operator} ${wrapExpression(right)}`;
    }
  }
}

export function popConditionForLoop(stack, mnemonic) {
  switch (mnemonic) {
    case "JMPIF":
    case "JMPIF_L": {
      const value = stack.pop();
      return value === undefined ? null : stripOuterParens(value);
    }
    case "JMPIFNOT":
    case "JMPIFNOT_L": {
      const value = stack.pop();
      return value === undefined ? null : `!${wrapExpression(value)}`;
    }
    default: {
      const right = stack.pop();
      const left = stack.pop();
      if (left === undefined || right === undefined) {
        return null;
      }
      const operator = originalComparisonMnemonic(mnemonic);
      if (operator === null) {
        return null;
      }
      return `${wrapExpression(left)} ${operator} ${wrapExpression(right)}`;
    }
  }
}

export function rewriteForLoops(result) {
  const statements = stripStructuredLabels(result.statements);
  // Pre-trim all statements once to avoid repeated .trim() in findMatchingClose.
  const trimmed = statements.map((s) => s.trim());
  for (let index = 1; index < statements.length - 2; index += 1) {
    const init = trimmed[index - 1];
    const whileLine = trimmed[index];
    const closeIndex = findMatchingCloseTrimmed(trimmed, index);
    if (closeIndex === -1 || closeIndex <= index + 1) {
      continue;
    }
    const increment = trimmed[closeIndex - 1];

    const initMatch = LET_INIT_RE.exec(init);
    const whileMatch = WHILE_HEADER_RE.exec(whileLine);
    const incrementMatch = SELF_INCREMENT_RE.exec(increment);
    if (!initMatch || !whileMatch || !incrementMatch) {
      continue;
    }
    const variable = initMatch[1];
    if (incrementMatch[1] !== variable) {
      continue;
    }
    if (!whileMatch[1].includes(variable)) {
      continue;
    }

    statements[index] = `for (let ${variable} = ${initMatch[2]}; ${whileMatch[1]}; ${increment.slice(0, -1)}) {`;
    trimmed[index] = statements[index];
    statements[index - 1] = "";
    trimmed[index - 1] = "";
    statements[closeIndex - 1] = "";
    trimmed[closeIndex - 1] = "";
  }
  return {
    statements: statements.filter((line) => line !== ""),
    warnings: result.warnings ?? [],
  };
}

export function collectDerivedWarnings(baseState, ...states) {
  const warnings = [...baseState.warnings];
  for (const state of states) {
    if (!state) {
      continue;
    }
    warnings.push(...state.warnings.slice(baseState.warnings.length));
  }
  return warnings;
}

const STRUCTURED_LABEL_RE = /^\s*label_0x[0-9a-f]+:\s*$/iu;
const LET_INIT_RE = /^let (\w+) = (.+);$/u;
const WHILE_HEADER_RE = /^while (.+) \{$/u;
const SELF_INCREMENT_RE = /^(\w+) = \1 \+ 1;$/u;

function stripStructuredLabels(statements) {
  const out = [];
  for (let i = 0; i < statements.length; i++) {
    const line = statements[i];
    if (!STRUCTURED_LABEL_RE.test(line)) {
      out.push(line);
      continue;
    }
    const next = statements[i + 1]?.trim() ?? "";
    if (next.startsWith("while ") || next.startsWith("do {") || next.startsWith("for (")) {
      continue;
    }
    out.push(line);
  }
  return out;
}

// Pre-trimmed variant: avoids O(n) .trim() calls per invocation.
function findMatchingCloseTrimmed(trimmed, startIndex) {
  let depth = 0;
  for (let index = startIndex; index < trimmed.length; index += 1) {
    const line = trimmed[index];
    if (line.endsWith("{")) {
      depth += 1;
    }
    if (line === "}") {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }
  return -1;
}

const NEGATED_COMPARISONS = new Map([
  ["JMPEQ", "!=="],
  ["JMPEQ_L", "!=="],
  ["JMPNE", "==="],
  ["JMPNE_L", "==="],
  ["JMPGT", "<="],
  ["JMPGT_L", "<="],
  ["JMPGE", "<"],
  ["JMPGE_L", "<"],
  ["JMPLT", ">="],
  ["JMPLT_L", ">="],
  ["JMPLE", ">"],
  ["JMPLE_L", ">"],
]);

const ORIGINAL_COMPARISONS = new Map([
  ["JMPEQ", "==="],
  ["JMPEQ_L", "==="],
  ["JMPNE", "!=="],
  ["JMPNE_L", "!=="],
  ["JMPGT", ">"],
  ["JMPGT_L", ">"],
  ["JMPGE", ">="],
  ["JMPGE_L", ">="],
  ["JMPLT", "<"],
  ["JMPLT_L", "<"],
  ["JMPLE", "<="],
  ["JMPLE_L", "<="],
]);

function negateComparisonMnemonic(mnemonic) {
  return NEGATED_COMPARISONS.get(mnemonic) ?? null;
}

function originalComparisonMnemonic(mnemonic) {
  return ORIGINAL_COMPARISONS.get(mnemonic) ?? null;
}
