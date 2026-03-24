import { formatOperand } from "./disassembler.js";

export function renderPseudocode(instructions) {
  let output = "";
  for (const instruction of instructions) {
    output += `${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${renderMnemonic(instruction)}`;
    if (instruction.operand !== null) {
      output += ` ${formatOperand(instruction.operand)}`;
    }
    output += "\n";
  }
  return output;
}

function renderMnemonic(instruction) {
  if (instruction.opcode.mnemonic === "UNKNOWN") {
    return `UNKNOWN_0x${instruction.opcode.byte
      .toString(16)
      .padStart(2, "0")
      .toUpperCase()}`;
  }
  return instruction.opcode.mnemonic;
}
