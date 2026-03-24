import { isSimpleConditional, popConditionForLoop } from "./high-level-control-flow-shared.js";
import { jumpTarget } from "./high-level-utils.js";

export function collectLabelTargets(instructions) {
  const knownOffsets = new Set(instructions.map((instruction) => instruction.offset));
  const labelTargets = new Set();

  for (const instruction of instructions) {
    const target = jumpTarget(instruction);
    if (target === null) {
      continue;
    }
    if (
      instruction.opcode.mnemonic === "JMP" ||
      instruction.opcode.mnemonic === "JMP_L" ||
      instruction.opcode.mnemonic === "ENDTRY" ||
      instruction.opcode.mnemonic === "ENDTRY_L" ||
      isSimpleConditional(instruction.opcode.mnemonic)
    ) {
      if (knownOffsets.has(target)) {
        labelTargets.add(target);
      }
    }
  }

  return labelTargets;
}

export function emitLabelIfNeeded(state, offset) {
  if (!state.labelTargets.has(offset) || state.emittedLabels.has(offset)) {
    return;
  }
  state.statements.push(`${labelName(offset)}:`);
  state.emittedLabels.add(offset);
}

export function tryControlTransferFallback(state, instruction) {
  const target = jumpTarget(instruction);
  if (target === null) {
    return false;
  }

  const mnemonic = instruction.opcode.mnemonic;
  if (isSimpleConditional(mnemonic)) {
    const condition = popConditionForLoop(state.stack, mnemonic);
    if (condition === null) {
      return false;
    }
    state.statements.push(`if ${condition} { goto ${labelName(target)}; }`);
    return true;
  }

  if (mnemonic === "JMP" || mnemonic === "JMP_L") {
    state.statements.push(`goto ${labelName(target)};`);
    state.stack.length = 0;
    return true;
  }

  if (mnemonic === "ENDTRY" || mnemonic === "ENDTRY_L") {
    state.statements.push(`leave ${labelName(target)};`);
    state.stack.length = 0;
    return true;
  }

  return false;
}

export function labelName(offset) {
  return `label_0x${offset.toString(16).padStart(4, "0")}`;
}
