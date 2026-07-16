import { tryCollectionExpression, tryCollectionStatement } from "../high-level-collections.js";
import { emitLabelIfNeeded, tryControlTransferFallback } from "../high-level-labels.js";
import { tryInternalCall, tryIndirectCall, trySyscall, tryTokenCall } from "../high-level-calls.js";
import { captureStoreInfo, finishInstruction } from "../high-level-state.js";
import { tryBinaryExpression, tryControlStatement, tryStackShapeOperation, tryUnaryExpression } from "../high-level-stack.js";
import { pushImmediate, tryLoadLocalOrArg, tryLoadStatic, trySlotDeclarations, tryStoreArgument, tryStoreLocal, tryStoreStatic } from "../high-level-slots.js";
import { renderUntranslatedInstruction, stripOuterParens } from "../high-level-utils.js";
export function executeStraightLine(state, instructions) {
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
    if (state.terminated) {
      break;
    }
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

    // ENDFINALLY without a structured try-block lift wrapping it
    // (i.e. one that already absorbed the matching ENDTRY) is
    // best treated as a pass-through: the JS port has no
    // verbose-mode trace comment to emit and the bytecode-level
    // `endfinally` semantics are subtle enough that flagging it
    // as "not yet translated" misleads. Match the Rust port's
    // clean-mode behaviour (silently consume).
    if (mnemonic === "ENDFINALLY") {
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
    // Surface the not-yet-translated opcode through the structured
    // `warnings` array too, so a programmatic caller (CI, IDE
    // integration) sees the hazard without having to grep the
    // rendered source. Mirrors the Rust port's `self.warn(...)` for
    // unhandled opcodes.
    const opcodeName =
      instruction.opcode.mnemonic === "UNKNOWN"
        ? `UNKNOWN_0x${instruction.opcode.byte.toString(16).padStart(2, "0").toUpperCase()}`
        : instruction.opcode.mnemonic;
    state.warnings.push(
      `high-level: 0x${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${opcodeName} (not yet translated)`,
    );
    finishInstruction(state, instruction);
  }
}
