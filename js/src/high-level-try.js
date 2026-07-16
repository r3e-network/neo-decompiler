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

function findTryIndex(instructions, start) {
  for (let i = start; i < instructions.length; i++) {
    const mnemonic = instructions[i]?.opcode?.mnemonic;
    if (mnemonic === "TRY" || mnemonic === "TRY_L") return i;
  }
  return -1;
}

export function createTryHelpers(runtime) {
  const {
    createState,
    cloneState,
    forkStateForSlice,
    executeStraightLine,
    liftStructuredSlice,
  } = runtime;

  function tryLiftSimpleTryBlock(
    instructions,
    manifestMethod,
    context,
    methodOffset,
    initialState = null,
  ) {
    const tryIndex = findTryIndex(instructions, 0);
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

    const endtryGlobalIndex = findBodyEndtryIndex(
      instructions,
      bodyStartIndex,
      catchTarget,
      finallyTarget,
      indexByOffset,
    );
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

      catchSlice = instructions.slice(catchIndex, finallyIndex);
      const finallyEndGlobalIndex = findHandlerEndIndex(
        instructions,
        finallyIndex,
        jumpTarget(instructions[endtryGlobalIndex]),
        indexByOffset,
      );
      if (finallyEndGlobalIndex >= 0) {
        finallySlice = instructions.slice(finallyIndex, finallyEndGlobalIndex);
        resumeSlice = instructions.slice(finallyEndGlobalIndex + 1);
      } else {
        const resumeIndex = indexByOffset.get(jumpTarget(instructions[endtryGlobalIndex]));
        if (resumeIndex !== undefined && resumeIndex > finallyIndex) {
          finallySlice = instructions.slice(finallyIndex, resumeIndex);
          resumeSlice = instructions.slice(resumeIndex);
        } else {
          const terminatingIndex = findTerminatingHandlerIndex(instructions, finallyIndex);
          if (terminatingIndex >= 0) {
            finallySlice = instructions.slice(finallyIndex, terminatingIndex + 1);
            resumeSlice = instructions.slice(terminatingIndex + 1);
          }
        }
      }
    } else if (catchTarget !== null) {
      const catchIndex = indexByOffset.get(catchTarget);
      if (catchIndex === undefined) {
        return null;
      }

      // C# compiler-generated catch-only regions use the ENDTRY at the end
      // of the normal body as the transfer to the shared resume block. The
      // handler itself follows that ENDTRY, so there is no second ENDTRY to
      // delimit the catch slice. Use the normal-path target when it lands
      // after the handler; compact synthetic fixtures still use the legacy
      // second-ENDTRY shape and fall through to the scan below.
      const normalResumeTarget = jumpTarget(instructions[endtryGlobalIndex]);
      const normalResumeIndex = normalResumeTarget === null
        ? undefined
        : indexByOffset.get(normalResumeTarget);
      if (
        normalResumeIndex !== undefined &&
        normalResumeIndex > catchIndex &&
        normalResumeTarget > catchTarget
      ) {
        catchSlice = instructions.slice(catchIndex, normalResumeIndex);
        resumeSlice = instructions.slice(normalResumeIndex);
      }

      const catchEndGlobalIndex = findMnemonicFrom(
        instructions,
        endtryGlobalIndex + 1,
        "ENDTRY",
      );
      if (catchSlice.length === 0 && catchEndGlobalIndex >= 0) {
        const catchEndInstruction = instructions[catchEndGlobalIndex];
        const catchEndTarget = jumpTarget(catchEndInstruction);
        if (catchEndTarget !== null && catchEndTarget > catchTarget) {
          catchSlice = instructions.slice(catchIndex, indexByOffset.get(catchEndTarget));
          resumeSlice = instructions.slice(catchEndGlobalIndex + 1);
        }
      }

      if (catchSlice.length === 0) {
        const catchEndFinallyIndex = findMnemonicFrom(instructions, catchIndex, "ENDFINALLY");
        if (catchEndFinallyIndex >= 0) {
          catchSlice = instructions.slice(catchIndex, catchEndFinallyIndex);
          resumeSlice = instructions.slice(catchEndFinallyIndex + 1);
        } else {
          const terminatingIndex = findTerminatingHandlerIndex(instructions, catchIndex);
          if (terminatingIndex >= 0) {
            catchSlice = instructions.slice(catchIndex, terminatingIndex + 1);
            resumeSlice = instructions.slice(terminatingIndex + 1);
          }
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

      // A terminating try body can transfer directly to an ENDFINALLY
      // marker. There is no finally payload to render, but the wrapper is
      // still structured and should not fall back to a raw TRY warning.
      if (instructions[finallyIndex]?.opcode?.mnemonic === "ENDFINALLY") {
        allowBareTry = true;
        resumeSlice = instructions.slice(finallyIndex + 1);
      }

      const finallyEndGlobalIndex = findHandlerEndIndex(
        instructions,
        finallyIndex,
        jumpTarget(instructions[endtryGlobalIndex]),
        indexByOffset,
      );
      if (!allowBareTry) {
        if (finallyEndGlobalIndex >= 0) {
          finallySlice = instructions.slice(finallyIndex, finallyEndGlobalIndex);
          // Resume picks up after ENDFINALLY. The finally body's stack
          // effects are propagated by cloning the resume state from
          // `finallyState` further below — re-slicing the finally bytes
          // into the resume would duplicate them and trip the
          // unstructured ENDFINALLY renderer.
          resumeSlice = instructions.slice(finallyEndGlobalIndex + 1);
        } else {
          const resumeIndex = indexByOffset.get(jumpTarget(instructions[endtryGlobalIndex]));
          if (resumeIndex !== undefined && resumeIndex > finallyIndex) {
            finallySlice = instructions.slice(finallyIndex, resumeIndex);
            resumeSlice = instructions.slice(resumeIndex);
          } else {
            const terminatingIndex = findTerminatingHandlerIndex(instructions, finallyIndex);
            if (terminatingIndex >= 0) {
              finallySlice = instructions.slice(finallyIndex, terminatingIndex + 1);
              resumeSlice = instructions.slice(terminatingIndex + 1);
            }
          }
        }
      }
    }

    if (catchSlice.length === 0 && finallySlice.length === 0 && !allowBareTry) {
      return null;
    }

    const prefixState = initialState
      ? forkStateForSlice(initialState, instructions)
      : createState(manifestMethod, context, methodOffset, instructions);
    executeStraightLine(prefixState, instructions.slice(0, tryIndex));
    const tryBodyState = cloneState(prefixState);
    executeStraightLine(tryBodyState, instructions.slice(bodyStartIndex, endtryGlobalIndex));

    // Nested compiler-generated try regions are common around conversions and
    // loop bodies. Render those slices recursively so the readable surface
    // keeps structured exception blocks instead of leaking raw TRY warnings;
    // the linear states below remain the conservative continuation model.
    const liftNestedTrySlice = (slice, entryState = prefixState) => {
      if (!liftStructuredSlice || !slice.some((instruction) => {
        const mnemonic = instruction.opcode.mnemonic;
        return mnemonic === "TRY" || mnemonic === "TRY_L";
      })) {
        return null;
      }
      return liftStructuredSlice(
        slice,
        manifestMethod,
        context,
        slice[0]?.offset ?? methodOffset,
        entryState,
      );
    };
    const catchEntryState = catchSlice.length > 0 ? cloneState(prefixState) : null;
    if (catchEntryState) catchEntryState.stack.push("exception");
    const nestedTryBody = liftNestedTrySlice(instructions.slice(bodyStartIndex, endtryGlobalIndex));
    const nestedCatchBody = catchEntryState
      ? liftNestedTrySlice(catchSlice, catchEntryState)
      : null;
    const nestedFinallyBody = liftNestedTrySlice(finallySlice);

    const statements = [...prefixState.statements];
    const hasStructuredHandler = catchSlice.length > 0 || finallySlice.length > 0;
    if (hasStructuredHandler) {
      statements.push("try {");
    }
    statements.push(...(nestedTryBody
      ? nestedTryBody.statements
      : tryBodyState.statements.slice(prefixState.statements.length)));
    let catchState = null;
    let finallyState = null;
    let resumeState = null;

    if (catchSlice.length > 0) {
      catchState = catchEntryState ?? cloneState(prefixState);
      executeStraightLine(catchState, catchSlice);
      statements.push("} catch {");
      statements.push(...(nestedCatchBody
        ? nestedCatchBody.statements
        : catchState.statements.slice(prefixState.statements.length)));
    }

    if (finallySlice.length > 0) {
      finallyState = cloneState(prefixState);
      executeStraightLine(finallyState, finallySlice);
      statements.push("} finally {");
      statements.push(...(nestedFinallyBody
        ? nestedFinallyBody.statements
        : finallyState.statements.slice(prefixState.statements.length)));
    }

    if (hasStructuredHandler) {
      statements.push("}");
    }

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
      ).filter((warning) => {
        if (!nestedTryBody && !nestedCatchBody && !nestedFinallyBody) return true;
        return !/TRY(?:_L)? \(not yet translated\)/u.test(warning);
      }).concat(
        nestedTryBody?.warnings ?? [],
        nestedCatchBody?.warnings ?? [],
        nestedFinallyBody?.warnings ?? [],
      ),
    });
  }

  return { tryLiftSimpleTryBlock };
}

// Compiler-generated nested try blocks place the outer handler immediately
// after the ENDTRY that closes the outer body. Searching from the body start
// for the first ENDTRY instead selects an inner handler transfer and slices
// the outer catch/finally regions at the wrong offsets.
function findBodyEndtryIndex(
  instructions,
  bodyStartIndex,
  catchTarget,
  finallyTarget,
  indexByOffset,
) {
  const firstHandlerTarget = catchTarget ?? finallyTarget;
  if (firstHandlerTarget === null || firstHandlerTarget === undefined) {
    return findMnemonicFrom(instructions, bodyStartIndex, "ENDTRY");
  }
  const handlerIndex = indexByOffset.get(firstHandlerTarget);
  // Keep compatibility with the compact synthetic fixtures used by the JS
  // API, whose relative catch offset points at the body start. Real compiler
  // handlers are strictly after the body and use the boundary scan below.
  if (handlerIndex === undefined || handlerIndex <= bodyStartIndex) {
    return findMnemonicFrom(instructions, bodyStartIndex, "ENDTRY");
  }
  if (catchTarget !== null && catchTarget !== undefined) {
    for (let index = handlerIndex - 1; index >= bodyStartIndex; index -= 1) {
      const mnemonic = instructions[index]?.opcode?.mnemonic;
      if (mnemonic === "ENDTRY" || mnemonic === "ENDTRY_L") return index;
    }
  } else {
    // A finally-only TRY may contain a nested catch before its own ENDTRY.
    // Prefer the boundary whose transfer lands at or beyond the finally
    // handler, which is how compiler-generated ENDTRY_L regions identify the
    // outer normal path.
    for (let index = handlerIndex - 1; index >= bodyStartIndex; index -= 1) {
      const mnemonic = instructions[index]?.opcode?.mnemonic;
      if (mnemonic !== "ENDTRY" && mnemonic !== "ENDTRY_L") continue;
      const target = jumpTarget(instructions[index]);
      if (target !== null && target >= finallyTarget) return index;
    }
    for (let index = handlerIndex - 1; index >= bodyStartIndex; index -= 1) {
      const mnemonic = instructions[index]?.opcode?.mnemonic;
      if (mnemonic === "ENDTRY" || mnemonic === "ENDTRY_L") return index;
    }
  }
  // A body whose last reachable instruction always throws has no normal
  // ENDTRY transfer before the handler. In that compiler layout the handler
  // offset itself is the body boundary; the handler's trailing ENDTRY is
  // discovered by the catch-slice scan.
  return handlerIndex;
}

function findHandlerEndIndex(instructions, startIndex, resumeTarget, indexByOffset) {
  const resumeIndex = resumeTarget === null ? undefined : indexByOffset.get(resumeTarget);
  if (resumeIndex !== undefined && resumeIndex > startIndex) {
    for (let index = resumeIndex - 1; index >= startIndex; index -= 1) {
      if (instructions[index]?.opcode?.mnemonic === "ENDFINALLY") return index;
    }
  }
  return findMnemonicFrom(instructions, startIndex, "ENDFINALLY");
}

function findTerminatingHandlerIndex(instructions, startIndex) {
  for (let index = startIndex; index < instructions.length; index += 1) {
    const mnemonic = instructions[index]?.opcode?.mnemonic;
    if (["ABORT", "ABORTMSG", "THROW", "RET"].includes(mnemonic)) return index;
  }
  return -1;
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
