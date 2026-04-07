import { formatOperand } from "./disassembler.js";
import { stripOuterParens } from "./high-level-utils.js";

export function trySlotDeclarations(statements, instruction) {
  if (
    instruction.opcode.mnemonic === "INITSLOT" &&
    instruction.operand?.kind === "Bytes" &&
    instruction.operand.value.length >= 2
  ) {
    const [locals, args] = instruction.operand.value;
    statements.push(`// declare ${locals} locals, ${args} arguments`);
    return true;
  }

  if (instruction.opcode.mnemonic === "INITSSLOT" && instruction.operand?.kind === "U8") {
    statements.push(`// declare ${instruction.operand.value} static slots`);
    return true;
  }

  return false;
}

export function pushImmediate(state, instruction) {
  const { stack, pointerTargetsByExpression, packedValuesByExpression } = state;
  const mnemonic = instruction.opcode.mnemonic;
  if (mnemonic === "PUSHNULL") {
    stack.push("null");
    return true;
  }
  if (mnemonic === "PUSHT") {
    stack.push("true");
    return true;
  }
  if (mnemonic === "PUSHF") {
    stack.push("false");
    return true;
  }
  const match = mnemonic.match(/^PUSH(\d+|M1)$/u);
  if (match) {
    stack.push(match[1] === "M1" ? "-1" : `${Number(match[1])}`);
    return true;
  }
  if (instruction.operand !== null) {
    if (instruction.operand.kind === "U32" && mnemonic === "PUSHA") {
      // PUSHA operand is U32-encoded but represents a signed I32 relative offset
      const signedOffset = instruction.operand.value | 0;
      const target = instruction.offset + signedOffset;
      const expression = `${target}`;
      stack.push(expression);
      pointerTargetsByExpression.set(expression, target);
      return true;
    }
    if (
      instruction.operand.kind === "I8" ||
      instruction.operand.kind === "I16" ||
      instruction.operand.kind === "I32" ||
      instruction.operand.kind === "I64"
    ) {
      if (mnemonic.startsWith("PUSHINT")) {
        stack.push(`${instruction.operand.value}`);
        return true;
      }
    }
    if (instruction.operand.kind === "Bytes" && mnemonic.startsWith("PUSHDATA")) {
      const expression = formatOperand(instruction.operand);
      stack.push(expression);
      packedValuesByExpression.delete(expression);
      return true;
    }
  }
  return false;
}

export function tryLoadLocalOrArg(stack, mnemonic, parameterNames, instruction) {
  const local = slotIndexFromMnemonic(mnemonic, "LDLOC");
  if (local !== null) {
    stack.push(`loc${local}`);
    return true;
  }
  if (mnemonic === "LDLOC") {
    const index = instruction.operand?.value;
    if (typeof index === "number") {
      stack.push(`loc${index}`);
      return true;
    }
  }
  const arg = slotIndexFromMnemonic(mnemonic, "LDARG");
  if (arg !== null) {
    stack.push(parameterNames[arg] ?? `arg${arg}`);
    return true;
  }
  if (mnemonic === "LDARG") {
    const index = instruction.operand?.value;
    if (typeof index === "number") {
      stack.push(parameterNames[index] ?? `arg${index}`);
      return true;
    }
  }
  return false;
}

export function tryLoadStatic(stack, mnemonic, instruction) {
  const index = slotIndexFromMnemonic(mnemonic, "LDSFLD");
  if (index !== null) {
    stack.push(`static${index}`);
    return true;
  }
  if (mnemonic === "LDSFLD") {
    const index = instruction.operand?.value;
    if (typeof index === "number") {
      stack.push(`static${index}`);
      return true;
    }
  }
  return false;
}

export function tryStoreLocal(
  statements,
  stack,
  initializedLocals,
  pointerTargetsByExpression,
  pointerTargetsBySlot,
  packedValuesByExpression,
  packedValuesBySlot,
  mnemonic,
  instruction,
) {
  let local = slotIndexFromMnemonic(mnemonic, "STLOC");
  if (local === null && mnemonic === "STLOC") {
    local = instruction.operand?.value ?? null;
  }
  if (local === null) {
    return false;
  }
  const value = stack.pop() ?? "/* stack_underflow */";
  const name = `loc${local}`;
  const stripped = stripOuterParens(value);
  const pointerTarget = pointerTargetsByExpression.get(stripped);
  if (pointerTarget !== undefined) {
    pointerTargetsBySlot.set(name, pointerTarget);
  } else {
    pointerTargetsBySlot.delete(name);
  }
  const packedValue = packedValuesByExpression.get(stripped);
  if (packedValue !== undefined) {
    packedValuesBySlot.set(name, [...packedValue]);
  } else {
    packedValuesBySlot.delete(name);
  }
  if (initializedLocals.has(local)) {
    statements.push(`${name} = ${stripped};`);
  } else {
    initializedLocals.add(local);
    statements.push(`let ${name} = ${stripped};`);
  }
  return true;
}

export function tryStoreStatic(
  statements,
  stack,
  initializedStatics,
  pointerTargetsByExpression,
  pointerTargetsBySlot,
  packedValuesByExpression,
  packedValuesBySlot,
  mnemonic,
  instruction,
) {
  let index = slotIndexFromMnemonic(mnemonic, "STSFLD");
  if (index === null && mnemonic === "STSFLD") {
    index = instruction.operand?.value ?? null;
  }
  if (index === null) {
    return false;
  }
  const value = stack.pop() ?? "/* stack_underflow */";
  const name = `static${index}`;
  const stripped = stripOuterParens(value);
  const pointerTarget = pointerTargetsByExpression.get(stripped);
  if (pointerTarget !== undefined) {
    pointerTargetsBySlot.set(name, pointerTarget);
  } else {
    pointerTargetsBySlot.delete(name);
  }
  const packedValue = packedValuesByExpression.get(stripped);
  if (packedValue !== undefined) {
    packedValuesBySlot.set(name, [...packedValue]);
  } else {
    packedValuesBySlot.delete(name);
  }
  if (initializedStatics.has(index)) {
    statements.push(`${name} = ${stripped};`);
  } else {
    initializedStatics.add(index);
    statements.push(`let ${name} = ${stripped};`);
  }
  return true;
}

export function tryStoreArgument(statements, stack, parameterNames, mnemonic, instruction) {
  let index = slotIndexFromMnemonic(mnemonic, "STARG");
  if (index === null && mnemonic === "STARG") {
    index = instruction.operand?.value ?? null;
  }
  if (index === null) {
    return false;
  }
  const value = stripOuterParens(stack.pop() ?? "/* stack_underflow */");
  const name = parameterNames[index] ?? `arg${index}`;
  statements.push(`${name} = ${value};`);
  return true;
}

const _slotRegexCache = new Map();

export function slotIndexFromMnemonic(mnemonic, prefix) {
  if (mnemonic === prefix) {
    return null;
  }
  let regex = _slotRegexCache.get(prefix);
  if (!regex) {
    regex = new RegExp(`^${prefix}(\\d+)$`, "u");
    _slotRegexCache.set(prefix, regex);
  }
  const match = mnemonic.match(regex);
  if (match) {
    return Number(match[1]);
  }
  return null;
}
