import { jumpTarget } from "./high-level-utils.js";
import {
  collectDerivedWarnings,
  rewriteForLoops,
} from "./high-level-control-flow-shared.js";

function findMnemonicFrom(instructions, start, mnemonic) {
  for (let i = start; i < instructions.length; i++) {
    if (instructions[i].opcode.mnemonic === mnemonic) return i;
  }
  return -1;
}

export function createTryHelpers(runtime) {
  const { createState, cloneState, executeStraightLine } = runtime;

  function tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset) {
    const tryIndex = findMnemonicFrom(instructions, 0, "TRY");
    if (tryIndex < 0) {
      return null;
    }

    const tryInstruction = instructions[tryIndex];
    const handlerTargets = tryHandlerTargets(tryInstruction);
    if (handlerTargets === null) {
      return null;
    }

    const { bodyStart, catchTarget, finallyTarget } = handlerTargets;
    const indexByOffset = new Map();
    for (let i = 0; i < instructions.length; i++) {
      indexByOffset.set(instructions[i].offset, i);
    }

    const bodyStartIndex = indexByOffset.get(bodyStart);
    if (bodyStartIndex === undefined) {
      return null;
    }

    const endtryGlobalIndex = findMnemonicFrom(instructions, bodyStartIndex, "ENDTRY");
    if (endtryGlobalIndex < 0) {
      return null;
    }
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

      const finallyEndGlobalIndex = findMnemonicFrom(
        instructions,
        endtryGlobalIndex + 1,
        "ENDFINALLY",
      );

      catchSlice = instructions.slice(catchIndex, finallyIndex);
      if (finallyEndGlobalIndex >= 0) {
        finallySlice = instructions.slice(finallyIndex, finallyEndGlobalIndex);
        resumeSlice = instructions.slice(finallyEndGlobalIndex + 1);
      }
    } else if (catchTarget !== null) {
      const catchIndex = indexByOffset.get(catchTarget);
      if (catchIndex === undefined) {
        return null;
      }

      const catchEndGlobalIndex = findMnemonicFrom(
        instructions,
        endtryGlobalIndex + 1,
        "ENDTRY",
      );
      if (catchEndGlobalIndex >= 0) {
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

      const finallyEndGlobalIndex = findMnemonicFrom(
        instructions,
        finallyIndex,
        "ENDFINALLY",
      );
      if (finallyEndGlobalIndex >= 0) {
        finallySlice = instructions.slice(finallyIndex, finallyEndGlobalIndex);
        // Resume picks up after ENDFINALLY. The finally body's stack
        // effects are propagated by cloning the resume state from
        // `finallyState` further below — re-slicing the finally bytes
        // into the resume would duplicate them and trip the
        // unstructured ENDFINALLY renderer.
        resumeSlice = instructions.slice(finallyEndGlobalIndex + 1);
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
      // Stack values that the try / finally body left on the operand
      // stack flow through to the resume target on the normal path
      // (NEO VM preserves the stack across try-context exits). Clone
      // from the most-recently-finished branch so the resume sees the
      // full stack a downstream RET / consumer expects: finally if it
      // ran, else the try body, else the prefix.
      const upstream = finallyState ?? tryBodyState ?? prefixState;
      resumeState = cloneState(upstream);
      executeStraightLine(resumeState, resumeSlice);
      statements.push(...resumeState.statements.slice(upstream.statements.length));
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
