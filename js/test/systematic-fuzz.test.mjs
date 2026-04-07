/**
 * Systematic Fuzz Tests for Neo Decompiler JS
 *
 * Covers:
 *  1. Opcode coverage matrix
 *  2. Stack depth stress
 *  3. Control flow torture
 *  4. Operand boundary testing
 *  5. Method token combinations
 *  6. Manifest + NEF interaction
 *  7. Structured grammar fuzzing
 *  8. Regression patterns
 */

import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";

import {
  parseNef,
  disassembleScript,
  decompileBytes,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
  analyzeBytes,
} from "../src/index.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

function computeChecksum(payload) {
  const first = createHash("sha256").update(Buffer.from(payload)).digest();
  const second = createHash("sha256").update(first).digest();
  return new Uint8Array(second.subarray(0, 4));
}

function writeVarint(buffer, value) {
  if (value <= 0xfc) {
    buffer.push(value);
  } else if (value <= 0xffff) {
    buffer.push(0xfd, value & 0xff, value >> 8);
  } else {
    buffer.push(
      0xfe,
      value & 0xff,
      (value >> 8) & 0xff,
      (value >> 16) & 0xff,
      (value >> 24) & 0xff,
    );
  }
}

function buildNef(opts = {}) {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from(opts.compiler ?? "test"), 0);
  data.push(...compiler);

  const source = opts.source ?? "";
  writeVarint(data, Buffer.byteLength(source));
  data.push(...Buffer.from(source));

  data.push(0); // reserved byte

  const tokens = opts.tokens ?? [];
  writeVarint(data, tokens.length);
  for (const token of tokens) {
    data.push(...token.hash);
    writeVarint(data, Buffer.byteLength(token.method));
    data.push(...Buffer.from(token.method));
    data.push(token.params & 0xff, (token.params >> 8) & 0xff);
    data.push(token.hasReturn ? 1 : 0);
    data.push(token.callFlags ?? 0x0f);
  }

  data.push(0x00, 0x00); // reserved word

  const script = Array.from(opts.script ?? [0x11, 0x40]);
  writeVarint(data, script.length);
  data.push(...script);

  const checksum = computeChecksum(data);
  data.push(...checksum);

  return new Uint8Array(data);
}

function buildValidNef(script) {
  return buildNef({ script });
}

/** Seeded PRNG for reproducible fuzzing (xorshift32). */
function createRng(seed) {
  let state = seed | 0 || 1;
  return {
    next() {
      state ^= state << 13;
      state ^= state >> 17;
      state ^= state << 5;
      return (state >>> 0) / 0x100000000;
    },
    nextInt(max) {
      return Math.floor(this.next() * max);
    },
    nextByte() {
      return this.nextInt(256);
    },
  };
}

function randomBytes(rng, length) {
  return new Uint8Array(length).map(() => rng.nextByte());
}

/**
 * Run a function, catching errors. Returns true if it completed or threw a
 * controlled Error. Fails the test only on non-Error throws.
 */
function mustNotCrash(fn, label) {
  try {
    fn();
    return true;
  } catch (e) {
    assert.ok(
      e instanceof Error,
      `${label}: threw non-Error value: ${String(e)}`,
    );
    return false;
  }
}

// Complete opcode table extracted from opcodes.js
const ALL_OPCODES = [
  // Push
  { byte: 0x00, mnemonic: "PUSHINT8",    encoding: "I8" },
  { byte: 0x01, mnemonic: "PUSHINT16",   encoding: "I16" },
  { byte: 0x02, mnemonic: "PUSHINT32",   encoding: "I32" },
  { byte: 0x03, mnemonic: "PUSHINT64",   encoding: "I64" },
  { byte: 0x04, mnemonic: "PUSHINT128",  encoding: "Bytes16" },
  { byte: 0x05, mnemonic: "PUSHINT256",  encoding: "Bytes32" },
  { byte: 0x08, mnemonic: "PUSHT",       encoding: "None" },
  { byte: 0x09, mnemonic: "PUSHF",       encoding: "None" },
  { byte: 0x0A, mnemonic: "PUSHA",       encoding: "U32" },
  { byte: 0x0B, mnemonic: "PUSHNULL",    encoding: "None" },
  { byte: 0x0C, mnemonic: "PUSHDATA1",   encoding: "Data1" },
  { byte: 0x0D, mnemonic: "PUSHDATA2",   encoding: "Data2" },
  { byte: 0x0E, mnemonic: "PUSHDATA4",   encoding: "Data4" },
  { byte: 0x0F, mnemonic: "PUSHM1",      encoding: "None" },
  { byte: 0x10, mnemonic: "PUSH0",       encoding: "None" },
  { byte: 0x11, mnemonic: "PUSH1",       encoding: "None" },
  { byte: 0x12, mnemonic: "PUSH2",       encoding: "None" },
  { byte: 0x13, mnemonic: "PUSH3",       encoding: "None" },
  { byte: 0x14, mnemonic: "PUSH4",       encoding: "None" },
  { byte: 0x15, mnemonic: "PUSH5",       encoding: "None" },
  { byte: 0x16, mnemonic: "PUSH6",       encoding: "None" },
  { byte: 0x17, mnemonic: "PUSH7",       encoding: "None" },
  { byte: 0x18, mnemonic: "PUSH8",       encoding: "None" },
  { byte: 0x19, mnemonic: "PUSH9",       encoding: "None" },
  { byte: 0x1A, mnemonic: "PUSH10",      encoding: "None" },
  { byte: 0x1B, mnemonic: "PUSH11",      encoding: "None" },
  { byte: 0x1C, mnemonic: "PUSH12",      encoding: "None" },
  { byte: 0x1D, mnemonic: "PUSH13",      encoding: "None" },
  { byte: 0x1E, mnemonic: "PUSH14",      encoding: "None" },
  { byte: 0x1F, mnemonic: "PUSH15",      encoding: "None" },
  { byte: 0x20, mnemonic: "PUSH16",      encoding: "None" },
  // Flow
  { byte: 0x21, mnemonic: "NOP",         encoding: "None" },
  { byte: 0x22, mnemonic: "JMP",         encoding: "Jump8" },
  { byte: 0x23, mnemonic: "JMP_L",       encoding: "Jump32" },
  { byte: 0x24, mnemonic: "JMPIF",       encoding: "Jump8" },
  { byte: 0x25, mnemonic: "JMPIF_L",     encoding: "Jump32" },
  { byte: 0x26, mnemonic: "JMPIFNOT",    encoding: "Jump8" },
  { byte: 0x27, mnemonic: "JMPIFNOT_L",  encoding: "Jump32" },
  { byte: 0x28, mnemonic: "JMPEQ",       encoding: "Jump8" },
  { byte: 0x29, mnemonic: "JMPEQ_L",     encoding: "Jump32" },
  { byte: 0x2A, mnemonic: "JMPNE",       encoding: "Jump8" },
  { byte: 0x2B, mnemonic: "JMPNE_L",     encoding: "Jump32" },
  { byte: 0x2C, mnemonic: "JMPGT",       encoding: "Jump8" },
  { byte: 0x2D, mnemonic: "JMPGT_L",     encoding: "Jump32" },
  { byte: 0x2E, mnemonic: "JMPGE",       encoding: "Jump8" },
  { byte: 0x2F, mnemonic: "JMPGE_L",     encoding: "Jump32" },
  { byte: 0x30, mnemonic: "JMPLT",       encoding: "Jump8" },
  { byte: 0x31, mnemonic: "JMPLT_L",     encoding: "Jump32" },
  { byte: 0x32, mnemonic: "JMPLE",       encoding: "Jump8" },
  { byte: 0x33, mnemonic: "JMPLE_L",     encoding: "Jump32" },
  { byte: 0x34, mnemonic: "CALL",        encoding: "Jump8" },
  { byte: 0x35, mnemonic: "CALL_L",      encoding: "Jump32" },
  { byte: 0x36, mnemonic: "CALLA",       encoding: "None" },
  { byte: 0x37, mnemonic: "CALLT",       encoding: "U16" },
  { byte: 0x38, mnemonic: "ABORT",       encoding: "None" },
  { byte: 0x39, mnemonic: "ASSERT",      encoding: "None" },
  { byte: 0x3A, mnemonic: "THROW",       encoding: "None" },
  { byte: 0x3B, mnemonic: "TRY",         encoding: "Bytes2" },
  { byte: 0x3C, mnemonic: "TRY_L",       encoding: "Bytes8" },
  { byte: 0x3D, mnemonic: "ENDTRY",      encoding: "Jump8" },
  { byte: 0x3E, mnemonic: "ENDTRY_L",    encoding: "Jump32" },
  { byte: 0x3F, mnemonic: "ENDFINALLY",  encoding: "None" },
  { byte: 0x40, mnemonic: "RET",         encoding: "None" },
  { byte: 0x41, mnemonic: "SYSCALL",     encoding: "Syscall" },
  // Stack
  { byte: 0x43, mnemonic: "DEPTH",       encoding: "None" },
  { byte: 0x45, mnemonic: "DROP",        encoding: "None" },
  { byte: 0x46, mnemonic: "NIP",         encoding: "None" },
  { byte: 0x48, mnemonic: "XDROP",       encoding: "None" },
  { byte: 0x49, mnemonic: "CLEAR",       encoding: "None" },
  { byte: 0x4A, mnemonic: "DUP",         encoding: "None" },
  { byte: 0x4B, mnemonic: "OVER",        encoding: "None" },
  { byte: 0x4D, mnemonic: "PICK",        encoding: "None" },
  { byte: 0x4E, mnemonic: "TUCK",        encoding: "None" },
  { byte: 0x50, mnemonic: "SWAP",        encoding: "None" },
  { byte: 0x51, mnemonic: "ROT",         encoding: "None" },
  { byte: 0x52, mnemonic: "ROLL",        encoding: "None" },
  { byte: 0x53, mnemonic: "REVERSE3",    encoding: "None" },
  { byte: 0x54, mnemonic: "REVERSE4",    encoding: "None" },
  { byte: 0x55, mnemonic: "REVERSEN",    encoding: "None" },
  // Slots
  { byte: 0x56, mnemonic: "INITSSLOT",   encoding: "U8" },
  { byte: 0x57, mnemonic: "INITSLOT",    encoding: "Bytes2" },
  { byte: 0x58, mnemonic: "LDSFLD0",     encoding: "None" },
  { byte: 0x59, mnemonic: "LDSFLD1",     encoding: "None" },
  { byte: 0x5A, mnemonic: "LDSFLD2",     encoding: "None" },
  { byte: 0x5B, mnemonic: "LDSFLD3",     encoding: "None" },
  { byte: 0x5C, mnemonic: "LDSFLD4",     encoding: "None" },
  { byte: 0x5D, mnemonic: "LDSFLD5",     encoding: "None" },
  { byte: 0x5E, mnemonic: "LDSFLD6",     encoding: "None" },
  { byte: 0x5F, mnemonic: "LDSFLD",      encoding: "U8" },
  { byte: 0x60, mnemonic: "STSFLD0",     encoding: "None" },
  { byte: 0x61, mnemonic: "STSFLD1",     encoding: "None" },
  { byte: 0x62, mnemonic: "STSFLD2",     encoding: "None" },
  { byte: 0x63, mnemonic: "STSFLD3",     encoding: "None" },
  { byte: 0x64, mnemonic: "STSFLD4",     encoding: "None" },
  { byte: 0x65, mnemonic: "STSFLD5",     encoding: "None" },
  { byte: 0x66, mnemonic: "STSFLD6",     encoding: "None" },
  { byte: 0x67, mnemonic: "STSFLD",      encoding: "U8" },
  { byte: 0x68, mnemonic: "LDLOC0",      encoding: "None" },
  { byte: 0x69, mnemonic: "LDLOC1",      encoding: "None" },
  { byte: 0x6A, mnemonic: "LDLOC2",      encoding: "None" },
  { byte: 0x6B, mnemonic: "LDLOC3",      encoding: "None" },
  { byte: 0x6C, mnemonic: "LDLOC4",      encoding: "None" },
  { byte: 0x6D, mnemonic: "LDLOC5",      encoding: "None" },
  { byte: 0x6E, mnemonic: "LDLOC6",      encoding: "None" },
  { byte: 0x6F, mnemonic: "LDLOC",       encoding: "U8" },
  { byte: 0x70, mnemonic: "STLOC0",      encoding: "None" },
  { byte: 0x71, mnemonic: "STLOC1",      encoding: "None" },
  { byte: 0x72, mnemonic: "STLOC2",      encoding: "None" },
  { byte: 0x73, mnemonic: "STLOC3",      encoding: "None" },
  { byte: 0x74, mnemonic: "STLOC4",      encoding: "None" },
  { byte: 0x75, mnemonic: "STLOC5",      encoding: "None" },
  { byte: 0x76, mnemonic: "STLOC6",      encoding: "None" },
  { byte: 0x77, mnemonic: "STLOC",       encoding: "U8" },
  { byte: 0x78, mnemonic: "LDARG0",      encoding: "None" },
  { byte: 0x79, mnemonic: "LDARG1",      encoding: "None" },
  { byte: 0x7A, mnemonic: "LDARG2",      encoding: "None" },
  { byte: 0x7B, mnemonic: "LDARG3",      encoding: "None" },
  { byte: 0x7C, mnemonic: "LDARG4",      encoding: "None" },
  { byte: 0x7D, mnemonic: "LDARG5",      encoding: "None" },
  { byte: 0x7E, mnemonic: "LDARG6",      encoding: "None" },
  { byte: 0x7F, mnemonic: "LDARG",       encoding: "U8" },
  { byte: 0x80, mnemonic: "STARG0",      encoding: "None" },
  { byte: 0x81, mnemonic: "STARG1",      encoding: "None" },
  { byte: 0x82, mnemonic: "STARG2",      encoding: "None" },
  { byte: 0x83, mnemonic: "STARG3",      encoding: "None" },
  { byte: 0x84, mnemonic: "STARG4",      encoding: "None" },
  { byte: 0x85, mnemonic: "STARG5",      encoding: "None" },
  { byte: 0x86, mnemonic: "STARG6",      encoding: "None" },
  { byte: 0x87, mnemonic: "STARG",       encoding: "U8" },
  // Buffer
  { byte: 0x88, mnemonic: "NEWBUFFER",   encoding: "None" },
  { byte: 0x89, mnemonic: "MEMCPY",      encoding: "None" },
  { byte: 0x8B, mnemonic: "CAT",         encoding: "None" },
  { byte: 0x8C, mnemonic: "SUBSTR",      encoding: "None" },
  { byte: 0x8D, mnemonic: "LEFT",        encoding: "None" },
  { byte: 0x8E, mnemonic: "RIGHT",       encoding: "None" },
  // Numeric
  { byte: 0x90, mnemonic: "INVERT",      encoding: "None" },
  { byte: 0x91, mnemonic: "AND",         encoding: "None" },
  { byte: 0x92, mnemonic: "OR",          encoding: "None" },
  { byte: 0x93, mnemonic: "XOR",         encoding: "None" },
  { byte: 0x97, mnemonic: "EQUAL",       encoding: "None" },
  { byte: 0x98, mnemonic: "NOTEQUAL",    encoding: "None" },
  { byte: 0x99, mnemonic: "SIGN",        encoding: "None" },
  { byte: 0x9A, mnemonic: "ABS",         encoding: "None" },
  { byte: 0x9B, mnemonic: "NEGATE",      encoding: "None" },
  { byte: 0x9C, mnemonic: "INC",         encoding: "None" },
  { byte: 0x9D, mnemonic: "DEC",         encoding: "None" },
  { byte: 0x9E, mnemonic: "ADD",         encoding: "None" },
  { byte: 0x9F, mnemonic: "SUB",         encoding: "None" },
  { byte: 0xA0, mnemonic: "MUL",         encoding: "None" },
  { byte: 0xA1, mnemonic: "DIV",         encoding: "None" },
  { byte: 0xA2, mnemonic: "MOD",         encoding: "None" },
  { byte: 0xA3, mnemonic: "POW",         encoding: "None" },
  { byte: 0xA4, mnemonic: "SQRT",        encoding: "None" },
  { byte: 0xA5, mnemonic: "MODMUL",      encoding: "None" },
  { byte: 0xA6, mnemonic: "MODPOW",      encoding: "None" },
  { byte: 0xA8, mnemonic: "SHL",         encoding: "None" },
  { byte: 0xA9, mnemonic: "SHR",         encoding: "None" },
  { byte: 0xAA, mnemonic: "NOT",         encoding: "None" },
  { byte: 0xAB, mnemonic: "BOOLAND",     encoding: "None" },
  { byte: 0xAC, mnemonic: "BOOLOR",      encoding: "None" },
  { byte: 0xB1, mnemonic: "NZ",          encoding: "None" },
  { byte: 0xB3, mnemonic: "NUMEQUAL",    encoding: "None" },
  { byte: 0xB4, mnemonic: "NUMNOTEQUAL", encoding: "None" },
  { byte: 0xB5, mnemonic: "LT",          encoding: "None" },
  { byte: 0xB6, mnemonic: "LE",          encoding: "None" },
  { byte: 0xB7, mnemonic: "GT",          encoding: "None" },
  { byte: 0xB8, mnemonic: "GE",          encoding: "None" },
  { byte: 0xB9, mnemonic: "MIN",         encoding: "None" },
  { byte: 0xBA, mnemonic: "MAX",         encoding: "None" },
  { byte: 0xBB, mnemonic: "WITHIN",      encoding: "None" },
  // Compound
  { byte: 0xBE, mnemonic: "PACKMAP",     encoding: "None" },
  { byte: 0xBF, mnemonic: "PACKSTRUCT",  encoding: "None" },
  { byte: 0xC0, mnemonic: "PACK",        encoding: "None" },
  { byte: 0xC1, mnemonic: "UNPACK",      encoding: "None" },
  { byte: 0xC2, mnemonic: "NEWARRAY0",   encoding: "None" },
  { byte: 0xC3, mnemonic: "NEWARRAY",    encoding: "None" },
  { byte: 0xC4, mnemonic: "NEWARRAY_T",  encoding: "U8" },
  { byte: 0xC5, mnemonic: "NEWSTRUCT0",  encoding: "None" },
  { byte: 0xC6, mnemonic: "NEWSTRUCT",   encoding: "None" },
  { byte: 0xC8, mnemonic: "NEWMAP",      encoding: "None" },
  { byte: 0xCA, mnemonic: "SIZE",        encoding: "None" },
  { byte: 0xCB, mnemonic: "HASKEY",      encoding: "None" },
  { byte: 0xCC, mnemonic: "KEYS",        encoding: "None" },
  { byte: 0xCD, mnemonic: "VALUES",      encoding: "None" },
  { byte: 0xCE, mnemonic: "PICKITEM",    encoding: "None" },
  { byte: 0xCF, mnemonic: "APPEND",      encoding: "None" },
  { byte: 0xD0, mnemonic: "SETITEM",     encoding: "None" },
  { byte: 0xD1, mnemonic: "REVERSEITEMS", encoding: "None" },
  { byte: 0xD2, mnemonic: "REMOVE",      encoding: "None" },
  { byte: 0xD3, mnemonic: "CLEARITEMS",  encoding: "None" },
  { byte: 0xD4, mnemonic: "POPITEM",     encoding: "None" },
  // Type
  { byte: 0xD8, mnemonic: "ISNULL",      encoding: "None" },
  { byte: 0xD9, mnemonic: "ISTYPE",      encoding: "U8" },
  { byte: 0xDB, mnemonic: "CONVERT",     encoding: "U8" },
  // Exception messages
  { byte: 0xE0, mnemonic: "ABORTMSG",    encoding: "None" },
  { byte: 0xE1, mnemonic: "ASSERTMSG",   encoding: "None" },
];

/** Build a minimal valid operand for a given encoding type. */
function operandForEncoding(encoding) {
  switch (encoding) {
    case "None":    return [];
    case "I8":      return [0x01];
    case "I16":     return [0x01, 0x00];
    case "I32":     return [0x01, 0x00, 0x00, 0x00];
    case "I64":     return [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    case "Bytes16": return new Array(16).fill(0);
    case "Bytes32": return new Array(32).fill(0);
    case "U8":      return [0x01];
    case "U16":     return [0x00, 0x00];
    case "U32":     return [0x00, 0x00, 0x00, 0x00];
    case "Jump8":   return [0x02]; // jump forward 2 (skip to RET)
    case "Jump32":  return [0x05, 0x00, 0x00, 0x00]; // jump forward 5
    case "Bytes2":  return [0x00, 0x00];
    case "Bytes8":  return [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    case "Data1":   return [0x01, 0xAB]; // 1 byte of data
    case "Data2":   return [0x01, 0x00, 0xAB]; // 1 byte of data
    case "Data4":   return [0x01, 0x00, 0x00, 0x00, 0xAB]; // 1 byte of data
    case "Syscall": return [0x00, 0x00, 0x00, 0x00];
    default:        return [];
  }
}

/** Size of the operand (not including the opcode byte). */
function operandSize(encoding) {
  return operandForEncoding(encoding).length;
}

// ════════════════════════════════════════════════════════════════════════════
// 1. OPCODE COVERAGE MATRIX
// ════════════════════════════════════════════════════════════════════════════

test("opcode-matrix: every opcode individually through disassemble", () => {
  let passed = 0;
  for (const op of ALL_OPCODES) {
    const scriptBytes = [op.byte, ...operandForEncoding(op.encoding), 0x40];
    const script = new Uint8Array(scriptBytes);
    const ok = mustNotCrash(
      () => disassembleScript(script),
      `disassemble ${op.mnemonic} (0x${op.byte.toString(16).padStart(2, "0")})`,
    );
    if (ok) passed++;
  }
  assert.ok(passed > 0, "at least some opcodes should disassemble");
});

test("opcode-matrix: every opcode individually through decompileBytes", () => {
  for (const op of ALL_OPCODES) {
    const scriptBytes = [op.byte, ...operandForEncoding(op.encoding), 0x40];
    const nef = buildValidNef(new Uint8Array(scriptBytes));
    mustNotCrash(
      () => decompileBytes(nef),
      `decompileBytes ${op.mnemonic}`,
    );
  }
});

test("opcode-matrix: every opcode individually through decompileHighLevelBytes", () => {
  for (const op of ALL_OPCODES) {
    const scriptBytes = [op.byte, ...operandForEncoding(op.encoding), 0x40];
    const nef = buildValidNef(new Uint8Array(scriptBytes));
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `highLevel ${op.mnemonic}`,
    );
  }
});

test("opcode-matrix: all pairs of no-operand opcodes", () => {
  const noOps = ALL_OPCODES.filter((o) => o.encoding === "None");
  let tested = 0;
  // Test a representative subset of pairs (all would be ~15k)
  const rng = createRng(42);
  for (let i = 0; i < 500; i++) {
    const a = noOps[rng.nextInt(noOps.length)];
    const b = noOps[rng.nextInt(noOps.length)];
    const script = new Uint8Array([0x11, 0x12, a.byte, b.byte, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `pair ${a.mnemonic}+${b.mnemonic}`,
    );
    tested++;
  }
  assert.ok(tested === 500);
});

test("opcode-matrix: triple opcode combinations (random sampling)", () => {
  const noOps = ALL_OPCODES.filter((o) => o.encoding === "None");
  const rng = createRng(123);
  for (let i = 0; i < 200; i++) {
    const a = noOps[rng.nextInt(noOps.length)];
    const b = noOps[rng.nextInt(noOps.length)];
    const c = noOps[rng.nextInt(noOps.length)];
    const script = new Uint8Array([0x11, 0x12, 0x13, a.byte, b.byte, c.byte, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `triple ${a.mnemonic}+${b.mnemonic}+${c.mnemonic}`,
    );
  }
});

// ════════════════════════════════════════════════════════════════════════════
// 2. STACK DEPTH STRESS
// ════════════════════════════════════════════════════════════════════════════

test("stack-depth: push 1000 values then consume", () => {
  const script = [];
  for (let i = 0; i < 1000; i++) {
    script.push(0x11); // PUSH1
  }
  // Consume in pairs: 999 ADDs reduce 1000 to 1
  for (let i = 0; i < 999; i++) {
    script.push(0x9E); // ADD
  }
  script.push(0x40); // RET
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "deep push + consume");
});

test("stack-depth: push 2000 values, no consumption (unbalanced)", () => {
  const script = [];
  for (let i = 0; i < 2000; i++) {
    script.push(0x11); // PUSH1
  }
  script.push(0x40); // RET
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "2000 pushes no pop");
});

test("stack-depth: deep DUP chain", () => {
  const script = [0x11]; // PUSH1
  for (let i = 0; i < 500; i++) {
    script.push(0x4A); // DUP
  }
  script.push(0x40); // RET
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "500 DUPs");
});

test("stack-depth: underflow via excessive DROP", () => {
  const script = [];
  for (let i = 0; i < 100; i++) {
    script.push(0x45); // DROP
  }
  script.push(0x40);
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "100 DROPs on empty stack");
});

test("stack-depth: underflow via binary ops on empty stack", () => {
  const script = [0x9E, 0x9F, 0xA0, 0xA1, 0xA2, 0x40]; // ADD SUB MUL DIV MOD RET
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "binary ops on empty stack");
});

test("stack-depth: alternating push/pop 500 times", () => {
  const script = [];
  for (let i = 0; i < 500; i++) {
    script.push(0x11); // PUSH1
    script.push(0x45); // DROP
  }
  script.push(0x40);
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "alternating push/pop 500x");
});

test("stack-depth: SWAP/ROT/ROLL on nearly empty stack", () => {
  const scripts = [
    [0x50, 0x40],             // SWAP on empty
    [0x11, 0x50, 0x40],      // SWAP with 1 item
    [0x51, 0x40],             // ROT on empty
    [0x11, 0x51, 0x40],      // ROT with 1 item
    [0x52, 0x40],             // ROLL on empty
    [0x11, 0x12, 0x52, 0x40], // ROLL with 2 items
  ];
  for (const bytes of scripts) {
    const nef = buildValidNef(new Uint8Array(bytes));
    mustNotCrash(() => decompileHighLevelBytes(nef), "stack ops on shallow stack");
  }
});

test("stack-depth: PICK with index exceeding stack", () => {
  const script = [0x11, 0x20, 0x4D, 0x40]; // PUSH1, PUSH16, PICK, RET
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "PICK past stack");
});

// ════════════════════════════════════════════════════════════════════════════
// 3. CONTROL FLOW TORTURE
// ════════════════════════════════════════════════════════════════════════════

test("control-flow: deeply nested if/else (50 levels)", () => {
  const script = [];
  const depth = 50;
  // Each level: PUSH1 JMPIFNOT +forward NOP
  // We need to compute jump offsets carefully.
  // Structure: PUSH1, JMPIFNOT to skip 1 NOP, then next level
  for (let i = 0; i < depth; i++) {
    script.push(0x11);       // PUSH1
    script.push(0x26, 0x03); // JMPIFNOT +3 (skip NOP)
    script.push(0x21);       // NOP
  }
  script.push(0x40); // RET
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "50-deep nested if");
});

test("control-flow: 100 sequential if/else blocks", () => {
  const script = [];
  for (let i = 0; i < 100; i++) {
    script.push(0x11);       // PUSH1
    script.push(0x26, 0x04); // JMPIFNOT +4 (to next block)
    script.push(0x21);       // NOP (then body)
    script.push(0x22, 0x02); // JMP +2 (skip else)
  }
  script.push(0x40);
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "100 sequential if/else");
});

test("control-flow: loop within loop (nested backward jumps)", () => {
  // Outer: PUSH1; inner: PUSH1 ... JMP back; JMP back
  const script = [
    // addr 0: outer loop start
    0x11,       // PUSH1 (loop condition)
    // addr 1: JMPIFNOT to end
    0x26, 0x09, // JMPIFNOT +9 -> addr 10
    // addr 3: inner loop start
    0x11,       // PUSH1
    // addr 4: JMPIFNOT to outer loop continue
    0x26, 0x04, // JMPIFNOT +4 -> addr 8
    // addr 6: JMP back to inner loop start
    0x22, 0xFD, // JMP -3 -> addr 3
    // addr 8: JMP back to outer loop start
    0x22, 0xF8, // JMP -8 -> addr 0
    // addr 10: RET
    0x40,
  ];
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "nested backward jumps");
});

test("control-flow: forward jumps to every valid position", () => {
  // 20 NOPs then RET. Jump from start to each NOP position.
  for (let target = 2; target <= 21; target++) {
    const script = [0x22, target]; // JMP +target
    for (let j = 0; j < 20; j++) {
      script.push(0x21); // NOP
    }
    script.push(0x40);
    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `forward jump to offset ${target}`,
    );
  }
});

test("control-flow: backward jump to self (infinite loop detection)", () => {
  const script = new Uint8Array([0x22, 0x00, 0x40]); // JMP +0 = jump to self
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "JMP to self");
});

test("control-flow: unreachable code blocks", () => {
  const script = new Uint8Array([
    0x22, 0x06, // JMP +6 (to RET at 6)
    0x11,       // unreachable: PUSH1
    0x12,       // unreachable: PUSH2
    0x9E,       // unreachable: ADD
    0x45,       // unreachable: DROP
    0x40,       // RET (reachable via jump)
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "unreachable code block");
});

test("control-flow: try/catch in a loop", () => {
  const script = new Uint8Array([
    // addr 0: loop start
    0x11,             // PUSH1 (condition)
    0x26, 0x0B,       // JMPIFNOT +11 -> addr 12 (exit)
    // addr 3: TRY catch=+6, finally=0
    0x3B, 0x06, 0x00,
    // addr 6: try body
    0x11,             // PUSH1
    0x3D, 0x04,       // ENDTRY +4 -> addr 12
    // addr 9: catch body
    0x21,             // NOP
    0x3D, 0x01,       // ENDTRY +1 -> addr 12
    // addr 12:
    0x22, 0xF4,       // JMP -12 -> addr 0
    // addr 14:
    0x40,
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "try/catch in loop");
});

test("control-flow: try with both catch and finally", () => {
  const script = new Uint8Array([
    0x3B, 0x05, 0x09, // TRY catch=+5, finally=+9
    // try body
    0x11,             // PUSH1
    0x3D, 0x08,       // ENDTRY +8 -> end
    // catch body (addr 5)
    0x21,             // NOP
    0x3D, 0x05,       // ENDTRY +5 -> end
    // addr 8 - gap
    0x21,             // NOP
    // finally body (addr 9)
    0x3F,             // ENDFINALLY
    // addr 10
    0x40,
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "try/catch/finally");
});

test("control-flow: nested try blocks (3 levels)", () => {
  const script = new Uint8Array([
    // outer try
    0x3B, 0x0B, 0x00, // TRY catch=+11
    // middle try
    0x3B, 0x05, 0x00,
    // inner try
    0x3B, 0x02, 0x00,
    0x3D, 0x06,       // ENDTRY -> end
    // inner catch
    0x3D, 0x04,       // ENDTRY -> end
    // middle catch
    0x3D, 0x02,       // ENDTRY -> end
    // outer catch
    0x3D, 0x01,       // ENDTRY
    0x40,
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "3-level nested try");
});

test("control-flow: all conditional jump types in sequence", () => {
  // JMPEQ, JMPNE, JMPGT, JMPGE, JMPLT, JMPLE - each needs 2 stack values
  const script = [
    0x11, 0x12, 0x28, 0x02, // PUSH1, PUSH2, JMPEQ +2
    0x11, 0x12, 0x2A, 0x02, // PUSH1, PUSH2, JMPNE +2
    0x11, 0x12, 0x2C, 0x02, // PUSH1, PUSH2, JMPGT +2
    0x11, 0x12, 0x2E, 0x02, // PUSH1, PUSH2, JMPGE +2
    0x11, 0x12, 0x30, 0x02, // PUSH1, PUSH2, JMPLT +2
    0x11, 0x12, 0x32, 0x02, // PUSH1, PUSH2, JMPLE +2
    0x40,
  ];
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "all conditional jumps");
});

test("control-flow: CALL to internal methods", () => {
  const script = new Uint8Array([
    // method 0 (entry)
    0x34, 0x03, // CALL +3 -> addr 3
    0x40,       // RET
    // method 1 (addr 3)
    0x11,       // PUSH1
    0x40,       // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "CALL to internal method");
});

test("control-flow: CALL_L with 32-bit offset", () => {
  const script = new Uint8Array([
    0x35, 0x06, 0x00, 0x00, 0x00, // CALL_L +6
    0x40,                           // RET
    // addr 6:
    0x11,                           // PUSH1
    0x40,                           // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "CALL_L 32-bit offset");
});

// ════════════════════════════════════════════════════════════════════════════
// 4. OPERAND BOUNDARY TESTING
// ════════════════════════════════════════════════════════════════════════════

test("operand-boundary: PUSHINT8 at min/max (-128, 0, 127)", () => {
  const values = [0x80, 0x00, 0x7F]; // -128, 0, 127 as signed bytes
  for (const v of values) {
    const script = new Uint8Array([0x00, v, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `PUSHINT8 val=${v}`);
  }
});

test("operand-boundary: PUSHINT16 at boundaries", () => {
  const values = [
    [0x00, 0x80], // -32768
    [0xFF, 0x7F], // 32767
    [0x00, 0x00], // 0
    [0x80, 0x00], // 128
    [0x7F, 0x00], // 127
    [0xFF, 0x00], // 255
    [0x00, 0x01], // 256
    [0xFF, 0xFF], // -1
  ];
  for (const [lo, hi] of values) {
    const script = new Uint8Array([0x01, lo, hi, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `PUSHINT16 [${lo},${hi}]`);
  }
});

test("operand-boundary: PUSHINT32 at boundaries", () => {
  const values = [
    [0x00, 0x00, 0x00, 0x80], // -2147483648 (INT32_MIN)
    [0xFF, 0xFF, 0xFF, 0x7F], // 2147483647 (INT32_MAX)
    [0x00, 0x00, 0x00, 0x00], // 0
    [0xFF, 0xFF, 0xFF, 0xFF], // -1
    [0x00, 0x00, 0x01, 0x00], // 65536
    [0xFF, 0xFF, 0x00, 0x00], // 65535
  ];
  for (const bytes of values) {
    const script = new Uint8Array([0x02, ...bytes, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `PUSHINT32 ${bytes}`);
  }
});

test("operand-boundary: PUSHINT64 at boundaries", () => {
  const values = [
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80], // INT64_MIN
    [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F], // INT64_MAX
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // 0
    [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], // -1
    [0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00], // 2^32
  ];
  for (const bytes of values) {
    const script = new Uint8Array([0x03, ...bytes, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `PUSHINT64`);
  }
});

test("operand-boundary: PUSHINT128 and PUSHINT256", () => {
  // All zeros
  const script128z = new Uint8Array([0x04, ...new Array(16).fill(0), 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(script128z)), "PUSHINT128 zeros");

  // All 0xFF
  const script128f = new Uint8Array([0x04, ...new Array(16).fill(0xFF), 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(script128f)), "PUSHINT128 0xFF");

  // PUSHINT256 all zeros
  const script256z = new Uint8Array([0x05, ...new Array(32).fill(0), 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(script256z)), "PUSHINT256 zeros");

  // PUSHINT256 all 0xFF
  const script256f = new Uint8Array([0x05, ...new Array(32).fill(0xFF), 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(script256f)), "PUSHINT256 0xFF");

  // PUSHINT256 max positive
  const script256max = new Uint8Array([0x05, ...new Array(31).fill(0xFF), 0x7F, 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(script256max)), "PUSHINT256 max positive");
});

test("operand-boundary: U8 operand 0 and 255", () => {
  // LDSFLD with index 0 and 255
  const s0 = new Uint8Array([0x5F, 0x00, 0x40]); // LDSFLD 0
  const s255 = new Uint8Array([0x5F, 0xFF, 0x40]); // LDSFLD 255
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s0)), "U8 operand 0");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s255)), "U8 operand 255");
});

test("operand-boundary: U16 operand 0 and 65535", () => {
  // CALLT with index 0 and 65535
  const s0 = new Uint8Array([0x37, 0x00, 0x00, 0x40]);
  const s65535 = new Uint8Array([0x37, 0xFF, 0xFF, 0x40]);
  const nef0 = buildValidNef(s0);
  const nef65535 = buildValidNef(s65535);
  mustNotCrash(() => decompileHighLevelBytes(nef0), "U16 operand 0");
  mustNotCrash(() => decompileHighLevelBytes(nef65535), "U16 operand 65535");
});

test("operand-boundary: Jump8 signed range (-128 to 127)", () => {
  for (const val of [-128, -1, 0, 1, 2, 127]) {
    const script = new Uint8Array([0x22, val & 0xFF, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `Jump8 val=${val}`);
  }
});

test("operand-boundary: Jump32 extreme values", () => {
  const writeI32 = (v) => [v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF];
  for (const val of [-2147483648, -1, 0, 1, 2147483647]) {
    const script = new Uint8Array([0x23, ...writeI32(val), 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `Jump32 val=${val}`);
  }
});

test("operand-boundary: PUSHDATA1 empty and max-size data", () => {
  // Empty data
  const empty = new Uint8Array([0x0C, 0x00, 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(empty)), "PUSHDATA1 empty");

  // 255 bytes of data (max for Data1)
  const maxData = [0x0C, 0xFF, ...new Array(255).fill(0xAA), 0x40];
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(new Uint8Array(maxData))), "PUSHDATA1 255 bytes");
});

test("operand-boundary: PUSHDATA2 larger payloads", () => {
  // 256 bytes (just over Data1 range)
  const data256 = [0x0D, 0x00, 0x01, ...new Array(256).fill(0xBB), 0x40];
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(new Uint8Array(data256))), "PUSHDATA2 256 bytes");

  // 1000 bytes
  const data1k = [0x0D, 0xE8, 0x03, ...new Array(1000).fill(0xCC), 0x40];
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(new Uint8Array(data1k))), "PUSHDATA2 1000 bytes");
});

test("operand-boundary: ISTYPE and CONVERT with all type IDs", () => {
  const typeIds = [0x00, 0x01, 0x11, 0x20, 0x21, 0x22, 0x28, 0x30, 0x40, 0x41, 0x42, 0x48, 0x61, 0xFF];
  for (const typeId of typeIds) {
    const scriptIsType = new Uint8Array([0x11, 0xD9, typeId, 0x40]);
    mustNotCrash(
      () => decompileHighLevelBytes(buildValidNef(scriptIsType)),
      `ISTYPE type=${typeId}`,
    );
    const scriptConvert = new Uint8Array([0x11, 0xDB, typeId, 0x40]);
    mustNotCrash(
      () => decompileHighLevelBytes(buildValidNef(scriptConvert)),
      `CONVERT type=${typeId}`,
    );
  }
});

// ════════════════════════════════════════════════════════════════════════════
// 5. METHOD TOKEN COMBINATIONS
// ════════════════════════════════════════════════════════════════════════════

test("method-tokens: 0 tokens", () => {
  const nef = buildNef({ tokens: [], script: new Uint8Array([0x11, 0x40]) });
  const result = parseNef(nef);
  assert.equal(result.methodTokens.length, 0);
});

test("method-tokens: 1 token with CALLT 0", () => {
  const token = {
    hash: new Uint8Array(20).fill(0x01),
    method: "transfer",
    params: 3,
    hasReturn: true,
    callFlags: 0x0f,
  };
  const script = new Uint8Array([0x37, 0x00, 0x00, 0x40]); // CALLT 0
  const nef = buildNef({ tokens: [token], script });
  mustNotCrash(() => decompileHighLevelBytes(nef), "CALLT with 1 token");
});

test("method-tokens: 128 tokens (half of max)", () => {
  const tokens = [];
  for (let i = 0; i < 128; i++) {
    tokens.push({
      hash: new Uint8Array(20).fill(i),
      method: `m${i}`,
      params: i % 8,
      hasReturn: i % 2 === 0,
      callFlags: 0x0f,
    });
  }
  const script = new Uint8Array([0x37, 0x7F, 0x00, 0x40]); // CALLT 127
  const nef = buildNef({ tokens, script });
  mustNotCrash(() => decompileHighLevelBytes(nef), "128 tokens");
});

test("method-tokens: all call flag combinations", () => {
  // Valid call flags are 0x00-0x0F (4-bit mask)
  for (let flags = 0; flags <= 0x0F; flags++) {
    const token = {
      hash: new Uint8Array(20).fill(flags),
      method: `m_flags_${flags}`,
      params: 1,
      hasReturn: true,
      callFlags: flags,
    };
    const script = new Uint8Array([0x37, 0x00, 0x00, 0x40]);
    const nef = buildNef({ tokens: [token], script });
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `callFlags=0x${flags.toString(16)}`,
    );
  }
});

test("method-tokens: long method name (500 chars)", () => {
  const longName = "a".repeat(500);
  const token = {
    hash: new Uint8Array(20).fill(0xAB),
    method: longName,
    params: 0,
    hasReturn: false,
    callFlags: 0x0f,
  };
  const script = new Uint8Array([0x37, 0x00, 0x00, 0x40]);
  const nef = buildNef({ tokens: [token], script });
  const parsed = parseNef(nef);
  assert.equal(parsed.methodTokens[0].method, longName);
});

test("method-tokens: token with 0 params and no return", () => {
  const token = {
    hash: new Uint8Array(20).fill(0x00),
    method: "noArgs",
    params: 0,
    hasReturn: false,
    callFlags: 0x0f,
  };
  const script = new Uint8Array([0x37, 0x00, 0x00, 0x40]);
  const nef = buildNef({ tokens: [token], script });
  mustNotCrash(() => decompileHighLevelBytes(nef), "token with 0 params, no return");
});

test("method-tokens: token with max params (65535)", () => {
  const token = {
    hash: new Uint8Array(20).fill(0xCC),
    method: "manyArgs",
    params: 65535,
    hasReturn: true,
    callFlags: 0x0f,
  };
  const script = new Uint8Array([0x37, 0x00, 0x00, 0x40]);
  const nef = buildNef({ tokens: [token], script });
  mustNotCrash(() => decompileHighLevelBytes(nef), "token with 65535 params");
});

test("method-tokens: multiple CALLT references to different tokens", () => {
  const tokens = [];
  for (let i = 0; i < 5; i++) {
    tokens.push({
      hash: new Uint8Array(20).fill(i + 1),
      method: `func${i}`,
      params: i,
      hasReturn: i % 2 === 0,
      callFlags: 0x0f,
    });
  }
  const script = new Uint8Array([
    0x37, 0x00, 0x00, // CALLT 0
    0x37, 0x01, 0x00, // CALLT 1
    0x37, 0x02, 0x00, // CALLT 2
    0x37, 0x03, 0x00, // CALLT 3
    0x37, 0x04, 0x00, // CALLT 4
    0x40,
  ]);
  const nef = buildNef({ tokens, script });
  mustNotCrash(() => decompileHighLevelBytes(nef), "multiple CALLT calls");
});

test("method-tokens: CALLT index out of bounds", () => {
  const token = {
    hash: new Uint8Array(20).fill(0x01),
    method: "only",
    params: 0,
    hasReturn: false,
    callFlags: 0x0f,
  };
  const script = new Uint8Array([0x37, 0x05, 0x00, 0x40]); // CALLT 5, but only 1 token
  const nef = buildNef({ tokens: [token], script });
  mustNotCrash(() => decompileHighLevelBytes(nef), "CALLT out of bounds");
});

// ════════════════════════════════════════════════════════════════════════════
// 6. MANIFEST + NEF INTERACTION
// ════════════════════════════════════════════════════════════════════════════

test("manifest: valid manifest with multiple methods", () => {
  const manifest = JSON.stringify({
    name: "TestContract",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0, safe: false },
        { name: "getValue", parameters: [{ name: "key", type: "String" }], returntype: "Integer", offset: 3, safe: true },
      ],
      events: [{ name: "Transfer", parameters: [{ name: "from", type: "Hash160" }] }],
    },
    supportedstandards: ["NEP-17"],
    permissions: [{ contract: "*", methods: "*" }],
    trusts: [],
    extra: { author: "test" },
  });
  const script = new Uint8Array([
    0x57, 0x00, 0x00, // INITSLOT 0, 0
    0x40,             // RET (method 0 at offset 0)
    0x57, 0x00, 0x01, // INITSLOT 0, 1 (method 1 at offset 3, taking 1 arg -- corrected offset)
    0x78,             // LDARG0
    0x40,             // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(
    () => decompileHighLevelBytesWithManifest(nef, manifest),
    "valid manifest with methods",
  );
});

test("manifest: empty manifest object", () => {
  const script = new Uint8Array([0x11, 0x40]);
  const nef = buildValidNef(script);
  mustNotCrash(
    () => decompileHighLevelBytesWithManifest(nef, "{}"),
    "empty manifest",
  );
});

test("manifest: manifest with null fields", () => {
  const manifests = [
    '{"name":null}',
    '{"abi":null}',
    '{"abi":{"methods":null}}',
    '{"permissions":null}',
    '{"trusts":null}',
    '{"extra":null}',
    '{"features":null}',
    '{"supportedstandards":null}',
    '{"groups":null}',
  ];
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  for (const m of manifests) {
    mustNotCrash(
      () => decompileHighLevelBytesWithManifest(nef, m),
      `manifest: ${m}`,
    );
  }
});

test("manifest: methods pointing to invalid offsets", () => {
  const manifest = JSON.stringify({
    name: "Bad",
    abi: {
      methods: [
        { name: "missing", offset: 9999, parameters: [], returntype: "Void" },
        { name: "negative", offset: -5, parameters: [], returntype: "Void" },
      ],
    },
  });
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  mustNotCrash(
    () => decompileHighLevelBytesWithManifest(nef, manifest),
    "invalid offsets in manifest",
  );
});

test("manifest: methods with all return types", () => {
  const returnTypes = [
    "Void", "Boolean", "Integer", "String", "Hash160", "Hash256",
    "ByteArray", "Signature", "Array", "Map", "InteropInterface", "Any",
  ];
  for (const rt of returnTypes) {
    const manifest = JSON.stringify({
      name: "TypeTest",
      abi: { methods: [{ name: "m", offset: 0, parameters: [], returntype: rt }] },
    });
    const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
    mustNotCrash(
      () => decompileHighLevelBytesWithManifest(nef, manifest),
      `manifest returntype=${rt}`,
    );
  }
});

test("manifest: methods with all parameter types", () => {
  const paramTypes = [
    "Boolean", "Integer", "String", "Hash160", "Hash256",
    "ByteArray", "Signature", "Array", "Map", "InteropInterface", "Any",
  ];
  for (const pt of paramTypes) {
    const manifest = JSON.stringify({
      name: "ParamTest",
      abi: {
        methods: [{
          name: "m",
          offset: 0,
          parameters: [{ name: "p", type: pt }],
          returntype: "Void",
        }],
      },
    });
    const script = new Uint8Array([0x57, 0x00, 0x01, 0x78, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytesWithManifest(nef, manifest),
      `manifest param type=${pt}`,
    );
  }
});

test("manifest: very long contract name", () => {
  const manifest = JSON.stringify({ name: "A".repeat(1000) });
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  mustNotCrash(
    () => decompileHighLevelBytesWithManifest(nef, manifest),
    "very long contract name",
  );
});

test("manifest: method names with special characters", () => {
  const names = [
    "camelCase", "snake_case", "PascalCase",
    "with spaces", "with-dashes", "with.dots",
    "123numeric", "_underscore", "",
    "unicode\u00e9\u00e8\u00ea", "\u{1F600}emoji",
  ];
  for (const name of names) {
    const manifest = JSON.stringify({
      name: "SpecialNames",
      abi: { methods: [{ name, offset: 0, parameters: [], returntype: "Void" }] },
    });
    const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
    mustNotCrash(
      () => decompileHighLevelBytesWithManifest(nef, manifest),
      `method name: ${JSON.stringify(name)}`,
    );
  }
});

test("manifest: empty methods array", () => {
  const manifest = JSON.stringify({
    name: "Empty",
    abi: { methods: [] },
  });
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  mustNotCrash(
    () => decompileHighLevelBytesWithManifest(nef, manifest),
    "empty methods array",
  );
});

test("manifest: analyzeBytes with and without manifest", () => {
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  mustNotCrash(() => analyzeBytes(nef), "analyzeBytes without manifest");
  mustNotCrash(
    () => analyzeBytes(nef, '{"name":"Test","abi":{"methods":[]}}'),
    "analyzeBytes with manifest",
  );
});

// ════════════════════════════════════════════════════════════════════════════
// 7. STRUCTURED GRAMMAR FUZZING
// ════════════════════════════════════════════════════════════════════════════

/**
 * Generate a random structurally valid script:
 * - Uses only known opcodes
 * - Provides correct operand sizes
 * - Ends with RET
 */
function generateRandomValidScript(rng, targetLength) {
  const script = [];
  const noOpOpcodes = ALL_OPCODES.filter(
    (o) => o.encoding === "None" && o.byte !== 0x40 /* skip RET */,
  );
  const allSafe = ALL_OPCODES.filter((o) => o.byte !== 0x40);

  while (script.length < targetLength) {
    // 80% chance of no-operand opcode, 20% chance of operand opcode
    const useOperand = rng.next() < 0.2;
    const pool = useOperand ? allSafe : noOpOpcodes;
    const op = pool[rng.nextInt(pool.length)];
    const operand = operandForEncoding(op.encoding);

    // Don't exceed target by too much
    if (script.length + 1 + operand.length > targetLength + 10) {
      break;
    }

    script.push(op.byte, ...operand);
  }
  script.push(0x40); // RET
  return new Uint8Array(script);
}

test("grammar-fuzz: 500 random valid scripts of length ~10", () => {
  const rng = createRng(7777);
  let completed = 0;
  for (let i = 0; i < 500; i++) {
    const script = generateRandomValidScript(rng, 10);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `grammar-fuzz-10 iter=${i}`,
    );
    completed++;
  }
  assert.equal(completed, 500);
});

test("grammar-fuzz: 200 random valid scripts of length ~100", () => {
  const rng = createRng(8888);
  let completed = 0;
  for (let i = 0; i < 200; i++) {
    const script = generateRandomValidScript(rng, 100);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `grammar-fuzz-100 iter=${i}`,
    );
    completed++;
  }
  assert.equal(completed, 200);
});

test("grammar-fuzz: 50 random valid scripts of length ~1000", () => {
  const rng = createRng(9999);
  let completed = 0;
  for (let i = 0; i < 50; i++) {
    const script = generateRandomValidScript(rng, 1000);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `grammar-fuzz-1000 iter=${i}`,
    );
    completed++;
  }
  assert.equal(completed, 50);
});

test("grammar-fuzz: 200 random scripts through disassemble only", () => {
  const rng = createRng(1111);
  for (let i = 0; i < 200; i++) {
    const len = 5 + rng.nextInt(200);
    const script = generateRandomValidScript(rng, len);
    mustNotCrash(
      () => disassembleScript(script),
      `disasm-fuzz iter=${i} len=${len}`,
    );
  }
});

test("grammar-fuzz: 200 random scripts through analyzeBytes", () => {
  const rng = createRng(2222);
  for (let i = 0; i < 200; i++) {
    const len = 5 + rng.nextInt(100);
    const script = generateRandomValidScript(rng, len);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => analyzeBytes(nef),
      `analyze-fuzz iter=${i}`,
    );
  }
});

test("grammar-fuzz: random scripts with INITSLOT prefix", () => {
  const rng = createRng(3333);
  for (let i = 0; i < 100; i++) {
    const nLocals = rng.nextInt(8);
    const nArgs = rng.nextInt(8);
    const body = generateRandomValidScript(rng, 20 + rng.nextInt(50));
    const script = new Uint8Array([0x57, nLocals, nArgs, ...body]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `initslot-fuzz iter=${i} locals=${nLocals} args=${nArgs}`,
    );
  }
});

test("grammar-fuzz: random scripts with manifest", () => {
  const rng = createRng(4444);
  for (let i = 0; i < 100; i++) {
    const nArgs = rng.nextInt(4);
    const params = [];
    for (let j = 0; j < nArgs; j++) {
      params.push({ name: `p${j}`, type: "Any" });
    }
    const manifest = JSON.stringify({
      name: `Fuzz${i}`,
      abi: {
        methods: [{
          name: "main",
          offset: 0,
          parameters: params,
          returntype: "Any",
        }],
      },
    });
    const script = generateRandomValidScript(rng, 15 + rng.nextInt(30));
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytesWithManifest(nef, manifest),
      `manifest-fuzz iter=${i}`,
    );
  }
});

test("grammar-fuzz: pure random byte scripts (not structurally valid)", () => {
  const rng = createRng(5555);
  for (let i = 0; i < 200; i++) {
    const len = 5 + rng.nextInt(100);
    const script = randomBytes(rng, len);
    script[script.length - 1] = 0x40; // ensure RET at end
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `random-bytes iter=${i}`,
    );
  }
});

// ════════════════════════════════════════════════════════════════════════════
// 8. REGRESSION PATTERNS
// ════════════════════════════════════════════════════════════════════════════

test("regression: all-zero script", () => {
  // 0x00 is PUSHINT8 which needs 1 byte operand, so [0x00, 0x00, 0x40]
  const scripts = [
    new Uint8Array([0x00, 0x00, 0x40]),     // PUSHINT8 0, RET
    new Uint8Array([0x10, 0x40]),             // PUSH0, RET
  ];
  for (const s of scripts) {
    const nef = buildValidNef(s);
    mustNotCrash(() => decompileHighLevelBytes(nef), "all-zero-ish script");
  }
});

test("regression: all-0xFF script body", () => {
  // 0xFF is not a known opcode, so the disassembler should handle it
  const script = new Uint8Array(50).fill(0xFF);
  script[script.length - 1] = 0x40;
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "all-0xFF script");
});

test("regression: script ending mid-operand", () => {
  // PUSHINT32 needs 4 bytes but only give 2 + no RET
  const truncated = [
    new Uint8Array([0x02, 0x01, 0x02]),
    new Uint8Array([0x03, 0x01]),
    new Uint8Array([0x0C, 0x05, 0x01]),
    new Uint8Array([0x23, 0x01, 0x00]),
    new Uint8Array([0x41, 0x01, 0x02]),
  ];
  for (const script of truncated) {
    const nef = buildValidNef(script);
    mustNotCrash(
      () => {
        try { decompileBytes(nef); } catch { /* expected */ }
        // Also test disassemble directly
        disassembleScript(script);
      },
      "mid-operand truncation",
    );
  }
});

test("regression: jump to self variations", () => {
  const scripts = [
    new Uint8Array([0x22, 0x00, 0x40]),                         // JMP +0
    new Uint8Array([0x23, 0x00, 0x00, 0x00, 0x00, 0x40]),       // JMP_L +0
    new Uint8Array([0x11, 0x24, 0x00, 0x40]),                   // PUSH1, JMPIF +0
    new Uint8Array([0x11, 0x26, 0x00, 0x40]),                   // PUSH1, JMPIFNOT +0
  ];
  for (const s of scripts) {
    const nef = buildValidNef(s);
    mustNotCrash(() => decompileHighLevelBytes(nef), "jump to self variant");
  }
});

test("regression: ABORT sequences", () => {
  const scripts = [
    new Uint8Array([0x38, 0x40]),                   // ABORT, RET
    new Uint8Array([0x38]),                           // just ABORT, no RET
    new Uint8Array([0x11, 0x39, 0x40]),              // PUSH1, ASSERT, RET
    new Uint8Array([0x10, 0x39, 0x40]),              // PUSH0, ASSERT, RET (will fail assert)
    new Uint8Array([0x11, 0xE1, 0x40]),              // PUSH1, ASSERTMSG (msg), RET
    new Uint8Array([0x0C, 0x03, 0x65, 0x72, 0x72, 0xE0, 0x40]), // PUSHDATA1 "err", ABORTMSG, RET
  ];
  for (const s of scripts) {
    const nef = buildValidNef(s);
    mustNotCrash(() => decompileHighLevelBytes(nef), "ABORT sequence");
  }
});

test("regression: THROW without TRY", () => {
  const script = new Uint8Array([0x11, 0x3A, 0x40]); // PUSH1, THROW, RET
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "THROW without TRY");
});

test("regression: ENDFINALLY without TRY", () => {
  const script = new Uint8Array([0x3F, 0x40]); // ENDFINALLY, RET
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "ENDFINALLY without TRY");
});

test("regression: ENDTRY without TRY", () => {
  const script = new Uint8Array([0x3D, 0x01, 0x40]); // ENDTRY +1, RET
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "ENDTRY without TRY");
});

test("regression: CALLA without function reference on stack", () => {
  const script = new Uint8Array([0x11, 0x36, 0x40]); // PUSH1, CALLA, RET
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "CALLA without function ref");
});

test("regression: PUSHA with various target offsets", () => {
  const writeU32 = (v) => [v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF];
  const offsets = [0, 1, 5, 100, 65535, 0xFFFFFFFF];
  for (const off of offsets) {
    const script = new Uint8Array([0x0A, ...writeU32(off), 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef), `PUSHA offset=${off}`);
  }
});

test("regression: SYSCALL with known hashes", () => {
  const writeU32 = (v) => [v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, (v >> 24) & 0xFF];
  const hashes = [
    0x9BF667CE, // System.Runtime.GetTime
    0x268F126A, // System.Runtime.Log
    0xF827EC8C, // System.Runtime.CheckWitness
    0x00000000, // unknown
    0xFFFFFFFF, // unknown
    0xDEADBEEF, // unknown
  ];
  for (const hash of hashes) {
    const script = new Uint8Array([0x41, ...writeU32(hash), 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `SYSCALL hash=0x${hash.toString(16)}`,
    );
  }
});

test("regression: NEWARRAY_T with various type bytes", () => {
  for (let t = 0; t <= 0xFF; t += 17) {
    const script = new Uint8Array([0x11, 0xC4, t, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `NEWARRAY_T type=${t}`,
    );
  }
});

test("regression: deeply nested PACK/UNPACK", () => {
  const script = [];
  // Build nested arrays: push items, pack, push more, pack again
  for (let i = 0; i < 20; i++) {
    script.push(0x11); // PUSH1
    script.push(0x11); // PUSH1 (count)
    script.push(0xC0); // PACK
  }
  script.push(0x40);
  const nef = buildValidNef(new Uint8Array(script));
  mustNotCrash(() => decompileHighLevelBytes(nef), "nested PACK/UNPACK");
});

test("regression: INITSSLOT and static field operations", () => {
  const script = new Uint8Array([
    0x56, 0x03, // INITSSLOT 3
    0x11,       // PUSH1
    0x60,       // STSFLD0
    0x12,       // PUSH2
    0x61,       // STSFLD1
    0x58,       // LDSFLD0
    0x59,       // LDSFLD1
    0x9E,       // ADD
    0x62,       // STSFLD2
    0x5A,       // LDSFLD2
    0x40,       // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "static field operations");
});

test("regression: INITSLOT then local and arg operations", () => {
  const script = new Uint8Array([
    0x57, 0x03, 0x02, // INITSLOT 3 locals, 2 args
    0x78,             // LDARG0
    0x70,             // STLOC0
    0x79,             // LDARG1
    0x71,             // STLOC1
    0x68,             // LDLOC0
    0x69,             // LDLOC1
    0x9E,             // ADD
    0x72,             // STLOC2
    0x6A,             // LDLOC2
    0x40,             // RET
  ]);
  const nef = buildValidNef(script);
  const result = decompileHighLevelBytes(nef);
  assert.ok(result.highLevel, "should produce high-level output");
});

test("regression: buffer operations sequence", () => {
  const script = new Uint8Array([
    0x11,       // PUSH1 (size)
    0x88,       // NEWBUFFER
    0x0C, 0x01, 0x41, // PUSHDATA1 "A"
    0x8B,       // CAT
    0xCA,       // SIZE
    0x40,       // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "buffer operations");
});

test("regression: map operations sequence", () => {
  const script = new Uint8Array([
    0xC8,       // NEWMAP
    0x4A,       // DUP
    0x11,       // PUSH1 (key)
    0x12,       // PUSH2 (value)
    0xD0,       // SETITEM
    0x4A,       // DUP
    0xCC,       // KEYS
    0x50,       // SWAP
    0xCD,       // VALUES
    0x40,       // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "map operations");
});

test("regression: NOP-only script (large)", () => {
  const script = new Uint8Array(1001);
  script.fill(0x21); // NOP
  script[1000] = 0x40; // RET
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "1000 NOPs");
});

test("regression: mixed known and unknown opcodes", () => {
  // Bytes 0x06, 0x07, 0x42, 0x44, 0x47, 0x4C, 0x4F are not defined opcodes
  const unknowns = [0x06, 0x07, 0x42, 0x44, 0x47, 0x4C, 0x4F, 0x94, 0x95, 0x96];
  for (const unk of unknowns) {
    const script = new Uint8Array([0x11, unk, 0x40]);
    const nef = buildValidNef(script);
    mustNotCrash(
      () => decompileHighLevelBytes(nef),
      `unknown opcode 0x${unk.toString(16)}`,
    );
  }
});

test("regression: WITHIN, MIN, MAX with stacked args", () => {
  // WITHIN needs 3 args
  const scriptWithin = new Uint8Array([0x11, 0x10, 0x12, 0xBB, 0x40]);
  mustNotCrash(
    () => decompileHighLevelBytes(buildValidNef(scriptWithin)),
    "WITHIN",
  );
  // MIN and MAX need 2
  const scriptMin = new Uint8Array([0x11, 0x12, 0xB9, 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(scriptMin)), "MIN");
  const scriptMax = new Uint8Array([0x11, 0x12, 0xBA, 0x40]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(scriptMax)), "MAX");
});

test("regression: MODMUL and MODPOW (3-arg opcodes)", () => {
  const scriptModmul = new Uint8Array([0x11, 0x12, 0x13, 0xA5, 0x40]);
  mustNotCrash(
    () => decompileHighLevelBytes(buildValidNef(scriptModmul)),
    "MODMUL",
  );
  const scriptModpow = new Uint8Array([0x11, 0x12, 0x13, 0xA6, 0x40]);
  mustNotCrash(
    () => decompileHighLevelBytes(buildValidNef(scriptModpow)),
    "MODPOW",
  );
});

test("regression: SUBSTR, LEFT, RIGHT string ops", () => {
  const data = [0x0C, 0x05, 0x48, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
  const s1 = new Uint8Array([...data, 0x11, 0x12, 0x8C, 0x40]); // SUBSTR
  const s2 = new Uint8Array([...data, 0x12, 0x8D, 0x40]);        // LEFT
  const s3 = new Uint8Array([...data, 0x12, 0x8E, 0x40]);        // RIGHT
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s1)), "SUBSTR");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s2)), "LEFT");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s3)), "RIGHT");
});

test("regression: MEMCPY (5-arg opcode)", () => {
  const script = new Uint8Array([
    0x11, // dst buffer placeholder (PUSH1)
    0x12, // dst index
    0x13, // src buffer
    0x14, // src index
    0x15, // count
    0x89, // MEMCPY
    0x40,
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "MEMCPY");
});

test("regression: CONVERT to every standard type", () => {
  const stackTypes = [0x21, 0x22, 0x28, 0x30, 0x40, 0x41, 0x42, 0x48, 0x61];
  for (const t of stackTypes) {
    const script = new Uint8Array([0x11, 0xDB, t, 0x40]);
    mustNotCrash(
      () => decompileHighLevelBytes(buildValidNef(script)),
      `CONVERT to type 0x${t.toString(16)}`,
    );
  }
});

test("regression: ISNULL on various values", () => {
  const scripts = [
    new Uint8Array([0x0B, 0xD8, 0x40]),       // PUSHNULL, ISNULL, RET
    new Uint8Array([0x11, 0xD8, 0x40]),        // PUSH1, ISNULL, RET
    new Uint8Array([0x08, 0xD8, 0x40]),        // PUSHT, ISNULL, RET
    new Uint8Array([0x09, 0xD8, 0x40]),        // PUSHF, ISNULL, RET
  ];
  for (const s of scripts) {
    mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s)), "ISNULL");
  }
});

test("regression: double RET", () => {
  const script = new Uint8Array([0x40, 0x40]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "double RET");
});

test("regression: NZ and boolean logic chain", () => {
  const script = new Uint8Array([
    0x11,  // PUSH1
    0xB1,  // NZ
    0x12,  // PUSH2
    0xB1,  // NZ
    0xAB,  // BOOLAND
    0x13,  // PUSH3
    0xB1,  // NZ
    0xAC,  // BOOLOR
    0xAA,  // NOT
    0x40,  // RET
  ]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "NZ + boolean logic");
});

test("regression: DEPTH and CLEAR on various stacks", () => {
  const scripts = [
    new Uint8Array([0x43, 0x40]),                   // DEPTH on empty
    new Uint8Array([0x11, 0x43, 0x40]),             // DEPTH with 1 item
    new Uint8Array([0x11, 0x12, 0x13, 0x43, 0x40]), // DEPTH with 3 items
    new Uint8Array([0x49, 0x40]),                    // CLEAR on empty
    new Uint8Array([0x11, 0x12, 0x49, 0x40]),       // CLEAR with 2 items
  ];
  for (const s of scripts) {
    mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s)), "DEPTH/CLEAR");
  }
});

test("regression: REVERSE3, REVERSE4, REVERSEN", () => {
  const s3 = new Uint8Array([0x11, 0x12, 0x13, 0x53, 0x40]);
  const s4 = new Uint8Array([0x11, 0x12, 0x13, 0x14, 0x54, 0x40]);
  const sn = new Uint8Array([0x11, 0x12, 0x13, 0x13, 0x55, 0x40]); // REVERSEN with n=3
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s3)), "REVERSE3");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s4)), "REVERSE4");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(sn)), "REVERSEN");
});

test("regression: TUCK and OVER patterns", () => {
  const s1 = new Uint8Array([0x11, 0x12, 0x4E, 0x40]); // TUCK
  const s2 = new Uint8Array([0x11, 0x12, 0x4B, 0x40]); // OVER
  const s3 = new Uint8Array([0x11, 0x12, 0x46, 0x40]); // NIP
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s1)), "TUCK");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s2)), "OVER");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s3)), "NIP");
});

test("regression: XDROP with stacked values", () => {
  const s = new Uint8Array([0x11, 0x12, 0x13, 0x11, 0x48, 0x40]); // 3 values, PUSH1, XDROP
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s)), "XDROP");
});

test("regression: PACKMAP and PACKSTRUCT", () => {
  const sPm = new Uint8Array([
    0x11, 0x12, // key, value
    0x11,       // count = 1
    0xBE,       // PACKMAP
    0x40,
  ]);
  const sPs = new Uint8Array([
    0x11, 0x12, // items
    0x12,       // count = 2
    0xBF,       // PACKSTRUCT
    0x40,
  ]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(sPm)), "PACKMAP");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(sPs)), "PACKSTRUCT");
});

test("regression: POPITEM on array", () => {
  const s = new Uint8Array([
    0x11,  // PUSH1
    0x11,  // PUSH1 (count)
    0xC0,  // PACK
    0xD4,  // POPITEM
    0x40,  // RET
  ]);
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s)), "POPITEM");
});

test("regression: large PUSHDATA4", () => {
  // PUSHDATA4 with 5000 bytes
  const len = 5000;
  const header = [0x0E, len & 0xFF, (len >> 8) & 0xFF, (len >> 16) & 0xFF, (len >> 24) & 0xFF];
  const data = new Array(len).fill(0xDD);
  const script = new Uint8Array([...header, ...data, 0x40]);
  const nef = buildValidNef(script);
  mustNotCrash(() => decompileHighLevelBytes(nef), "PUSHDATA4 5000 bytes");
});

test("regression: EQUAL and NOTEQUAL with mixed types", () => {
  const s1 = new Uint8Array([0x11, 0x0B, 0x97, 0x40]); // 1, null, EQUAL
  const s2 = new Uint8Array([0x08, 0x09, 0x97, 0x40]); // true, false, EQUAL
  const s3 = new Uint8Array([0x11, 0x0B, 0x98, 0x40]); // 1, null, NOTEQUAL
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s1)), "EQUAL mixed 1");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s2)), "EQUAL mixed 2");
  mustNotCrash(() => decompileHighLevelBytes(buildValidNef(s3)), "NOTEQUAL mixed");
});

test("regression: rapid method boundary changes (multi-method script)", () => {
  // Script with many small methods (indicated by RET boundaries)
  const script = [];
  for (let i = 0; i < 50; i++) {
    script.push(0x11); // PUSH1
    script.push(0x40); // RET
  }
  const nef = buildValidNef(new Uint8Array(script));
  const result = decompileHighLevelBytes(nef);
  assert.ok(result.highLevel, "multi-method script should produce output");
});

console.log("Systematic fuzz tests loaded");
