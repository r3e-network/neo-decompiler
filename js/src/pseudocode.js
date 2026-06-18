import { formatOperand } from "./disassembler.js";
import { hex8, hexOffset } from "./util.js";

export function renderPseudocode(instructions) {
  const lines = [];
  for (let i = 0; i < instructions.length; i++) {
    const instruction = instructions[i];
    let line = `${hexOffset(instruction.offset)}: ${renderMnemonic(instruction)}`;
    if (instruction.operand !== null) {
      line += ` ${formatOperand(instruction.operand)}`;
    }
    lines.push(line);
  }
  return lines.join("\n") + (instructions.length > 0 ? "\n" : "");
}

function renderMnemonic(instruction) {
  if (instruction.opcode.mnemonic === "UNKNOWN") {
    return `UNKNOWN_0x${hex8(instruction.opcode.byte)}`;
  }
  return instruction.opcode.mnemonic;
}
