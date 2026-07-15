import { SYSCALLS } from "./generated/syscalls.js";
import { upperHex } from "./util.js";

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
  const staticValues = inferConstantStaticPointerValues(instructions);
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
import {
  ensureArgValueArray,
  inferConstantStaticPointerValues,
  inferMethodArgCount,
  integerValue,
  isImmediateInteger,
  isLoadArgument,
  isLoadLocal,
  isLoadStatic,
  isStoreArgument,
  isStoreLocal,
  isStoreStatic,
  isJumpOperand,
  mergeValues,
  pointerTargetBeforeIndex,
  pointerTargetFromSlotFlow,
  pointerValue,
  popMany,
  popValue,
  propagateCallArguments,
  relativePointerTarget,
  resolveMethodTarget,
  slotIndex,
  valueToPointer,
} from "./call-graph-helpers.js";
