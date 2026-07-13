import { hexOffset } from "./util.js";

export function resolveMethodTarget(methodByOffset, targetOffset) {
  return (
    methodByOffset.get(targetOffset) ?? {
      offset: targetOffset,
      name: `sub_0x${hexOffset(targetOffset)}`,
    }
  );
}

export function pointerTargetBeforeIndex(instructions, index, localValues, staticValues) {
  let cursor = index - 1;
  while (cursor >= 0) {
    const previous = instructions[cursor];
    if (!previous) {
      return null;
    }
    if (previous.opcode.mnemonic === "NOP") {
      cursor -= 1;
      continue;
    }
    if (previous.opcode.mnemonic === "DUP") {
      cursor -= 1;
      continue;
    }
    if (previous.opcode.mnemonic === "PUSHA" && previous.operand?.kind === "I32") {
      return relativePointerTarget(previous);
    }
    const local = isLoadLocal(previous.opcode.mnemonic)
      ? slotIndex(previous.opcode.mnemonic, previous)
      : null;
    if (local !== null) {
      return valueToPointer(localValues.get(local) ?? null);
    }
    const staticSlot = isLoadStatic(previous.opcode.mnemonic)
      ? slotIndex(previous.opcode.mnemonic, previous)
      : null;
    if (staticSlot !== null) {
      return valueToPointer(staticValues.get(staticSlot) ?? null);
    }
    return null;
  }
  return null;
}

export function pointerTargetFromSlotFlow(previous, instruction, localValues, staticValues) {
  if (!previous) {
    return null;
  }
  const local = isLoadLocal(previous.opcode.mnemonic) ? slotIndex(previous.opcode.mnemonic, previous) : null;
  if (local !== null) {
    return valueToPointer(localValues.get(local) ?? null);
  }
  const staticSlot = isLoadStatic(previous.opcode.mnemonic) ? slotIndex(previous.opcode.mnemonic, previous) : null;
  if (staticSlot !== null) {
    return valueToPointer(staticValues.get(staticSlot) ?? null);
  }
  return null;
}

export function isJumpOperand(operand) {
  return operand?.kind === "Jump" || operand?.kind === "Jump32";
}

const SLOT_INDEX_RE = /(?:LD|ST)(?:LOC|ARG|SFLD)(\d+)$/u;
const STLOC_RE = /^STLOC(?:\d+)?$/u;
const STARG_RE = /^STARG(?:\d+)?$/u;
const STSFLD_RE = /^STSFLD(?:\d+)?$/u;
const LDARG_RE = /^LDARG(?:\d+)?$/u;
const LDLOC_RE = /^LDLOC(?:\d+)?$/u;
const LDSFLD_RE = /^LDSFLD(?:\d+)?$/u;

export function slotIndex(mnemonic, instruction) {
  const exact = SLOT_INDEX_RE.exec(mnemonic);
  if (exact) {
    return Number(exact[1]);
  }
  if (instruction.operand?.kind === "U8") {
    return instruction.operand.value;
  }
  return null;
}

export function isStoreLocal(mnemonic) {
  return STLOC_RE.test(mnemonic);
}

export function isStoreArgument(mnemonic) {
  return STARG_RE.test(mnemonic);
}

export function isStoreStatic(mnemonic) {
  return STSFLD_RE.test(mnemonic);
}

export function isLoadArgument(mnemonic) {
  return LDARG_RE.test(mnemonic);
}

export function isLoadLocal(mnemonic) {
  return LDLOC_RE.test(mnemonic);
}

export function isLoadStatic(mnemonic) {
  return LDSFLD_RE.test(mnemonic);
}

export function inferMethodArgCount(group) {
  if (group.source?.parameters) {
    return group.source.parameters.length;
  }
  const first = group.instructions[0];
  if (
    first?.opcode?.mnemonic === "INITSLOT" &&
    first.operand?.kind === "Bytes" &&
    first.operand.value.length >= 2
  ) {
    return first.operand.value[1];
  }
  let maxArg = -1;
  for (const instruction of group.instructions) {
    if (isLoadArgument(instruction.opcode.mnemonic) || isStoreArgument(instruction.opcode.mnemonic)) {
      const slot = slotIndex(instruction.opcode.mnemonic, instruction);
      if (slot !== null) {
        maxArg = Math.max(maxArg, slot);
      }
    }
  }
  if (maxArg >= 0) {
    return maxArg + 1;
  }
  return 0;
}

export function popValue(valueStack) {
  if (valueStack.length === 0) {
    return null;
  }
  return valueStack.pop();
}

export function popMany(valueStack, count) {
  for (let index = 0; index < count; index += 1) {
    if (valueStack.length === 0) {
      break;
    }
    valueStack.pop();
  }
}

export function ensureArgValueArray(methodArgValues, methodOffset, size) {
  const current = methodArgValues.get(methodOffset) ?? [];
  while (current.length < size) {
    current.push(null);
  }
  methodArgValues.set(methodOffset, current);
  return current;
}

export function propagateCallArguments(
  methodArgValues,
  methodArgCountsByOffset,
  targetOffset,
  valueStack,
  targetOnStack,
) {
  const argCount = methodArgCountsByOffset.get(targetOffset) ?? 0;
  if (argCount === 0) {
    return;
  }
  const args = [];
  const start = Math.max(0, valueStack.length - argCount);
  for (let index = valueStack.length - 1; index >= start; index -= 1) {
    args.push(valueStack[index] ?? null);
  }
  const values = ensureArgValueArray(methodArgValues, targetOffset, argCount);
  for (let index = 0; index < argCount; index += 1) {
    values[index] = mergeValues(values[index], args[index] ?? null);
  }
  popMany(valueStack, argCount);
  if (targetOnStack) {
    // target pointer was already popped by CALLA resolution
    return;
  }
}

export function relativePointerTarget(instruction) {
  // PUSHA carries a signed I32 relative offset (backward pointers are
  // legal). Mirrors Rust's `pusha_absolute_target`: a target that falls
  // before the script start is unresolvable (`checked_add_signed` → None).
  const target = instruction.offset + instruction.operand.value;
  return target >= 0 ? target : null;
}

export function pointerValue(target) {
  return { kind: "pointer", target };
}

export function valueToPointer(value) {
  return value?.kind === "pointer" ? value.target : null;
}

export function mergeValues(existing, next) {
  if (next === null || next === undefined) {
    return existing ?? null;
  }
  if (existing === undefined || existing === null) {
    return next;
  }
  if (existing?.kind === "pointer" && next?.kind === "pointer") {
    return existing.target === next.target ? existing : null;
  }
  return existing === next ? existing : null;
}

const PUSH_LIT_RE = /^PUSH(\d+|M1)$/u;
const PUSHINT_RE = /^PUSHINT(?:8|16|32|64)$/u;

export function isImmediateInteger(mnemonic, instruction) {
  if (PUSH_LIT_RE.test(mnemonic)) {
    return true;
  }
  return PUSHINT_RE.test(mnemonic);
}

export function integerValue(mnemonic, instruction) {
  const match = PUSH_LIT_RE.exec(mnemonic);
  if (match) {
    return { kind: "int", value: match[1] === "M1" ? -1 : Number(match[1]) };
  }
  const raw = instruction.operand?.value;
  return { kind: "int", value: Number(raw) };
}
