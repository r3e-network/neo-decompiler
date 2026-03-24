import { jumpTarget } from "./high-level-utils.js";
import {
  collectDerivedWarnings,
  rewriteForLoops,
} from "./high-level-control-flow-shared.js";

export function createTryHelpers(runtime) {
  const { createState, cloneState, executeStraightLine } = runtime;

  function tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset) {
    const tryIndex = instructions.findIndex(
      (instruction) => instruction.opcode.mnemonic === "TRY",
    );
    if (tryIndex < 0) {
      return null;
    }

    const tryInstruction = instructions[tryIndex];
    const handlerTargets = tryHandlerTargets(tryInstruction);
    if (handlerTargets === null) {
      return null;
    }

    const { bodyStart, catchTarget, finallyTarget } = handlerTargets;
    const indexByOffset = new Map(
      instructions.map((instruction, index) => [instruction.offset, index]),
    );

    const bodyStartIndex = indexByOffset.get(bodyStart);
    if (bodyStartIndex === undefined) {
      return null;
    }

    const endtryIndexInSlice = instructions.slice(bodyStartIndex).findIndex(
      (instruction) => instruction.opcode.mnemonic === "ENDTRY",
    );
    if (endtryIndexInSlice < 0) {
      return null;
    }

    const endtryGlobalIndex = bodyStartIndex + endtryIndexInSlice;
    let catchSlice = [];
    let finallySlice = [];
    let resumeSlice = [];
    let allowBareTry = false;

    if (catchTarget !== null && finallyTarget !== null) {
      const catchIndex = indexByOffset.get(catchTarget);
      const finallyIndex = indexByOffset.get(finallyTarget);
      if (catchIndex === undefined || finallyIndex === undefined) {
        return null;
      }

      const endtrySliceStart = endtryGlobalIndex + 1;
      const finallyEndIndexInSlice = instructions.slice(endtrySliceStart).findIndex(
        (instruction) => instruction.opcode.mnemonic === "ENDFINALLY",
      );
      let finallyEndGlobalIndex = null;
      if (finallyEndIndexInSlice >= 0) {
        finallyEndGlobalIndex = endtrySliceStart + finallyEndIndexInSlice;
      }

      catchSlice = instructions.slice(catchIndex, finallyIndex);
      if (finallyEndGlobalIndex !== null) {
        finallySlice = instructions.slice(finallyIndex, finallyEndGlobalIndex);
        resumeSlice = instructions.slice(finallyEndGlobalIndex + 1);
      }
    } else if (catchTarget !== null) {
      const catchIndex = indexByOffset.get(catchTarget);
      if (catchIndex === undefined) {
        return null;
      }

      const afterEndtryIndex = endtryGlobalIndex + 1;
      const catchEndtryIndexInSlice = instructions.slice(afterEndtryIndex).findIndex(
        (instruction) => instruction.opcode.mnemonic === "ENDTRY",
      );
      let catchEndGlobalIndex = null;
      if (catchEndtryIndexInSlice >= 0) {
        catchEndGlobalIndex = afterEndtryIndex + catchEndtryIndexInSlice;
        const catchEndInstruction = instructions[catchEndGlobalIndex];
        const catchEndTarget = jumpTarget(catchEndInstruction);
        if (catchEndTarget !== null && catchEndTarget > catchTarget) {
          catchSlice = instructions.slice(catchIndex, indexByOffset.get(catchEndTarget));
          resumeSlice = instructions.slice(catchEndGlobalIndex + 1);
        }
      }

      if (
        catchSlice.length === 0 &&
        instructions[catchIndex]?.opcode.mnemonic === "ENDFINALLY"
      ) {
        allowBareTry = true;
        resumeSlice = instructions.slice(catchIndex + 1);
      }
    } else if (finallyTarget !== null) {
      const finallyIndex = indexByOffset.get(finallyTarget);
      if (finallyIndex === undefined) {
        return null;
      }

      const finallyEndIndexInSlice = instructions.slice(finallyIndex).findIndex(
        (instruction) => instruction.opcode.mnemonic === "ENDFINALLY",
      );
      if (finallyEndIndexInSlice >= 0) {
        const finallyEndGlobalIndex = finallyIndex + finallyEndIndexInSlice;
        finallySlice = instructions.slice(finallyIndex, finallyEndGlobalIndex);
        resumeSlice = instructions.slice(endtryGlobalIndex + 1);
      }
    }

    if (catchSlice.length === 0 && finallySlice.length === 0 && !allowBareTry) {
      return null;
    }

    const prefixState = createState(manifestMethod, context, methodOffset, instructions);
    executeStraightLine(prefixState, instructions.slice(0, tryIndex));
    const tryBodyState = cloneState(prefixState);
    executeStraightLine(tryBodyState, instructions.slice(bodyStartIndex, endtryGlobalIndex));

    const statements = [...prefixState.statements];
    statements.push("try {");
    statements.push(...tryBodyState.statements.slice(prefixState.statements.length));
    let catchState = null;
    let finallyState = null;
    let resumeState = null;

    if (catchSlice.length > 0) {
      catchState = cloneState(prefixState);
      catchState.stack.push("exception");
      executeStraightLine(catchState, catchSlice);
      statements.push("} catch {");
      statements.push(...catchState.statements.slice(prefixState.statements.length));
    }

    if (finallySlice.length > 0) {
      finallyState = cloneState(prefixState);
      executeStraightLine(finallyState, finallySlice);
      statements.push("} finally {");
      statements.push(...finallyState.statements.slice(prefixState.statements.length));
    }

    statements.push("}");

    if (resumeSlice.length > 0) {
      resumeState = cloneState(prefixState);
      executeStraightLine(resumeState, resumeSlice);
      statements.push(...resumeState.statements.slice(prefixState.statements.length));
    }

    return rewriteForLoops({
      statements,
      warnings: collectDerivedWarnings(
        prefixState,
        tryBodyState,
        catchState,
        finallyState,
        resumeState,
      ),
    });
  }

  return { tryLiftSimpleTryBlock };
}

function tryHandlerTargets(instruction) {
  if (instruction.opcode.mnemonic !== "TRY" && instruction.opcode.mnemonic !== "TRY_L") {
    return null;
  }

  const operand = instruction.operand;
  if (operand === null || operand.kind !== "Bytes") {
    return null;
  }

  const bytes = operand.value;
  let catchDelta;
  let finallyDelta;

  if (bytes.length === 2) {
    catchDelta = bytes[0];
    finallyDelta = bytes[1];
    if (catchDelta > 127) catchDelta -= 256;
    if (finallyDelta > 127) finallyDelta -= 256;
  } else if (bytes.length === 8) {
    catchDelta = bytes[0] | (bytes[1] << 8) | (bytes[2] << 16) | (bytes[3] << 24);
    finallyDelta = bytes[4] | (bytes[5] << 8) | (bytes[6] << 16) | (bytes[7] << 24);
    if (catchDelta > 2147483647) catchDelta -= 4294967296;
    if (finallyDelta > 2147483647) finallyDelta -= 4294967296;
  } else {
    return null;
  }

  const width = 1 + bytes.length;
  const bodyStart = instruction.offset + width;

  let catchTarget = null;
  if (catchDelta !== 0) {
    const target = instruction.offset + catchDelta;
    if (target > instruction.offset) {
      catchTarget = target;
    }
  }

  let finallyTarget = null;
  if (finallyDelta !== 0) {
    const target = instruction.offset + finallyDelta;
    if (target > instruction.offset) {
      finallyTarget = target;
    }
  }

  return { bodyStart, catchTarget, finallyTarget };
}
