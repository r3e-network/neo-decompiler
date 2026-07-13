import { SYSCALLS } from "./generated/syscalls.js";
import { hexOffset, upperHex } from "./util.js";

export function buildCallGraph(nef, instructions, methodGroups) {
  const methods = methodGroups.map((group) => ({
    offset: group.start,
    name: group.name,
  }));
  const methodByOffset = new Map(methods.map((method) => [method.offset, method]));
  const methodStartOffsets = new Set(methods.map((method) => method.offset));
  // Valid instruction start offsets. A CALL/CALL_L target must land on one to
  // resolve to an internal method; a target past the script end (or
  // mid-instruction) is unresolvable, mirroring the Rust port.
  const instructionOffsets = new Set(instructions.map((ins) => ins.offset));
  const methodArgCountsByOffset = new Map(
    methodGroups.map((group) => [group.start, inferMethodArgCount(group)]),
  );
  const methodReturnsValueByOffset = new Map(
    methodGroups
      .filter((group) => typeof group.source?.returnType === "string")
      .map((group) => [group.start, group.source.returnType.toLowerCase() !== "void"]),
  );
  const methodArgValues = new Map();

  const edges = [];
  const localValues = new Map();
  const staticValues = new Map();
  let valueStack = [];
  let currentMethodOffset = methods[0]?.offset ?? 0;
  let currentArgValues = methodArgValues.get(currentMethodOffset) ?? [];

  // Instructions and methods are both sorted by ascending offset, so attribute
  // each instruction to its enclosing method with a single forward-moving
  // cursor (O(N+M)) instead of rescanning every method per instruction (O(N*M)).
  let methodCursor = 0;
  const fallbackCaller = methods[0] ?? { offset: 0, name: "script_entry" };

  for (let index = 0; index < instructions.length; index += 1) {
    const instruction = instructions[index];
    while (
      methodCursor + 1 < methods.length &&
      methods[methodCursor + 1].offset <= instruction.offset
    ) {
      methodCursor += 1;
    }
    const caller = methods[methodCursor] ?? fallbackCaller;
    const mnemonic = instruction.opcode.mnemonic;

    if (index > 0 && methodStartOffsets.has(instruction.offset)) {
      localValues.clear();
      valueStack = [];
      currentMethodOffset = caller.offset;
      currentArgValues = methodArgValues.get(currentMethodOffset) ?? [];
    }

    if (mnemonic === "NOP") {
      continue;
    }

    if (mnemonic === "PUSHA" && instruction.operand?.kind === "I32") {
      const target = relativePointerTarget(instruction);
      valueStack.push(target !== null ? pointerValue(target) : null);
      continue;
    }

    if (isImmediateInteger(mnemonic, instruction)) {
      valueStack.push(integerValue(mnemonic, instruction));
      continue;
    }

    if (mnemonic === "NEWARRAY0") {
      valueStack.push({ kind: "array", items: [] });
      continue;
    }

    if (mnemonic === "DUP") {
      valueStack.push(valueStack.at(-1) ?? null);
      continue;
    }

    if (mnemonic === "DROP") {
      valueStack.pop();
      continue;
    }

    if (isLoadArgument(mnemonic)) {
      const slot = slotIndex(mnemonic, instruction);
      valueStack.push(slot !== null ? currentArgValues[slot] ?? null : null);
      continue;
    }

    if (isLoadLocal(mnemonic)) {
      const slot = slotIndex(mnemonic, instruction);
      valueStack.push(slot !== null ? localValues.get(slot) ?? null : null);
      continue;
    }

    if (isLoadStatic(mnemonic)) {
      const slot = slotIndex(mnemonic, instruction);
      valueStack.push(slot !== null ? staticValues.get(slot) ?? null : null);
      continue;
    }

    if (isStoreLocal(mnemonic)) {
      const slot = slotIndex(mnemonic, instruction);
      const value = popValue(valueStack);
      const target =
        valueToPointer(value) ??
        pointerTargetBeforeIndex(instructions, index, localValues, staticValues);
      if (slot !== null && value !== null) {
        localValues.set(slot, value);
      } else if (slot !== null && target !== null) {
        localValues.set(slot, pointerValue(target));
      } else if (slot !== null) {
        localValues.delete(slot);
      }
      continue;
    }

    if (isStoreStatic(mnemonic)) {
      const slot = slotIndex(mnemonic, instruction);
      const value = popValue(valueStack);
      const target =
        valueToPointer(value) ??
        pointerTargetBeforeIndex(instructions, index, localValues, staticValues);
      if (slot !== null && value !== null) {
        staticValues.set(slot, value);
      } else if (slot !== null && target !== null) {
        staticValues.set(slot, pointerValue(target));
      } else if (slot !== null) {
        staticValues.delete(slot);
      }
      continue;
    }

    if (isStoreArgument(mnemonic)) {
      const slot = slotIndex(mnemonic, instruction);
      const value = popValue(valueStack);
      if (slot !== null) {
        currentArgValues = ensureArgValueArray(
          methodArgValues,
          currentMethodOffset,
          Math.max(currentArgValues.length, slot + 1),
        );
        currentArgValues[slot] = mergeValues(currentArgValues[slot], value);
      }
      continue;
    }

    if (mnemonic === "APPEND") {
      const item = popValue(valueStack);
      const target = popValue(valueStack);
      if (target?.kind === "array") {
        target.items.push(item);
      }
      continue;
    }

    if (mnemonic === "PICKITEM") {
      const indexValue = popValue(valueStack);
      const target = popValue(valueStack);
      if (
        target?.kind === "array" &&
        indexValue?.kind === "int" &&
        indexValue.value >= 0 &&
        indexValue.value < target.items.length
      ) {
        valueStack.push(target.items[indexValue.value] ?? null);
      } else {
        valueStack.push(null);
      }
      continue;
    }

    if (mnemonic === "SYSCALL" && instruction.operand?.kind === "Syscall") {
      const info = SYSCALLS.get(instruction.operand.value) ?? null;
      popMany(valueStack, info?.param_count ?? 0);
      if (info?.returns_value ?? true) {
        valueStack.push(null);
      }
      edges.push({
        caller,
        callOffset: instruction.offset,
        opcode: mnemonic,
        target: {
          kind: "Syscall",
          hash: instruction.operand.value,
          name: info?.name ?? null,
          returnsValue: info?.returns_value ?? true,
        },
      });
      continue;
    }

    if ((mnemonic === "CALL" || mnemonic === "CALL_L") && isJumpOperand(instruction.operand)) {
      const targetOffset = instruction.offset + instruction.operand.value;
      if (targetOffset < 0 || !instructionOffsets.has(targetOffset)) {
        // A CALL/CALL_L whose absolute target is negative (malformed backward
        // delta) or lands past the script end / mid-instruction cannot be a
        // real method. Mirror the Rust port, which emits an UnresolvedInternal
        // edge with the raw signed target instead of fabricating an Internal
        // edge at an invalid offset.
        edges.push({
          caller,
          callOffset: instruction.offset,
          opcode: mnemonic,
          target: { kind: "UnresolvedInternal", target: targetOffset },
        });
        valueStack.push(null);
        continue;
      }
      propagateCallArguments(
        methodArgValues,
        methodArgCountsByOffset,
        targetOffset,
        valueStack,
        false,
      );
      edges.push({
        caller,
        callOffset: instruction.offset,
        opcode: mnemonic,
        target: {
          kind: "Internal",
          method: resolveMethodTarget(methodByOffset, targetOffset),
        },
      });
      if (methodReturnsValueByOffset.get(targetOffset) ?? true) {
        valueStack.push(null);
      }
      continue;
    }

    if (mnemonic === "CALLT" && instruction.operand?.kind === "U16") {
      const token = nef.methodTokens[instruction.operand.value] ?? null;
      popMany(valueStack, token?.parametersCount ?? 0);
      if (token?.hasReturnValue ?? true) {
        valueStack.push(null);
      }
      edges.push({
        caller,
        callOffset: instruction.offset,
        opcode: mnemonic,
        target: token
          ? {
              kind: "MethodToken",
              index: instruction.operand.value,
              hashLe: upperHex(token.hash),
              hashBe: upperHex([...token.hash].reverse()),
              method: token.method,
              parametersCount: token.parametersCount,
              hasReturnValue: token.hasReturnValue,
              callFlags: token.callFlags,
            }
          : {
              kind: "Indirect",
              opcode: mnemonic,
              operand: instruction.operand.value,
            },
      });
      continue;
    }

    if (mnemonic === "CALLA") {
      const stackTarget = valueToPointer(popValue(valueStack));
      const rawTarget =
        stackTarget ??
        pointerTargetBeforeIndex(instructions, index, localValues, staticValues) ??
        pointerTargetFromSlotFlow(instructions[index - 1], instruction, localValues, staticValues);
      // Only a pointer that lands on a valid instruction offset is a resolved
      // internal target; an out-of-range pointer is Indirect, not a fabricated
      // sub_0xNNNN method (mirrors the Rust port).
      const resolved =
        rawTarget !== null && instructionOffsets.has(rawTarget) ? rawTarget : null;
      if (resolved !== null) {
        propagateCallArguments(
          methodArgValues,
          methodArgCountsByOffset,
          resolved,
          valueStack,
          true,
        );
      }
      edges.push({
        caller,
        callOffset: instruction.offset,
        opcode: mnemonic,
        target:
          resolved !== null
            ? {
                kind: "Internal",
                method: resolveMethodTarget(methodByOffset, resolved),
              }
            : {
                kind: "Indirect",
                opcode: mnemonic,
                operand: null,
              },
      });
      if (resolved === null || (methodReturnsValueByOffset.get(resolved) ?? true)) {
        valueStack.push(null);
      }
      continue;
    }

    // Pointer resolution must prefer false negatives over fabricated internal
    // calls. If an opcode has no transfer function above, its stack effect is
    // unknown; retaining earlier values could expose a stale pointer to a later
    // CALLA. Invalidate the simulated stack until a modeled producer rebuilds it.
    valueStack = [];
  }

  return { methods, edges };
}

function resolveMethodTarget(methodByOffset, targetOffset) {
  return (
    methodByOffset.get(targetOffset) ?? {
      offset: targetOffset,
      name: `sub_0x${hexOffset(targetOffset)}`,
    }
  );
}

function pointerTargetBeforeIndex(instructions, index, localValues, staticValues) {
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

function pointerTargetFromSlotFlow(previous, instruction, localValues, staticValues) {
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

function isJumpOperand(operand) {
  return operand?.kind === "Jump" || operand?.kind === "Jump32";
}

const SLOT_INDEX_RE = /(?:LD|ST)(?:LOC|ARG|SFLD)(\d+)$/u;
const STLOC_RE = /^STLOC(?:\d+)?$/u;
const STARG_RE = /^STARG(?:\d+)?$/u;
const STSFLD_RE = /^STSFLD(?:\d+)?$/u;
const LDARG_RE = /^LDARG(?:\d+)?$/u;
const LDLOC_RE = /^LDLOC(?:\d+)?$/u;
const LDSFLD_RE = /^LDSFLD(?:\d+)?$/u;

function slotIndex(mnemonic, instruction) {
  const exact = SLOT_INDEX_RE.exec(mnemonic);
  if (exact) {
    return Number(exact[1]);
  }
  if (instruction.operand?.kind === "U8") {
    return instruction.operand.value;
  }
  return null;
}

function isStoreLocal(mnemonic) {
  return STLOC_RE.test(mnemonic);
}

function isStoreArgument(mnemonic) {
  return STARG_RE.test(mnemonic);
}

function isStoreStatic(mnemonic) {
  return STSFLD_RE.test(mnemonic);
}

function isLoadArgument(mnemonic) {
  return LDARG_RE.test(mnemonic);
}

function isLoadLocal(mnemonic) {
  return LDLOC_RE.test(mnemonic);
}

function isLoadStatic(mnemonic) {
  return LDSFLD_RE.test(mnemonic);
}

function inferMethodArgCount(group) {
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

function popValue(valueStack) {
  if (valueStack.length === 0) {
    return null;
  }
  return valueStack.pop();
}

function popMany(valueStack, count) {
  for (let index = 0; index < count; index += 1) {
    if (valueStack.length === 0) {
      break;
    }
    valueStack.pop();
  }
}

function ensureArgValueArray(methodArgValues, methodOffset, size) {
  const current = methodArgValues.get(methodOffset) ?? [];
  while (current.length < size) {
    current.push(null);
  }
  methodArgValues.set(methodOffset, current);
  return current;
}

function propagateCallArguments(
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

function relativePointerTarget(instruction) {
  // PUSHA carries a signed I32 relative offset (backward pointers are
  // legal). Mirrors Rust's `pusha_absolute_target`: a target that falls
  // before the script start is unresolvable (`checked_add_signed` → None).
  const target = instruction.offset + instruction.operand.value;
  return target >= 0 ? target : null;
}

function pointerValue(target) {
  return { kind: "pointer", target };
}

function valueToPointer(value) {
  return value?.kind === "pointer" ? value.target : null;
}

function mergeValues(existing, next) {
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

function isImmediateInteger(mnemonic, instruction) {
  if (PUSH_LIT_RE.test(mnemonic)) {
    return true;
  }
  return PUSHINT_RE.test(mnemonic);
}

function integerValue(mnemonic, instruction) {
  const match = PUSH_LIT_RE.exec(mnemonic);
  if (match) {
    return { kind: "int", value: match[1] === "M1" ? -1 : Number(match[1]) };
  }
  const raw = instruction.operand?.value;
  return { kind: "int", value: Number(raw) };
}
