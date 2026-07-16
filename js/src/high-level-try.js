import { jumpTarget } from "./high-level-utils.js";
import {
  collectDerivedWarnings,
  rewriteForLoops,
} from "./high-level-control-flow-shared.js";
import {
  extendTerminatingTryRegion,
  extendTryBodyForNestedHandlers,
  findBodyEndtryIndex,
  findHandlerEndIndex,
  findTerminatingHandlerIndex,
  tryHandlerTargets,
} from "./high-level-try-boundaries.js";

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
    // Nested TRY recovery is intentionally bounded. Large or adversarial
    // streams remain available through the linear renderer and must not turn
    // recursive handler discovery into quadratic/exponential work.
    if (instructions.length > 256) {
      return null;
    }
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
    const handlerTargetsForBoundary = [catchTarget, finallyTarget].filter(
      (target) => target !== null,
    );
    const methodStartsWithSlotSetup =
      instructions[0]?.opcode?.mnemonic === "INITSLOT";
    const outerHandlerBoundary = methodStartsWithSlotSetup || handlerTargetsForBoundary.length === 0
      ? Math.min(...handlerTargetsForBoundary, Number.POSITIVE_INFINITY)
      : (instructions.at(-1)?.offset ?? Number.POSITIVE_INFINITY) + 1;
    const tryBodyEndIndex = extendTryBodyForNestedHandlers(
      instructions,
      bodyStartIndex,
      endtryGlobalIndex,
      outerHandlerBoundary,
      indexByOffset,
    );
    const tryBodySliceEnd = tryBodyEndIndex > endtryGlobalIndex
      ? tryBodyEndIndex + 1
      : endtryGlobalIndex;
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
      // When the protected body terminates before emitting an ENDTRY, the
      // handler begins at `endtryGlobalIndex` and often ends in THROW/ABORT.
      // Do not let a later, unrelated ENDTRY (for a following TRY region)
      // absorb that handler and all subsequent instructions.
      const bodyEndedWithoutEndtry = ![
        "ENDTRY",
        "ENDTRY_L",
      ].includes(instructions[endtryGlobalIndex]?.opcode?.mnemonic);
      const terminatingCatchIndex = bodyEndedWithoutEndtry
        ? findTerminatingHandlerIndex(instructions, catchIndex)
        : -1;
      if (
        bodyEndedWithoutEndtry &&
        terminatingCatchIndex >= catchIndex &&
        (catchEndGlobalIndex < 0 || terminatingCatchIndex < catchEndGlobalIndex)
      ) {
        const regionEnd = extendTerminatingTryRegion(
          instructions,
          catchIndex,
          terminatingCatchIndex,
          indexByOffset,
        );
        catchSlice = instructions.slice(catchIndex, regionEnd + 1);
        resumeSlice = instructions.slice(regionEnd + 1);
      }
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
              const regionEnd = extendTerminatingTryRegion(
                instructions,
                finallyIndex,
                terminatingIndex,
                indexByOffset,
              );
              finallySlice = instructions.slice(finallyIndex, regionEnd + 1);
              resumeSlice = instructions.slice(regionEnd + 1);
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
    executeStraightLine(
      tryBodyState,
      instructions.slice(bodyStartIndex, tryBodySliceEnd),
    );

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
    const nestedTryBody = liftNestedTrySlice(
      instructions.slice(bodyStartIndex, tryBodySliceEnd),
    );
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
    let nestedResumeBody = null;

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
      // A compiler-generated method may place another protected region in
      // the continuation after this TRY. Lift that continuation as a
      // structured slice so the first try does not force all later regions
      // through the straight-line opcode fallback. The cloned linear state
      // remains the stack model used for this boundary; the nested result is
      // only responsible for rendering and warning propagation.
      if (
        liftStructuredSlice &&
        (manifestMethod !== null || (context?.methodTokens?.length ?? 0) > 0) &&
        resumeSlice.length <= 256 &&
        resumeSlice.some((instruction) => {
          const mnemonic = instruction.opcode?.mnemonic;
          return mnemonic === "TRY" || mnemonic === "TRY_L";
        })
      ) {
        nestedResumeBody = liftStructuredSlice(
          resumeSlice,
          manifestMethod,
          context,
          resumeSlice[0]?.offset ?? methodOffset,
          upstream,
        );
      }
      resumeState = cloneState(upstream);
      executeStraightLine(resumeState, resumeSlice);
      statements.push(
        ...(nestedResumeBody
          ? nestedResumeBody.statements
          : resumeState.statements.slice(upstream.statements.length)),
      );
    }

    const finalStackState = resumeState ?? finallyState ?? catchState ?? tryBodyState ?? prefixState;
    return rewriteForLoops({
      statements,
      warnings: collectDerivedWarnings(
        prefixState,
        tryBodyState,
        catchState,
        finallyState,
        resumeState,
      ).filter((warning) => {
        if (
          !nestedTryBody &&
          !nestedCatchBody &&
          !nestedFinallyBody &&
          !nestedResumeBody
        ) return true;
        return !/TRY(?:_L)? \(not yet translated\)/u.test(warning);
      }).concat(
        nestedTryBody?.warnings ?? [],
        nestedCatchBody?.warnings ?? [],
        nestedFinallyBody?.warnings ?? [],
        nestedResumeBody?.warnings ?? [],
      ),
      stack: [...finalStackState.stack],
      nextTempId: finalStackState.nextTempId,
      terminated: finalStackState.terminated === true,
    });
  }

  return { tryLiftSimpleTryBlock };
}
