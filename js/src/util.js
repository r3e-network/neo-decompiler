import { createHash } from "node:crypto";

export function asUint8Array(value) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  throw new TypeError("expected Uint8Array-compatible input");
}

const HEX_TABLE = Array.from({ length: 256 }, (_, i) =>
  i.toString(16).padStart(2, "0").toUpperCase(),
);

export function upperHex(bytes) {
  let out = "";
  for (let i = 0; i < bytes.length; i++) {
    out += HEX_TABLE[bytes[i]];
  }
  return out;
}

export function upperHexReversed(bytes) {
  let out = "";
  for (let i = bytes.length - 1; i >= 0; i--) {
    out += HEX_TABLE[bytes[i]];
  }
  return out;
}

// 1-byte (2-hex-digit) zero-padded uppercase hex of an unsigned 8-bit value.
export function hex8(value) {
  return HEX_TABLE[value & 0xff];
}

// 2-byte (4-hex-digit) zero-padded uppercase hex of an unsigned 16-bit value.
export function hex16(value) {
  return HEX_TABLE[(value >>> 8) & 0xff] + HEX_TABLE[value & 0xff];
}

// 4-byte (8-hex-digit) zero-padded uppercase hex of an unsigned 32-bit value.
export function hex32(value) {
  return (
    HEX_TABLE[(value >>> 24) & 0xff] +
    HEX_TABLE[(value >>> 16) & 0xff] +
    HEX_TABLE[(value >>> 8) & 0xff] +
    HEX_TABLE[value & 0xff]
  );
}

export function readU16LE(bytes, offset) {
  return bytes[offset] | (bytes[offset + 1] << 8);
}

export function readI16LE(bytes, offset) {
  return ((bytes[offset] | (bytes[offset + 1] << 8)) << 16) >> 16;
}

export function readU32LE(bytes, offset) {
  return (
    (bytes[offset] |
      (bytes[offset + 1] << 8) |
      (bytes[offset + 2] << 16) |
      (bytes[offset + 3] << 24)) >>>
    0
  );
}

export function readI32LE(bytes, offset) {
  return (
    bytes[offset] |
    (bytes[offset + 1] << 8) |
    (bytes[offset + 2] << 16) |
    (bytes[offset + 3] << 24)
  );
}

export function readI64LE(bytes, offset) {
  const lo = BigInt(readU32LE(bytes, offset));
  const hi = BigInt(readI32LE(bytes, offset + 4));
  return (hi << 32n) | lo;
}

export function readU64LE(bytes, offset) {
  const lo = BigInt(readU32LE(bytes, offset));
  const hi = BigInt(readU32LE(bytes, offset + 4));
  return (hi << 32n) | lo;
}

export function computeChecksum(bytes) {
  const first = createHash("sha256").update(Buffer.from(bytes)).digest();
  const second = createHash("sha256").update(first).digest();
  return new Uint8Array(second.subarray(0, 4));
}

export function computeScriptHash(script) {
  const sha256 = createHash("sha256").update(Buffer.from(script)).digest();
  const ripemd160 = createHash("ripemd160").update(sha256).digest();
  return new Uint8Array(ripemd160);
}

export function scanSlotCounts(instructions) {
  for (const instruction of instructions) {
    if (
      instruction.opcode.mnemonic === "INITSLOT" &&
      instruction.operand?.kind === "Bytes" &&
      instruction.operand.value.length >= 2
    ) {
      return [instruction.operand.value[0], instruction.operand.value[1]];
    }
  }

  let maxLocal = -1;
  let maxArg = -1;
  for (const instruction of instructions) {
    const mnemonic = instruction.opcode.mnemonic;
    if (LOC_LD_ST_RE.test(mnemonic)) {
      maxLocal = Math.max(maxLocal, slotIndex(mnemonic, instruction));
    }
    if (ARG_LD_ST_RE.test(mnemonic)) {
      maxArg = Math.max(maxArg, slotIndex(mnemonic, instruction));
    }
  }
  return [maxLocal + 1, maxArg + 1];
}

const LOC_LD_ST_RE = /^(?:LD|ST)LOC(?:\d+)?$/u;
const ARG_LD_ST_RE = /^(?:LD|ST)ARG(?:\d+)?$/u;
const SLOT_INDEX_RE = /(?:LD|ST)(?:LOC|ARG|SFLD)(\d+)$/u;

export function scanStaticSlotCount(instructions) {
  for (const instruction of instructions) {
    if (instruction.opcode.mnemonic === "INITSSLOT" && instruction.operand?.kind === "U8") {
      return instruction.operand.value;
    }
  }
  return 0;
}

export function slotIndex(mnemonic, instruction) {
  const exact = SLOT_INDEX_RE.exec(mnemonic);
  if (exact) {
    return Number(exact[1]);
  }
  if (instruction.operand?.kind === "U8") {
    return instruction.operand.value;
  }
  return 0;
}
