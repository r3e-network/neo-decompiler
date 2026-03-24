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
  for (let index = 1; index < statements.length - 2; index += 1) {
    const init = statements[index - 1];
    const whileLine = statements[index];
    const closeIndex = findMatchingClose(statements, index);
    if (closeIndex === -1 || closeIndex <= index + 1) {
      continue;
    }
    const increment = statements[closeIndex - 1];

    const initMatch = init.match(/^let (\w+) = (.+);$/u);
    const whileMatch = whileLine.match(/^while (.+) \{$/u);
    const incrementMatch = increment.match(/^(\w+) = \1 \+ 1;$/u);
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
    statements[index - 1] = "";
    statements[closeIndex - 1] = "";
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

function stripStructuredLabels(statements) {
  return statements.flatMap((line, index) => {
    if (!/^\s*label_0x[0-9a-f]+:\s*$/iu.test(line)) {
      return [line];
    }
    const next = statements[index + 1]?.trim() ?? "";
    if (next.startsWith("while ") || next.startsWith("do {") || next.startsWith("for (")) {
      return [];
    }
    return [line];
  });
}

function findMatchingClose(statements, startIndex) {
  let depth = 0;
  for (let index = startIndex; index < statements.length; index += 1) {
    const line = statements[index].trim();
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

function negateComparisonMnemonic(mnemonic) {
  const operators = {
    JMPEQ: "!==",
    JMPEQ_L: "!==",
    JMPNE: "===",
    JMPNE_L: "===",
    JMPGT: "<=",
    JMPGT_L: "<=",
    JMPGE: "<",
    JMPGE_L: "<",
    JMPLT: ">=",
    JMPLT_L: ">=",
    JMPLE: ">",
    JMPLE_L: ">",
  };
  return operators[mnemonic] ?? null;
}

function originalComparisonMnemonic(mnemonic) {
  const operators = {
    JMPEQ: "===",
    JMPEQ_L: "===",
    JMPNE: "!==",
    JMPNE_L: "!==",
    JMPGT: ">",
    JMPGT_L: ">",
    JMPGE: ">=",
    JMPGE_L: ">=",
    JMPLT: "<",
    JMPLT_L: "<",
    JMPLE: "<=",
    JMPLE_L: "<=",
  };
  return operators[mnemonic] ?? null;
}
