import { DisassemblyError } from "./errors.js";
import { OPCODES } from "./generated/opcodes.js";
import {
  asUint8Array,
  readI16LE,
  readI32LE,
  readI64LE,
  readU16LE,
  readU32LE,
  upperHex,
} from "./util.js";

const MAX_OPERAND_LEN = 1_048_576;

export function disassembleScript(input, options = {}) {
  const bytecode = asUint8Array(input);
  const failOnUnknownOpcodes = options.failOnUnknownOpcodes ?? false;
  const instructions = [];
  const warnings = [];
  let offset = 0;

  while (offset < bytecode.byteLength) {
    const opcodeByte = bytecode[offset];
    const opcode = OPCODES.get(opcodeByte);
    if (!opcode) {
      if (failOnUnknownOpcodes) {
        throw new DisassemblyError(
          `unknown opcode 0x${opcodeByte.toString(16).padStart(2, "0").toUpperCase()} at offset ${offset}`,
          { code: "UnknownOpcode", opcode: opcodeByte, offset },
        );
      }
      warnings.push(
        `disassembly: unknown opcode 0x${opcodeByte.toString(16).padStart(2, "0").toUpperCase()} at 0x${offset.toString(16).padStart(4, "0").toUpperCase()}; continuing may desynchronize output`,
      );
      instructions.push({
        offset,
        opcode: {
          name: "Unknown",
          mnemonic: "UNKNOWN",
          byte: opcodeByte,
        },
        operand: null,
      });
      offset += 1;
      continue;
    }

    const operandResult = readOperand(opcode, bytecode, offset);
    instructions.push({
      offset,
      opcode,
      operand: operandResult.operand,
    });
    offset += 1 + operandResult.consumed;
  }

  return { instructions, warnings };
}

export function formatOperand(operand) {
  if (operand === null) {
    return "";
  }
  switch (operand.kind) {
    case "I8":
    case "I16":
    case "I32":
    case "I64":
    case "U8":
    case "U16":
    case "U32":
      return `${operand.value}`;
    case "Bool":
      return operand.value ? "true" : "false";
    case "Null":
      return "null";
    case "Bytes":
      return `0x${upperHex(operand.value)}`;
    case "Jump":
    case "Jump32":
      return `${operand.value}`;
    case "Syscall":
      return `0x${operand.value.toString(16).padStart(8, "0").toUpperCase()}`;
    default:
      return String(operand.value);
  }
}

function readOperand(opcode, bytecode, offset) {
  const immediate = immediateConstant(opcode.mnemonic);
  if (immediate) {
    return { operand: immediate, consumed: 0 };
  }

  const encoding = opcode.operandEncoding;
  switch (encoding.kind) {
    case "None":
      return { operand: null, consumed: 0 };
    case "I8": {
      readSlice(bytecode, offset + 1, 1, offset);
      return { operand: { kind: "I8", value: new Int8Array(bytecode.buffer, bytecode.byteOffset + offset + 1, 1)[0] }, consumed: 1 };
    }
    case "I16": {
      readSlice(bytecode, offset + 1, 2, offset);
      return { operand: { kind: "I16", value: readI16LE(bytecode, offset + 1) }, consumed: 2 };
    }
    case "I32": {
      readSlice(bytecode, offset + 1, 4, offset);
      return { operand: { kind: "I32", value: readI32LE(bytecode, offset + 1) }, consumed: 4 };
    }
    case "I64": {
      readSlice(bytecode, offset + 1, 8, offset);
      return { operand: { kind: "I64", value: readI64LE(bytecode, offset + 1).toString() }, consumed: 8 };
    }
    case "Bytes":
      return {
        operand: { kind: "Bytes", value: readSlice(bytecode, offset + 1, encoding.length, offset) },
        consumed: encoding.length,
      };
    case "Data1":
      return readPrefixedBytes(bytecode, offset, 1);
    case "Data2":
      return readPrefixedBytes(bytecode, offset, 2);
    case "Data4":
      return readPrefixedBytes(bytecode, offset, 4);
    case "Jump8": {
      readSlice(bytecode, offset + 1, 1, offset);
      return { operand: { kind: "Jump", value: new Int8Array(bytecode.buffer, bytecode.byteOffset + offset + 1, 1)[0] }, consumed: 1 };
    }
    case "Jump32": {
      readSlice(bytecode, offset + 1, 4, offset);
      return { operand: { kind: "Jump32", value: readI32LE(bytecode, offset + 1) }, consumed: 4 };
    }
    case "U8":
      return { operand: { kind: "U8", value: readSlice(bytecode, offset + 1, 1, offset)[0] }, consumed: 1 };
    case "U16": {
      readSlice(bytecode, offset + 1, 2, offset);
      return { operand: { kind: "U16", value: readU16LE(bytecode, offset + 1) }, consumed: 2 };
    }
    case "U32": {
      readSlice(bytecode, offset + 1, 4, offset);
      return { operand: { kind: "U32", value: readU32LE(bytecode, offset + 1) }, consumed: 4 };
    }
    case "Syscall": {
      readSlice(bytecode, offset + 1, 4, offset);
      return { operand: { kind: "Syscall", value: readU32LE(bytecode, offset + 1) }, consumed: 4 };
    }
    default:
      return { operand: null, consumed: 0 };
  }
}

function readPrefixedBytes(bytecode, offset, prefixLength) {
  const length = readLength(bytecode, offset + 1, prefixLength, offset);
  if (length > MAX_OPERAND_LEN) {
    throw new DisassemblyError(
      `operand length ${length} exceeds maximum at offset ${offset}`,
      { code: "OperandTooLarge", offset, len: length },
    );
  }
  return {
    operand: {
      kind: "Bytes",
      value: readSlice(bytecode, offset + 1 + prefixLength, length, offset),
    },
    consumed: prefixLength + length,
  };
}

function readLength(bytecode, start, prefixLength, offset) {
  switch (prefixLength) {
    case 1:
      return readSlice(bytecode, start, 1, offset)[0];
    case 2:
      return readU16LE(bytecode, start);
    case 4:
      return readU32LE(bytecode, start);
    default:
      return 0;
  }
}

function readSlice(bytecode, start, length, offset) {
  const end = start + length;
  if (end > bytecode.byteLength) {
    throw new DisassemblyError(`unexpected end of bytecode at offset ${offset}`, {
      code: "UnexpectedEof",
      offset,
    });
  }
  return bytecode.subarray(start, end);
}

function immediateConstant(mnemonic) {
  if (mnemonic === "PUSHNULL") return { kind: "Null", value: null };
  if (mnemonic === "PUSHT") return { kind: "Bool", value: true };
  if (mnemonic === "PUSHF") return { kind: "Bool", value: false };
  const match = mnemonic.match(/^PUSH(\d+|M1)$/u);
  if (!match) return null;
  if (match[1] === "M1") return { kind: "I32", value: -1 };
  return { kind: "I32", value: Number(match[1]) };
}
