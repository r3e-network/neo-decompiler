import { formatOperand } from "./disassembler.js";

export function jumpTarget(instruction) {
  const operand = instruction.operand;
  if (operand === null) {
    return null;
  }
  if (operand.kind === "Jump" || operand.kind === "Jump32") {
    return instruction.offset + operand.value;
  }
  return null;
}

export function resolvePackedValue(state, expression) {
  return state.packedValuesByExpression.get(expression) ?? state.packedValuesBySlot.get(expression) ?? null;
}

export function convertTargetName(operand) {
  if (!operand || (operand.kind !== "U8" && operand.kind !== "I8")) {
    return null;
  }
  const byte = operand.kind === "U8" ? operand.value : operand.value & 0xff;
  const targets = {
    0x00: "any",
    0x10: "pointer",
    0x20: "bool",
    0x21: "integer",
    0x28: "bytestring",
    0x30: "buffer",
    0x40: "array",
    0x41: "struct",
    0x48: "map",
    0x60: "interopinterface",
  };
  return targets[byte] ?? null;
}

export function renderUntranslatedInstruction(instruction) {
  const mnemonic =
    instruction.opcode.mnemonic === "UNKNOWN"
      ? `UNKNOWN_0x${instruction.opcode.byte.toString(16).padStart(2, "0").toUpperCase()}`
      : instruction.opcode.mnemonic;
  const operandText = instruction.operand !== null ? ` ${formatOperand(instruction.operand)}` : "";
  return `// ${formatOffset(instruction.offset)}: ${mnemonic}${operandText} (not yet translated)`;
}

export function wrapExpression(value) {
  if (
    /^(?:-?\d+|true|false|null|[A-Za-z_][A-Za-z0-9_]*)$/u.test(value) ||
    /^\[.*\]$/u.test(value) ||
    /^\{.*\}$/u.test(value)
  ) {
    return value;
  }
  if (value.startsWith("(") && value.endsWith(")")) {
    return value;
  }
  return `(${value})`;
}

export function stripOuterParens(value) {
  if (value.startsWith("(") && value.endsWith(")")) {
    // Verify the opening paren actually matches the closing one
    // by tracking brace depth — "(a) + (b)" must NOT be stripped.
    let depth = 0;
    for (let i = 0; i < value.length - 1; i++) {
      if (value[i] === "(") depth++;
      if (value[i] === ")") depth--;
      if (depth === 0) return value; // closed before the final char
    }
    return value.slice(1, -1);
  }
  return value;
}

export function formatOffset(offset) {
  return offset.toString(16).padStart(4, "0").toUpperCase();
}
