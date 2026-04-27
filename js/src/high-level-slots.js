import { stripOuterParens } from "./high-level-utils.js";
import { upperHex } from "./util.js";

/**
 * Decode a little-endian, signed two's-complement byte array (the
 * `PUSHINT128` / `PUSHINT256` operand shape) into a `BigInt` and return
 * its decimal string representation. Empty input is treated as zero.
 * Mirrors the Rust `format_int_bytes_as_decimal` helper.
 */
function decodeSignedLeBigInt(bytes) {
  if (bytes.length === 0) return 0n;
  // Walk the bytes high-to-low, building the unsigned magnitude.
  let magnitude = 0n;
  for (let i = bytes.length - 1; i >= 0; i--) {
    magnitude = (magnitude << 8n) | BigInt(bytes[i]);
  }
  // Two's-complement: if the top bit of the most-significant byte is
  // set, the value is negative.
  const signBit = bytes[bytes.length - 1] & 0x80;
  if (signBit !== 0) {
    const range = 1n << BigInt(bytes.length * 8);
    return magnitude - range;
  }
  return magnitude;
}

/**
 * Decode a `PUSHDATA*` byte payload as a quoted string literal when every
 * byte is printable ASCII or common whitespace; otherwise fall back to the
 * raw `0xHEX` form. Mirrors the Rust `format_pushdata` helper so the two
 * ports render byte-string operands identically.
 */
export function formatPushdata(bytes) {
  if (bytes.length === 0) {
    return '""';
  }
  // Only decode bytes that fit the same printable range Rust accepts:
  // 0x20..=0x7E plus \n, \r, \t. Anything else (UTF-8 multi-byte, NUL,
  // control bytes) stays as hex so binary keys stay unambiguous.
  let decodable = true;
  let decoded = "";
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (b === 0x0A || b === 0x0D || b === 0x09 || (b >= 0x20 && b <= 0x7E)) {
      decoded += String.fromCharCode(b);
    } else {
      decodable = false;
      break;
    }
  }
  if (!decodable) {
    return `0x${upperHex(bytes)}`;
  }
  return `"${decoded.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

const PUSH_LIT_RE = /^PUSH(\d+|M1)$/u;

export function trySlotDeclarations(_statements, instruction) {
  // INITSLOT / INITSSLOT have no observable effect on the lifted
  // source — they declare slot capacity, which is implicit in
  // subsequent LDLOC/STLOC/LDSFLD/STSFLD usage. Mark them handled so
  // the main lift loop doesn't fall back to the default per-opcode
  // handling, but emit no statement: the slot-declaration trace
  // comment was always informational noise that the postprocess
  // pass had to strip anyway.
  if (
    instruction.opcode.mnemonic === "INITSLOT" &&
    instruction.operand?.kind === "Bytes" &&
    instruction.operand.value.length >= 2
  ) {
    return true;
  }

  if (instruction.opcode.mnemonic === "INITSSLOT" && instruction.operand?.kind === "U8") {
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
  const match = PUSH_LIT_RE.exec(mnemonic);
  if (match) {
    stack.push(match[1] === "M1" ? "-1" : `${Number(match[1])}`);
    return true;
  }
  if (instruction.operand !== null) {
    if (instruction.operand.kind === "U32" && mnemonic === "PUSHA") {
      // PUSHA operand is U32-encoded but represents a signed I32 relative offset.
      // Mirror Rust's `resolve_pusha_display`: when the absolute target
      // resolves to a known method label use `&{label}` (e.g.
      // `&sub_0x000C`); otherwise fall back to `&fn_0xNNNN` with
      // uppercase hex. Earlier this pushed the bare integer (`123`),
      // which lost the function-pointer semantics and conflated PUSHA
      // with PUSHINT operands.
      const signedOffset = instruction.operand.value | 0;
      const target = instruction.offset + signedOffset;
      const labelMap = state.context?.methodLabelsByOffset;
      const resolved = labelMap?.get(target) ?? null;
      const hex = (target >>> 0).toString(16).padStart(4, "0").toUpperCase();
      const expression = resolved ? `&${resolved}` : `&fn_0x${hex}`;
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
      const expression = formatPushdata(instruction.operand.value);
      stack.push(expression);
      packedValuesByExpression.delete(expression);
      return true;
    }
    if (
      instruction.operand.kind === "Bytes" &&
      (mnemonic === "PUSHINT128" || mnemonic === "PUSHINT256")
    ) {
      // The big-integer PUSHINT operands are little-endian, signed
      // two's-complement byte arrays. Decode into a BigInt and render
      // as a decimal literal — mirrors the Rust port's
      // `format_int_bytes_as_decimal`.
      stack.push(`${decodeSignedLeBigInt(instruction.operand.value)}`);
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
  const value = stack.pop() ?? "???";
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
  const value = stack.pop() ?? "???";
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
  const value = stripOuterParens(stack.pop() ?? "???");
  const name = parameterNames[index] ?? `arg${index}`;
  statements.push(`${name} = ${value};`);
  return true;
}

export function slotIndexFromMnemonic(mnemonic, prefix) {
  // Fast path: avoid regex by checking prefix + all-digit suffix directly.
  // Called per-instruction across LDLOC/LDARG/LDSFLD/STLOC/STARG/STSFLD,
  // so micro-cost matters across thousands of calls.
  if (mnemonic.length <= prefix.length || !mnemonic.startsWith(prefix)) {
    return null;
  }
  let value = 0;
  for (let i = prefix.length; i < mnemonic.length; i++) {
    const code = mnemonic.charCodeAt(i);
    if (code < 48 || code > 57) return null;
    value = value * 10 + (code - 48);
  }
  return value;
}
