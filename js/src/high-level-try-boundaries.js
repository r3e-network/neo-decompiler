import { jumpTarget } from "./high-level-utils.js";

function findMnemonicFrom(instructions, start, mnemonic) {
  for (let index = start; index < instructions.length; index += 1) {
    if (instructions[index]?.opcode?.mnemonic === mnemonic) return index;
  }
  return -1;
}

// Compiler-generated nested try blocks place the outer handler immediately
// after the ENDTRY that closes the outer body. Searching from the body start
// for the first ENDTRY instead selects an inner handler transfer and slices
// the outer catch/finally regions at the wrong offsets.
export function findBodyEndtryIndex(
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

// Some compiler layouts overlap nested protected regions: an inner finally
// handler begins after the outer normal ENDTRY but before the outer catch or
// finally target. Keep that bounded handler in the recursive body slice so it
// can be rendered as part of the inner TRY. The outer transfer boundary still
// controls catch/finally/resume slicing, so this does not consume a sibling
// handler or a following method.
export function extendTryBodyForNestedHandlers(
  instructions,
  bodyStartIndex,
  bodyEndIndex,
  outerHandlerBoundary,
  indexByOffset,
) {
  if (!Number.isFinite(outerHandlerBoundary)) return bodyEndIndex;
  let endIndex = bodyEndIndex;
  for (let pass = 0; pass < 16; pass += 1) {
    let extended = false;
    for (let index = bodyStartIndex; index <= endIndex; index += 1) {
      const mnemonic = instructions[index]?.opcode?.mnemonic;
      if (mnemonic !== "TRY" && mnemonic !== "TRY_L") continue;
      const targets = tryHandlerTargets(instructions[index]);
      if (!targets) continue;
      for (const target of [targets.catchTarget, targets.finallyTarget]) {
        if (target === null || target >= outerHandlerBoundary) continue;
        const targetIndex = indexByOffset.get(target);
        if (targetIndex === undefined || targetIndex <= endIndex) continue;
        const handlerEnd = findBoundedHandlerEnd(
          instructions,
          targetIndex,
          outerHandlerBoundary,
        );
        if (handlerEnd > endIndex) {
          endIndex = handlerEnd;
          extended = true;
        }
      }
    }
    if (!extended) break;
  }
  return endIndex;
}

function findBoundedHandlerEnd(instructions, startIndex, outerHandlerBoundary) {
  for (let index = startIndex; index < instructions.length; index += 1) {
    if ((instructions[index]?.offset ?? Number.POSITIVE_INFINITY) >= outerHandlerBoundary) {
      return index - 1;
    }
    const mnemonic = instructions[index]?.opcode?.mnemonic;
    if (mnemonic === "ENDFINALLY") return index;
    if (["ABORT", "ABORTMSG", "THROW", "RET"].includes(mnemonic)) {
      return index;
    }
  }
  return startIndex;
}

export function findHandlerEndIndex(instructions, startIndex, resumeTarget, indexByOffset) {
  const resumeIndex = resumeTarget === null ? undefined : indexByOffset.get(resumeTarget);
  if (resumeIndex !== undefined && resumeIndex > startIndex) {
    let firstTerminator = -1;
    for (let index = startIndex; index < resumeIndex; index += 1) {
      const mnemonic = instructions[index]?.opcode?.mnemonic;
      if (mnemonic === "ENDFINALLY") return index;
      if (
        firstTerminator < 0 &&
        ["ABORT", "ABORTMSG", "THROW", "RET"].includes(mnemonic)
      ) {
        firstTerminator = index;
      }
    }
    if (firstTerminator >= 0) {
      return extendTerminatingTryRegion(instructions, startIndex, firstTerminator, indexByOffset);
    }
  }
  return findMnemonicFrom(instructions, startIndex, "ENDFINALLY");
}

export function findTerminatingHandlerIndex(instructions, startIndex) {
  for (let index = startIndex; index < instructions.length; index += 1) {
    const mnemonic = instructions[index]?.opcode?.mnemonic;
    if (["ABORT", "ABORTMSG", "THROW", "RET"].includes(mnemonic)) return index;
  }
  return -1;
}

// A finally body can begin with a nested catch-only TRY whose protected body
// terminates in THROW before its handler. The first terminator is then the
// body exit, not the end of the complete nested region. Extend through the
// corresponding handler terminator so recursive lifting receives both sides
// of the nested transfer.
export function extendTerminatingTryRegion(
  instructions,
  startIndex,
  terminatingIndex,
  indexByOffset,
) {
  let endIndex = terminatingIndex;
  let regionStart = startIndex;
  for (let depth = 0; depth < 16; depth += 1) {
    let tryIndex = regionStart;
    while (
      tryIndex <= endIndex &&
      !["TRY", "TRY_L"].includes(instructions[tryIndex]?.opcode?.mnemonic)
    ) {
      tryIndex += 1;
    }
    if (tryIndex > endIndex) break;
    const targets = tryHandlerTargets(instructions[tryIndex]);
    if (!targets) break;

    const catchIndex = targets.catchTarget === null
      ? undefined
      : indexByOffset.get(targets.catchTarget);
    if (catchIndex !== undefined && catchIndex > endIndex) {
      const catchTerminator = findTerminatingHandlerIndex(instructions, catchIndex);
      if (catchTerminator < catchIndex) break;
      endIndex = catchTerminator;
      regionStart = catchIndex;
      continue;
    }

    const finallyIndex = targets.finallyTarget === null
      ? undefined
      : indexByOffset.get(targets.finallyTarget);
    if (finallyIndex !== undefined && finallyIndex > endIndex) {
      const finallyEnd = findMnemonicFrom(instructions, finallyIndex, "ENDFINALLY");
      if (finallyEnd < finallyIndex) break;
      endIndex = finallyEnd;
      regionStart = finallyEnd + 1;
      continue;
    }
    break;
  }
  return endIndex;
}

export function tryHandlerTargets(instruction) {
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
