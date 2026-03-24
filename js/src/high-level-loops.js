import { jumpTarget } from "./high-level-utils.js";
import {
  collectDerivedWarnings,
  isSimpleConditional,
  isUnconditionalJump,
  popConditionForBranch,
  popConditionForLoop,
  rewriteForLoops,
} from "./high-level-control-flow-shared.js";

export function createLoopHelpers(runtime) {
  const { createState, cloneState, executeStraightLine, liftStructuredSlice } =
    runtime;

  function tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset) {
    return (
      tryLiftSimpleWhile(instructions, manifestMethod, context, methodOffset) ??
      tryLiftSimpleDoWhile(instructions, manifestMethod, context, methodOffset)
    );
  }

  function tryLiftSimpleWhile(instructions, manifestMethod, context, methodOffset) {
    const conditionalIndex = instructions.findIndex((instruction) =>
      isSimpleConditional(instruction.opcode.mnemonic),
    );
    if (conditionalIndex < 0) {
      return null;
    }

    const conditional = instructions[conditionalIndex];
    const falseTarget = jumpTarget(conditional);
    if (falseTarget === null || falseTarget <= conditional.offset) {
      return null;
    }

    const indexByOffset = new Map(
      instructions.map((instruction, index) => [instruction.offset, index]),
    );
    const falseIndex = indexByOffset.get(falseTarget) ?? instructions.length;
    if (falseIndex <= conditionalIndex + 1) {
      return null;
    }

    const backJump = instructions[falseIndex - 1];
    if (!backJump || !isUnconditionalJump(backJump.opcode.mnemonic)) {
      return null;
    }
    const loopTarget = jumpTarget(backJump);
    if (loopTarget === null || loopTarget > conditional.offset) {
      return null;
    }

    const prefixState = createState(manifestMethod, context, methodOffset, instructions);
    executeStraightLine(prefixState, instructions.slice(0, conditionalIndex));
    const condition = popConditionForBranch(prefixState.stack, conditional.opcode.mnemonic);
    if (condition === null) {
      return null;
    }

    const actualBodySlice = instructions
      .slice(conditionalIndex + 1, falseIndex - 1 + 1)
      .slice(0, -1);
    const bodyStructure = analyzeLoopBody(actualBodySlice, loopTarget, falseTarget);
    if (bodyStructure === null) {
      return null;
    }

    const bodyState = cloneState(prefixState);
    executeStructuredLoopBody(
      bodyState,
      bodyStructure,
      executeStraightLine,
      liftStructuredSlice,
      manifestMethod,
      context,
      methodOffset,
    );
    const suffixState = cloneState(prefixState);
    executeStraightLine(suffixState, instructions.slice(falseIndex));

    return rewriteForLoops({
      statements: [
        ...prefixState.statements,
        `while ${condition} {`,
        ...bodyState.statements.slice(prefixState.statements.length),
        "}",
        ...suffixState.statements.slice(prefixState.statements.length),
      ],
      warnings: collectDerivedWarnings(prefixState, bodyState, suffixState),
    });
  }

  function tryLiftSimpleDoWhile(instructions, manifestMethod, context, methodOffset) {
    const tailIndex = instructions.findIndex(
      (instruction, index) =>
        isSimpleConditional(instruction.opcode.mnemonic) &&
        index > 0 &&
        ((jumpTarget(instruction) ?? Number.POSITIVE_INFINITY) < instruction.offset),
    );
    if (tailIndex < 0) {
      return null;
    }

    const tail = instructions[tailIndex];
    const loopStart = jumpTarget(tail);
    if (loopStart === null) {
      return null;
    }

    const indexByOffset = new Map(
      instructions.map((instruction, index) => [instruction.offset, index]),
    );
    const loopStartIndex = indexByOffset.get(loopStart);
    if (loopStartIndex === undefined || loopStartIndex >= tailIndex) {
      return null;
    }

    const prefixState = createState(manifestMethod, context, methodOffset, instructions);
    executeStraightLine(prefixState, instructions.slice(0, loopStartIndex));

    const bodyState = cloneState(prefixState);
    executeStraightLine(bodyState, instructions.slice(loopStartIndex, tailIndex));
    const condition = popConditionForLoop(bodyState.stack, tail.opcode.mnemonic);
    if (condition === null) {
      return null;
    }

    const suffixState = cloneState(prefixState);
    executeStraightLine(suffixState, instructions.slice(tailIndex + 1));

    return rewriteForLoops({
      statements: [
        ...prefixState.statements,
        "do {",
        ...bodyState.statements.slice(prefixState.statements.length),
        `} while (${condition});`,
        ...suffixState.statements.slice(prefixState.statements.length),
      ],
      warnings: collectDerivedWarnings(prefixState, bodyState, suffixState),
    });
  }

  return { tryLiftSimpleLoop };
}

function analyzeLoopBody(instructions, loopHeadOffset, breakOffset) {
  const parts = [];
  let cursor = 0;

  while (cursor < instructions.length) {
    const instruction = instructions[cursor];
    if (isUnconditionalJump(instruction.opcode.mnemonic)) {
      const target = jumpTarget(instruction);
      if (target === loopHeadOffset) {
        parts.push({ type: "continue" });
        cursor += 1;
        continue;
      }
      if (target === breakOffset) {
        parts.push({ type: "break" });
        cursor += 1;
        continue;
      }
      return null;
    }

    if (isSimpleConditional(instruction.opcode.mnemonic)) {
      return null;
    }

    let nextCursor = cursor + 1;
    while (
      nextCursor < instructions.length &&
      !isUnconditionalJump(instructions[nextCursor].opcode.mnemonic)
    ) {
      if (isSimpleConditional(instructions[nextCursor].opcode.mnemonic)) {
        return null;
      }
      nextCursor += 1;
    }
    parts.push({
      type: "straight",
      instructions: instructions.slice(cursor, nextCursor),
    });
    cursor = nextCursor;
  }

  return parts;
}

function executeStructuredLoopBody(
  state,
  parts,
  executeStraightLineFn,
  liftStructuredSlice,
  manifestMethod,
  context,
  methodOffset,
) {
  for (const part of parts) {
    if (part.type === "straight") {
      const hasStructuredTry = part.instructions.some(
        (instruction) =>
          instruction.opcode.mnemonic === "TRY" ||
          instruction.opcode.mnemonic === "TRY_L",
      );
      if (hasStructuredTry) {
        const nested = liftStructuredSlice(
          part.instructions,
          manifestMethod,
          context,
          part.instructions[0]?.offset ?? methodOffset,
        );
        state.statements.push(...nested.statements);
        state.warnings.push(...nested.warnings);
      } else {
        executeStraightLineFn(state, part.instructions);
      }
    } else if (part.type === "break") {
      state.statements.push("break;");
      state.stack.length = 0;
    } else if (part.type === "continue") {
      state.statements.push("continue;");
      state.stack.length = 0;
    }
  }
}
