import { tryCollectionExpression, tryCollectionStatement } from "./high-level-collections.js";
import { createControlFlowHelpers } from "./high-level-control-flow.js";
import { emitLabelIfNeeded, tryControlTransferFallback } from "./high-level-labels.js";
import { tryInternalCall, tryIndirectCall, trySyscall, tryTokenCall } from "./high-level-calls.js";
import { captureStoreInfo, cloneState, createState, emptyContext, finishInstruction } from "./high-level-state.js";
import {
  tryBinaryExpression,
  tryControlStatement,
  tryStackShapeOperation,
  tryUnaryExpression,
} from "./high-level-stack.js";
import {
  pushImmediate,
  tryLoadLocalOrArg,
  tryLoadStatic,
  trySlotDeclarations,
  tryStoreArgument,
  tryStoreLocal,
  tryStoreStatic,
} from "./high-level-slots.js";
import { renderUntranslatedInstruction, stripOuterParens } from "./high-level-utils.js";
import { formatManifestParameters, formatManifestType, makeUniqueIdentifier, sanitizeIdentifier } from "./manifest.js";
import { postprocess } from "./postprocess.js";

let CONTROL_FLOW;

function liftStructuredSlice(
  instructions,
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = instructions[0]?.offset ?? 0,
) {
  if (instructions.length === 0) {
    return { statements: [], warnings: [] };
  }

  const result =
    CONTROL_FLOW.tryLiftSimpleSwitch(instructions, manifestMethod, context, methodOffset) ??
    CONTROL_FLOW.tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset) ??
    CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset) ??
    CONTROL_FLOW.tryLiftSimpleBranch(instructions, manifestMethod, context, methodOffset) ??
    liftStraightLineMethodBody(instructions, manifestMethod, context, undefined, methodOffset);
  return result;
}

CONTROL_FLOW = createControlFlowHelpers({
  createState,
  cloneState,
  executeStraightLine,
  liftStructuredSlice,
});

export function renderHighLevelMethodGroups(groups, manifest, context = null) {
  const contractName = manifest ? sanitizeIdentifier(manifest.name) : "Contract";
  const lines = [`contract ${contractName} {`];

  const used = new Set();
  for (const group of groups) {
    const signature = renderMethodSignature(group, used, context);
    lines.push(`    ${signature} {`);

    const body = liftMethodBody(group.instructions, group.source, context, group.start);
    if (context?.highLevelWarnings) {
      context.highLevelWarnings.push(...body.warnings);
    }
    if (body.statements.length === 0) {
      lines.push("        // no instructions decoded");
    } else {
      let indentLevel = 0;
      for (const statement of body.statements) {
        const trimmed = statement.trim();
        if (trimmed.startsWith("}")) {
          indentLevel = Math.max(0, indentLevel - 1);
        }
        lines.push(`${" ".repeat(8 + indentLevel * 4)}${trimmed}`);
        if (trimmed.endsWith("{")) {
          indentLevel += 1;
        }
      }
    }

    lines.push("    }");
  }

  lines.push("}");
  return lines.join("\n") + "\n";
}

function renderMethodSignature(group, used, context = null) {
  const name = makeUniqueIdentifier(group.name, used);
  const parameters = group.source?.parameters ?? [];
  let args;
  if (parameters.length > 0) {
    args = formatManifestParameters(parameters);
  } else {
    const inferredArgCount = context?.methodArgCountsByOffset?.get(group.start) ?? 0;
    args = Array.from({ length: inferredArgCount }, (_, index) => `arg${index}`).join(", ");
  }
  const returnType = group.source?.returnType;
  const renderedReturnType = returnType ? formatManifestType(returnType) : null;
  if (renderedReturnType && renderedReturnType !== "void") {
    return `fn ${name}(${args}) -> ${renderedReturnType}`;
  }
  return `fn ${name}(${args})`;
}

export function liftMethodBody(
  instructions,
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = instructions[0]?.offset ?? 0,
) {
  let result;
  const switchLift = CONTROL_FLOW.tryLiftSimpleSwitch(instructions, manifestMethod, context, methodOffset);
  if (switchLift !== null) {
    result = switchLift;
  } else {
    const loopLift = CONTROL_FLOW.tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset);
    if (loopLift !== null) {
      result = loopLift;
    } else {
      const tryLift = CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset);
      if (tryLift !== null) {
        result = tryLift;
      } else {
        const branchLift = CONTROL_FLOW.tryLiftSimpleBranch(instructions, manifestMethod, context, methodOffset);
        if (branchLift !== null) {
          result = branchLift;
        } else {
          result = liftStraightLineMethodBody(instructions, manifestMethod, context, undefined, methodOffset);
        }
      }
    }
  }

  postprocess(result.statements, context.postprocessOptions);
  return result;
}

function liftStraightLineMethodBody(
  instructions,
  manifestMethod = null,
  context,
  initialState,
  methodOffset = instructions[0]?.offset ?? 0,
) {
  const state = initialState ?? createState(manifestMethod, context, methodOffset, instructions);
  executeStraightLine(state, instructions);
  if (instructions.at(-1)?.opcode?.mnemonic !== "RET" && state.stack.length > 0) {
    for (const expression of state.stack) {
      state.statements.push(`${stripOuterParens(expression)};`);
    }
    state.stack.length = 0;
  }
  return { statements: state.statements, warnings: state.warnings };
}

function executeStraightLine(state, instructions) {
  // Destructure stable references once. state.stack / state.statements / the
  // four pointer-and-packed Maps are mutated in place but never reassigned,
  // so caching the references avoids per-instruction property loads. The
  // assigned-to state.previousStoreInfo / state.previousInstruction stay on
  // `state` since those are slot writes, not mutations.
  const {
    statements,
    initializedLocals,
    initializedStatics,
    parameterNames,
    returnsVoid,
    stack,
    pointerTargetsByExpression,
    pointerTargetsBySlot,
    packedValuesByExpression,
    packedValuesBySlot,
  } = state;

  for (const instruction of instructions) {
    const mnemonic = instruction.opcode.mnemonic;
    emitLabelIfNeeded(state, instruction.offset);

    if (trySlotDeclarations(statements, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (pushImmediate(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (mnemonic === "NOP") {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryLoadLocalOrArg(stack, mnemonic, parameterNames, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryLoadStatic(stack, mnemonic, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (
      tryStoreLocal(
        statements,
        stack,
        initializedLocals,
        pointerTargetsByExpression,
        pointerTargetsBySlot,
        packedValuesByExpression,
        packedValuesBySlot,
        mnemonic,
        instruction,
      )
    ) {
      state.previousStoreInfo = captureStoreInfo(instruction, state);
      finishInstruction(state, instruction);
      continue;
    }

    if (tryStoreArgument(statements, stack, parameterNames, mnemonic, instruction)) {
      state.previousStoreInfo = captureStoreInfo(instruction, state);
      finishInstruction(state, instruction);
      continue;
    }

    if (
      tryStoreStatic(
        statements,
        stack,
        initializedStatics,
        pointerTargetsByExpression,
        pointerTargetsBySlot,
        packedValuesByExpression,
        packedValuesBySlot,
        mnemonic,
        instruction,
      )
    ) {
      state.previousStoreInfo = captureStoreInfo(instruction, state);
      finishInstruction(state, instruction);
      continue;
    }

    if (tryBinaryExpression(stack, mnemonic)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryInternalCall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryIndirectCall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryTokenCall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (trySyscall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryCollectionExpression(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryCollectionStatement(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryStackShapeOperation(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryUnaryExpression(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryControlStatement(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryControlTransferFallback(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (mnemonic === "RET") {
      if (returnsVoid || stack.length === 0) {
        statements.push("return;");
      } else {
        statements.push(`return ${stripOuterParens(stack.pop())};`);
      }
      finishInstruction(state, instruction);
      continue;
    }

    statements.push(renderUntranslatedInstruction(instruction));
    finishInstruction(state, instruction);
  }
}
