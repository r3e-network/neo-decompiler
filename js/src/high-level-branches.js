import { jumpTarget } from "./high-level-utils.js";
import {
  branchTerminates,
  collectDerivedWarnings,
  containsUnsupportedBranchStructure,
  isUnconditionalJump,
  isSimpleConditional,
  popConditionForBranch,
  rewriteForLoops,
} from "./high-level-control-flow-shared.js";
import { mergeBranchStacks } from "./high-level-branch-merge.js";

export function createBranchHelpers(runtime) {
  const {
    createState,
    cloneState,
    forkStateForSlice,
    executeStraightLine,
    liftStructuredSlice,
  } = runtime;

  function tryLiftSimpleSwitch(
    instructions,
    manifestMethod,
    context,
    methodOffset = 0,
    initialState = null,
  ) {
    if (instructions.length < 12) {
      return null;
    }

    let cursor = 0;
    const prefixState = initialState
      ? forkStateForSlice(initialState, instructions)
      : createState(manifestMethod, context, methodOffset, instructions);

    while (cursor < instructions.length) {
      const instruction = instructions[cursor];
      if (
        instruction.opcode.mnemonic === "LDLOC0" &&
        instructions[cursor + 1]?.opcode.mnemonic?.startsWith("PUSH") &&
        instructions[cursor + 2]?.opcode.mnemonic === "EQUAL" &&
        instructions[cursor + 3]?.opcode.mnemonic === "JMPIFNOT"
      ) {
        break;
      }
      cursor += 1;
    }

    if (cursor === instructions.length) {
      return null;
    }

    executeStraightLine(prefixState, instructions.slice(0, cursor));

    const indexByOffset = new Map();
    for (let i = 0; i < instructions.length; i++) {
      indexByOffset.set(instructions[i].offset, i);
    }

    const cases = [];
    let current = cursor;
    let endOffset = null;
    while (current + 6 < instructions.length) {
      const load = instructions[current];
      const pushCase = instructions[current + 1];
      const equal = instructions[current + 2];
      const branch = instructions[current + 3];
      const pushValue = instructions[current + 4];
      const store = instructions[current + 5];
      const jump = instructions[current + 6];
      if (
        load?.opcode.mnemonic !== "LDLOC0" ||
        !pushCase?.opcode.mnemonic.startsWith("PUSH") ||
        equal?.opcode.mnemonic !== "EQUAL" ||
        branch?.opcode.mnemonic !== "JMPIFNOT" ||
        !pushValue?.opcode.mnemonic.startsWith("PUSH") ||
        store?.opcode.mnemonic !== "STLOC0" ||
        jump?.opcode.mnemonic !== "JMP"
      ) {
        break;
      }

      const caseValue = immediateValue(pushCase);
      const assignValue = immediateValue(pushValue);
      const falseTarget = jumpTarget(branch);
      const jumpTargetOffset = jumpTarget(jump);
      if (caseValue === null || assignValue === null || falseTarget === null || jumpTargetOffset === null) {
        return null;
      }

      endOffset = jumpTargetOffset;
      cases.push({ match: caseValue, assign: assignValue, nextOffset: falseTarget });
      const nextIndex = indexByOffset.get(falseTarget);
      if (nextIndex === undefined) {
        return null;
      }
      current = nextIndex;

      const maybeNext = instructions[current];
      if (maybeNext?.opcode.mnemonic !== "LDLOC0") {
        break;
      }
    }

    if (cases.length < 2 || endOffset === null) {
      return null;
    }

    const endIndex = indexByOffset.get(endOffset);
    if (endIndex === undefined) {
      return null;
    }
    const defaultSlice = instructions.slice(current, endIndex);
    const returnSlice = instructions.slice(endIndex);
    if (
      defaultSlice.length < 2 ||
      defaultSlice[0].opcode.mnemonic.startsWith("PUSH") === false ||
      defaultSlice[1].opcode.mnemonic !== "STLOC0" ||
      returnSlice.length < 2 ||
      returnSlice[0].opcode.mnemonic !== "LDLOC0" ||
      returnSlice[1].opcode.mnemonic !== "RET"
    ) {
      return null;
    }
    const defaultValue = immediateValue(defaultSlice[0]);
    if (defaultValue === null) {
      return null;
    }

    const statements = [...prefixState.statements];
    statements.push("switch loc0 {");
    for (const item of cases) {
      statements.push(`case ${item.match} {`);
      statements.push(`loc0 = ${item.assign};`);
      statements.push("}");
    }
    statements.push("default {");
    statements.push(`loc0 = ${defaultValue};`);
    statements.push("}");
    statements.push("}");
    statements.push("return loc0;");
    return { statements, warnings: [...prefixState.warnings] };
  }

  function tryLiftSimpleBranch(
    instructions,
    manifestMethod,
    context,
    methodOffset,
    initialState = null,
  ) {
    const conditionalIndex = instructions.findIndex((instruction) => isSimpleConditional(instruction.opcode.mnemonic));
    if (conditionalIndex < 0) {
      return null;
    }

    const conditional = instructions[conditionalIndex];
    const target = jumpTarget(conditional);
    if (target === null || target <= conditional.offset) {
      return null;
    }

    const indexByOffset = new Map();
    for (let i = 0; i < instructions.length; i++) {
      indexByOffset.set(instructions[i].offset, i);
    }
    const exactTargetIndex = indexByOffset.get(target);
    let targetIndex;
    if (exactTargetIndex !== undefined) {
      targetIndex = exactTargetIndex;
    } else {
      const nextTargetIndex = instructions.findIndex((instruction) => instruction.offset > target);
      targetIndex = nextTargetIndex >= 0 ? nextTargetIndex : instructions.length;
    }
    if (targetIndex === -1 || targetIndex <= conditionalIndex) {
      return null;
    }

    const prefixState = initialState
      ? forkStateForSlice(initialState, instructions)
      : createState(manifestMethod, context, methodOffset, instructions);
    // Only emit labels for offsets within the prefix range — prevents labels
    // for inner branch targets from appearing before the if-header.
    const prefixOffsets = new Set(instructions.slice(0, conditionalIndex).map((i) => i.offset));
    for (const t of prefixState.labelTargets) {
      if (!prefixOffsets.has(t)) prefixState.labelTargets.delete(t);
    }
    executeStraightLine(prefixState, instructions.slice(0, conditionalIndex));
    const condition = popConditionForBranch(prefixState.stack, conditional.opcode.mnemonic);
    if (condition === null) {
      return null;
    }

    const rawThenSlice = instructions.slice(conditionalIndex + 1, targetIndex);
    if (rawThenSlice.length === 0) {
      return null;
    }

    let thenSlice = rawThenSlice;
    let elseSlice = [];
    let suffixSlice = instructions.slice(targetIndex);

    const explicitElseJump = rawThenSlice.at(-1);
    if (explicitElseJump && isUnconditionalJump(explicitElseJump.opcode.mnemonic)) {
      const mergeTarget = jumpTarget(explicitElseJump);
      const mergeIndex = mergeTarget !== null ? indexByOffset.get(mergeTarget) : undefined;
      if (mergeTarget !== null && mergeTarget >= target && mergeIndex !== undefined && mergeIndex >= targetIndex) {
        thenSlice = rawThenSlice.slice(0, -1);
        elseSlice = instructions.slice(targetIndex, mergeIndex);
        suffixSlice = instructions.slice(mergeIndex);
        // When mergeIndex === targetIndex, the else-slice is empty but the
        // suffix contains the else body (e.g. PUSHF for false-path).
        if (elseSlice.length === 0 && suffixSlice.length > 0) {
          elseSlice = suffixSlice;
          suffixSlice = [];
        }
      }
    }

    if (thenSlice.length === 0) {
      return null;
    }

    const nestedThen = containsUnsupportedBranchStructure(thenSlice)
      ? liftStructuredSlice(
          thenSlice,
          manifestMethod,
          context,
          thenSlice[0]?.offset ?? methodOffset,
          prefixState,
        )
      : null;
    // Close unclosed blocks from liftStructuredSlice falling through to
    // liftStraightLineMethodBody, which emits if-headers via tryControlTransferFallback
    // without closing braces.
    if (nestedThen) {
      let braceDepth = 0;
      for (const stmt of nestedThen.statements) {
        const t = stmt.trim();
        if (t.endsWith("{")) braceDepth++;
        if (t === "}" || t.startsWith("} ")) braceDepth--;
      }
      for (; braceDepth > 0; braceDepth--) {
        nestedThen.statements.push("}");
      }
    }
    const thenState = cloneState(prefixState);
    let thenStackState = thenState;
    if (nestedThen === null) {
      executeStraightLine(thenState, thenSlice);
    } else {
      thenStackState = cloneState(prefixState);
      if (nestedThen.stack) {
        thenStackState.stack = [...nestedThen.stack];
        thenStackState.warnings = [...prefixState.warnings];
      } else {
        executeStraightLine(thenStackState, thenSlice);
      }
      thenStackState.terminated = nestedThen.terminated === true;
      if (Number.isInteger(nestedThen.nextTempId)) {
        thenStackState.nextTempId = nestedThen.nextTempId;
      }
    }
    const thenTerminates = thenStackState.terminated || branchTerminates(thenSlice, context);

    if (elseSlice.length === 0) {
      const restSlice = instructions.slice(targetIndex);
      suffixSlice = restSlice;
      if (thenTerminates && restSlice.length > 0) {
        if (restSlice.length > 1 || restSlice[0].opcode.mnemonic !== "RET") {
          elseSlice = restSlice;
          suffixSlice = [];
        }
      }
    }

    let nestedElse = null;
    const elseState = cloneState(prefixState);
    let elseStackState = elseState;
    if (elseSlice.length > 0) {
      nestedElse = containsUnsupportedBranchStructure(elseSlice)
        ? liftStructuredSlice(
            elseSlice,
            manifestMethod,
            context,
            elseSlice[0]?.offset ?? methodOffset,
            prefixState,
          )
        : null;
      if (nestedElse === null) {
        executeStraightLine(elseState, elseSlice);
      } else {
        elseStackState = cloneState(prefixState);
        if (nestedElse.stack) {
          elseStackState.stack = [...nestedElse.stack];
          elseStackState.warnings = [...prefixState.warnings];
        } else {
          executeStraightLine(elseStackState, elseSlice);
        }
        elseStackState.terminated = nestedElse.terminated === true;
        if (Number.isInteger(nestedElse.nextTempId)) {
          elseStackState.nextTempId = nestedElse.nextTempId;
        }
      }
    }
    const elseTerminates = elseSlice.length > 0
      ? elseStackState.terminated || branchTerminates(elseSlice, context)
      : false;
    const branchMerge = mergeBranchStacks(
      prefixState,
      thenStackState,
      elseStackState,
      thenTerminates,
      elseTerminates,
    );
    let suffixState = null;
    let nestedSuffix = null;
    if (suffixSlice.length > 0) {
      suffixState = cloneState(prefixState);
      if (branchMerge) suffixState.stack = [...branchMerge.mergedStack];
      const canRecoverStructuredSuffix =
        manifestMethod !== null || (context?.methodTokens?.length ?? 0) > 0;
      const hasStructuredSuffix = canRecoverStructuredSuffix &&
        (suffixSlice.some((instruction) =>
          ["TRY", "TRY_L"].includes(instruction.opcode?.mnemonic),
        )
          ? suffixSlice.length <= 256
          : suffixSlice.length <= 48 &&
            containsUnsupportedBranchStructure(suffixSlice) &&
            (suffixSlice.some((instruction) => instruction.opcode?.mnemonic === "CALLT")
              || containsKnownTerminator(suffixSlice, context)));
      if (hasStructuredSuffix) {
        nestedSuffix = liftStructuredSlice(
          suffixSlice,
          manifestMethod,
          context,
          suffixSlice[0]?.offset ?? methodOffset,
          suffixState,
        );
      }
      if (nestedSuffix?.stack) {
        suffixState.stack = [...nestedSuffix.stack];
        suffixState.warnings = [...prefixState.warnings];
        suffixState.terminated = nestedSuffix.terminated === true;
      } else {
        executeStraightLine(suffixState, suffixSlice);
      }
    }
    const statements = [...prefixState.statements];

    if (branchMerge) {
      statements.push(...branchMerge.declarations);
      prefixState.nextTempId = branchMerge.nextTempId;
    }

    statements.push(`if ${condition} {`);
    if (nestedThen) {
      statements.push(...nestedThen.statements);
      if (branchMerge?.thenAssignments.length) {
        statements.push(...branchMerge.thenAssignments);
      }
    } else {
      statements.push(...thenState.statements.slice(prefixState.statements.length));
      if (branchMerge?.thenAssignments.length) {
        statements.push(...branchMerge.thenAssignments);
      } else {
        // Emit remaining stack values as statements (PUSH-only then-bodies)
        while (thenState.stack.length > 0) {
          const val = thenState.stack.shift();
          if (val !== undefined) statements.push(`${val};`);
        }
      }
    }
    if (elseSlice.length > 0) {
      statements.push("} else {");
      if (nestedElse) {
        statements.push(...nestedElse.statements);
        if (branchMerge?.elseAssignments.length) {
          statements.push(...branchMerge.elseAssignments);
        }
      } else {
        statements.push(...elseState.statements.slice(prefixState.statements.length));
        if (branchMerge?.elseAssignments.length) {
          statements.push(...branchMerge.elseAssignments);
        } else {
          // Emit remaining stack values as statements (PUSH-only else-bodies)
          while (elseState.stack.length > 0) {
            const val = elseState.stack.shift();
            if (val !== undefined) statements.push(`${val};`);
          }
        }
      }
      statements.push("}");
      if (nestedSuffix) statements.push(...nestedSuffix.statements);
      else if (suffixState) statements.push(...suffixState.statements.slice(prefixState.statements.length));
      return rewriteForLoops({
        statements,
        warnings: [
          ...collectDerivedWarnings(prefixState, thenState, elseState, suffixState),
          ...collectDerivedWarnings(prefixState, thenStackState, elseStackState),
          ...(nestedThen?.warnings ?? []),
          ...(nestedElse?.warnings ?? []),
          ...(nestedSuffix?.warnings ?? []),
        ],
        stack: [...(suffixState?.stack ?? branchMerge?.mergedStack ?? prefixState.stack)],
        nextTempId: suffixState?.nextTempId ?? branchMerge?.nextTempId ?? prefixState.nextTempId,
        terminated: suffixState?.terminated === true
          || nestedSuffix?.terminated === true
          || (thenTerminates && elseTerminates),
      });
    }

    statements.push("}");
    if (nestedSuffix) statements.push(...nestedSuffix.statements);
    else if (suffixState) statements.push(...suffixState.statements.slice(prefixState.statements.length));
    return rewriteForLoops({
      statements,
      warnings: [
        ...collectDerivedWarnings(prefixState, thenState, suffixState),
        ...collectDerivedWarnings(prefixState, thenStackState, elseStackState),
        ...(nestedThen?.warnings ?? []),
        ...(nestedSuffix?.warnings ?? []),
      ],
      stack: [...(suffixState?.stack ?? branchMerge?.mergedStack ?? prefixState.stack)],
      nextTempId: suffixState?.nextTempId ?? branchMerge?.nextTempId ?? prefixState.nextTempId,
      terminated: suffixState?.terminated === true
        || nestedSuffix?.terminated === true
        || thenTerminates,
    });
  }

  return { tryLiftSimpleSwitch, tryLiftSimpleBranch };
}

const PUSH_LIT_RE = /^PUSH(\d+|M1)$/u;

function immediateValue(instruction) {
  const mnemonic = instruction?.opcode?.mnemonic;
  if (!mnemonic) {
    return null;
  }
  if (mnemonic === "PUSHNULL") return "null";
  if (mnemonic === "PUSHT") return "true";
  if (mnemonic === "PUSHF") return "false";

  const match = PUSH_LIT_RE.exec(mnemonic);
  if (match) {
    return match[1] === "M1" ? "-1" : `${Number(match[1])}`;
  }
  if (instruction.operand && ["I8", "I16", "I32", "I64", "U8", "U16", "U32"].includes(instruction.operand.kind)) {
    return `${instruction.operand.value}`;
  }
  if (instruction.operand?.kind === "Bytes" && instruction.opcode.mnemonic.startsWith("PUSHDATA")) {
    const text = decodePrintableBytes(instruction.operand.value);
    if (text !== null) {
      return `"${text}"`;
    }
    return `0x${Array.from(instruction.operand.value, (b) => b.toString(16).padStart(2, "0"))
      .join("")
      .toUpperCase()}`;
  }
  return null;
}

function decodePrintableBytes(bytes) {
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    if ([...text].every((ch) => /[\x20-\x7E]/u.test(ch))) {
      return text;
    }
  } catch {}
  return null;
}

function containsKnownTerminator(instructions, context) {
  return instructions.some((instruction) => {
    const mnemonic = instruction.opcode?.mnemonic;
    if (["THROW", "ABORT", "ABORTMSG"].includes(mnemonic)) {
      return true;
    }
    if (mnemonic !== "CALL" && mnemonic !== "CALL_L") {
      return false;
    }
    const target = jumpTarget(instruction);
    return target !== null && context?.methodNeverReturnsByOffset?.get(target) === true;
  });
}
