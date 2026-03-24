/**
 * Boundary and Edge Case Tests for Neo Decompiler JS
 * Tests limits, edge cases, and corner cases
 */

import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";

import {
  parseNef,
  disassembleScript,
  decompileBytes,
  decompileHighLevelBytes,
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
  } else if (value <= 0xffffffff) {
    buffer.push(
      0xfe,
      value & 0xff,
      (value >> 8) & 0xff,
      (value >> 16) & 0xff,
      (value >> 24) & 0xff,
    );
  } else {
    buffer.push(0xff);
    const big = BigInt.asUintN(64, BigInt(value));
    for (let i = 0; i < 8; i++) {
      buffer.push(Number((big >> BigInt(i * 8)) & BigInt(0xff)));
    }
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

// ─── Boundary Tests ─────────────────────────────────────────────────────────

test("boundary: single byte script", () => {
  // Minimum valid script (just RET)
  const nef = buildNef({ script: new Uint8Array([0x40]) });
  const result = decompileHighLevelBytes(nef);
  assert.ok(result.highLevel);
});

test("boundary: max short varint (0xfc = 252)", () => {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0xfc); // source length = 252 (max 1-byte varint)
  data.push(...new Uint8Array(252)); // source bytes
  data.push(0); // reserved
  data.push(0); // 0 tokens
  data.push(0, 0); // reserved word
  data.push(1, 0x40); // script len=1, RET
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  assert.doesNotThrow(() => parseNef(new Uint8Array(data)));
});

test("boundary: 2-byte varint boundary (253 = 0xfd)", () => {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0xfd, 0xfd, 0x00); // source length = 253 (needs 2-byte encoding)
  data.push(...new Uint8Array(253));
  data.push(0); // reserved
  data.push(0); // 0 tokens
  data.push(0, 0); // reserved word
  data.push(1, 0x40); // script
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  assert.doesNotThrow(() => parseNef(new Uint8Array(data)));
});

test("boundary: max 2-byte varint exceeds source limit and is rejected", () => {
  // Source string is limited to 256 bytes, so 65535 should be rejected
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0xfd, 0xff, 0xff); // source length = 65535 (exceeds limit)
  data.push(...new Uint8Array(65535));
  data.push(0); // reserved
  data.push(0); // 0 tokens
  data.push(0, 0); // reserved word
  data.push(1, 0x40); // script
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  assert.throws(() => parseNef(new Uint8Array(data)), /exceeds/);
});

test("boundary: all PUSH0-PUSH16 opcodes", () => {
  for (let i = 0; i <= 16; i++) {
    const opcode = 0x10 + i; // PUSH0 = 0x10, PUSH16 = 0x20
    const nef = buildNef({ script: new Uint8Array([opcode, 0x40]) });
    const result = decompileBytes(nef);
    assert.ok(result.instructions.some(inst => inst.offset === 0));
  }
});

test("boundary: all jump variants", () => {
  const jumps = [
    { opcode: 0x22, name: "JMP" },
    { opcode: 0x23, name: "JMP_L" },
    { opcode: 0x24, name: "JMPIF" },
    { opcode: 0x25, name: "JMPIF_L" },
    { opcode: 0x26, name: "JMPIFNOT" },
    { opcode: 0x27, name: "JMPIFNOT_L" },
  ];
  
  for (const { opcode, name } of jumps) {
    const script = name.endsWith("_L")
      ? new Uint8Array([opcode, 0x05, 0x00, 0x00, 0x00, 0x40]) // 32-bit offset
      : new Uint8Array([opcode, 0x05, 0x40]); // 8-bit offset
    const nef = buildNef({ script });
    assert.doesNotThrow(() => decompileHighLevelBytes(nef), `${name} should not crash`);
  }
});

test("boundary: all slot opcodes (0-6)", () => {
  const opcodes = [
    // Locals
    [0x68, 0x69, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e], // LDLOC0-6
    [0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76], // STLOC0-6
    // Args
    [0x78, 0x79, 0x7a, 0x7b, 0x7c, 0x7d, 0x7e], // LDARG0-6
    [0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86], // STARG0-6
    // Statics
    [0x58, 0x59, 0x5a, 0x5b, 0x5c, 0x5d, 0x5e], // LDSFLD0-6
    [0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66], // STSFLD0-6
  ];
  
  for (const group of opcodes) {
    for (const opcode of group) {
      const nef = buildNef({ script: new Uint8Array([opcode, 0x40]) });
      assert.doesNotThrow(() => decompileHighLevelBytes(nef), `opcode 0x${opcode.toString(16)} should not crash`);
    }
  }
});

test("boundary: indexed slot opcodes (LDLOC, STLOC, etc.)", () => {
  const script = new Uint8Array([
    0x57, 0xff, 0xff, // INITSLOT 255 locals, 255 args (max)
    0x6f, 0xff, // LDLOC 255
    0x77, 0xff, // STLOC 255
    0x7f, 0xff, // LDARG 255
    0x87, 0xff, // STARG 255
    0x5f, 0xff, // LDSFLD 255
    0x67, 0xff, // STSFLD 255
    0x40, // RET
  ]);
  const nef = buildNef({ script });
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("boundary: arithmetic opcodes", () => {
  const arithOps = [
    0x90, // INVERT
    0x91, // AND
    0x92, // OR
    0x93, // XOR
    0x94, // EQUAL
    0x95, // NOTEQUAL
    0x96, // SIGN
    0x99, // ABS
    0x9a, // NEGATE
    0x9b, // INC
    0x9c, // DEC
    0x9d, // ADD
    0x9e, // SUB
    0x9f, // MUL
    0xa0, // DIV
    0xa1, // MOD
    0xa4, // SQRT
    0xa5, // MODMUL
    0xa8, // SHL
    0xa9, // SHR
    0xaa, // NOT
    0xb5, // LT
    0xb6, // LE
    0xb7, // GT
    0xb8, // GE
  ];
  
  for (const opcode of arithOps) {
    const nef = buildNef({ script: new Uint8Array([0x11, 0x12, opcode, 0x40]) });
    assert.doesNotThrow(() => decompileHighLevelBytes(nef), `opcode 0x${opcode.toString(16)} should not crash`);
  }
});

test("boundary: max script length (within limits)", () => {
  // Test with a reasonably large script (100KB)
  const largeScript = new Uint8Array(100000);
  largeScript.fill(0x21); // NOPs
  largeScript[0] = 0x57; // INITSLOT
  largeScript[1] = 0x00;
  largeScript[2] = 0x00;
  largeScript[largeScript.length - 1] = 0x40; // RET
  
  const nef = buildNef({ script: largeScript });
  assert.doesNotThrow(() => analyzeBytes(nef));
});

test("boundary: back-to-back conditional jumps", () => {
  const script = new Uint8Array([
    0x11, // PUSH1 (true)
    0x26, 0x03, // JMPIFNOT +3 (skip next if false)
    0x11, // PUSH1
    0x26, 0x03, // JMPIFNOT +3
    0x11, // PUSH1
    0x26, 0x03, // JMPIFNOT +3
    0x11, // PUSH1
    0x40, // RET
  ]);
  const nef = buildNef({ script });
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("boundary: jump to self (infinite loop)", () => {
  const script = new Uint8Array([
    0x22, 0x00, // JMP +0 (infinite loop)
    0x40, // RET (unreachable)
  ]);
  const nef = buildNef({ script });
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("boundary: unreachable code after unconditional jump", () => {
  const script = new Uint8Array([
    0x22, 0x03, // JMP +3
    0x11, // PUSH1 (unreachable)
    0x12, // PUSH2 (unreachable)
    0x40, // RET
  ]);
  const nef = buildNef({ script });
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("boundary: empty blocks in control flow", () => {
  const script = new Uint8Array([
    0x11, // PUSH1
    0x26, 0x03, // JMPIFNOT +3
    0x22, 0x02, // JMP +2 (empty then block)
    0x40, // RET (else block)
  ]);
  const nef = buildNef({ script });
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("boundary: zero-sized data PUSHes", () => {
  const script = new Uint8Array([
    0x0c, 0x00, // PUSHDATA1 with 0 bytes
    0x40, // RET
  ]);
  const nef = buildNef({ script });
  assert.doesNotThrow(() => decompileBytes(nef));
});

test("boundary: all TRY variants", () => {
  const tryShort = new Uint8Array([
    0x3b, 0x04, 0x00, // TRY catch=+4
    0x11, // PUSH1
    0x3d, 0x02, // ENDTRY +2
    0x40, // RET
  ]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script: tryShort })));
  
  const tryLong = new Uint8Array([
    0x3c, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TRY_L
    0x11, // PUSH1
    0x3e, 0x02, 0x00, 0x00, 0x00, // ENDTRY_L +2
    0x40, // RET
  ]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script: tryLong })));
});

test("boundary: exception handling edge cases", () => {
  // THROW without try block
  const throwScript = new Uint8Array([0x11, 0x3a, 0x40]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script: throwScript })));
  
  // ABORT
  const abortScript = new Uint8Array([0x38, 0x40]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script: abortScript })));
  
  // ASSERT
  const assertScript = new Uint8Array([0x11, 0x39, 0x40]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script: assertScript })));
});

test("boundary: ENDFINALLY without preceding TRY", () => {
  const script = new Uint8Array([0x3f, 0x40]); // ENDFINALLY, RET
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script })));
});

test("boundary: very long method name in token", () => {
  const longName = "a".repeat(1000);
  const hash = new Uint8Array(20).fill(0xab);
  const nef = buildNef({
    tokens: [{
      hash,
      method: longName,
      params: 0,
      hasReturn: false,
      callFlags: 0x0f,
    }],
    script: new Uint8Array([0x37, 0x00, 0x00, 0x40]),
  });
  
  const parsed = parseNef(nef);
  assert.equal(parsed.methodTokens[0].method, longName);
});

test("boundary: max method tokens", () => {
  const tokens = [];
  for (let i = 0; i < 256; i++) {
    tokens.push({
      hash: new Uint8Array(20).fill(i),
      method: `method${i}`,
      params: i % 16,
      hasReturn: i % 2 === 0,
      callFlags: 0x0f,
    });
  }
  
  const nef = buildNef({
    tokens,
    script: new Uint8Array([0x37, 0x00, 0x00, 0x40]),
  });
  
  const parsed = parseNef(nef);
  assert.equal(parsed.methodTokens.length, 256);
});

test("boundary: all syscall hashes", () => {
  // Test with known syscall hashes
  const syscalls = [
    { hash: [0xb7, 0xc3, 0x88, 0x03], name: "GetTime" },
    { hash: [0xcf, 0xe7, 0x47, 0x96], name: "Log" },
    { hash: [0xf8, 0x27, 0xec, 0x8c], name: "CheckWitness" },
    { hash: [0xe6, 0x3f, 0x18, 0x84], name: "Storage.Put" },
  ];
  
  for (const { hash, name } of syscalls) {
    const script = new Uint8Array([0x41, ...hash, 0x40]);
    const result = decompileHighLevelBytes(buildNef({ script }));
    assert.ok(result.highLevel.includes(name) || result.highLevel.includes("syscall"), 
      `should handle ${name} syscall`);
  }
});

test("boundary: reserved bytes must be zero", () => {
  // Reserved byte at offset 68 should be 0
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0); // source len
  data.push(0x42); // reserved byte = non-zero (invalid)
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  assert.throws(() => parseNef(new Uint8Array(data)));
});

test("boundary: non-canonical varint encoding", () => {
  // Encoding 1 as 0xfd 0x01 0x00 is non-canonical (should be 0x01)
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0xfd, 0x01, 0x00); // non-canonical: using 2 bytes for value 1
  data.push(...new Uint8Array(1)); // source
  data.push(0); // reserved
  data.push(0); // tokens
  data.push(0, 0); // reserved word
  data.push(1, 0x40); // script
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  assert.throws(() => parseNef(new Uint8Array(data)));
});

test("boundary: all collection opcodes", () => {
  const collectionOps = [
    0xc0, // PACK
    0xc1, // UNPACK
    0xc2, // NEWARRAY0
    0xc3, // NEWARRAY
    0xc4, // NEWARRAY_T
    0xc5, // NEWSTRUCT0
    0xc6, // NEWSTRUCT
    0xc7, // NEWMAP
    0xbe, // PACKMAP
    0xbf, // PACKSTRUCT
    0xce, // PICKITEM
    0xcf, // APPEND
    0xd0, // SETITEM
    0xd1, // REVERSEITEMS
    0xd2, // REMOVE
    0xd3, // CLEARITEMS
    0xd4, // POPITEM
    0xcb, // HASKEY
    0xcc, // KEYS
    0xcd, // VALUES
  ];
  
  for (const opcode of collectionOps) {
    const script = opcode === 0xc4 
      ? new Uint8Array([0x11, opcode, 0x00, 0x40]) // NEWARRAY_T needs type byte
      : new Uint8Array([0x11, 0x12, opcode, 0x40]);
    assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script })), 
      `collection opcode 0x${opcode.toString(16)} should not crash`);
  }
});

test("boundary: string type conversions", () => {
  const script = new Uint8Array([
    0x0c, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f, // PUSHDATA1 "Hello"
    0xdb, 0x28, // CONVERT to ByteString (0x28)
    0x40, // RET
  ]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script })));
});

test("boundary: very deep operand stack simulation", () => {
  // Push many values then consume them
  const script = [];
  for (let i = 0; i < 50; i++) {
    script.push(0x11); // PUSH1
  }
  for (let i = 0; i < 25; i++) {
    script.push(0x9e); // ADD (pops 2, pushes 1)
  }
  script.push(0x40); // RET
  
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script: new Uint8Array(script) })));
});

test("boundary: multiple returns in same method", () => {
  const script = new Uint8Array([
    0x11, // PUSH1
    0x26, 0x03, // JMPIFNOT +3
    0x11, // PUSH1
    0x40, // RET (early return)
    0x12, // PUSH2
    0x40, // RET
  ]);
  assert.doesNotThrow(() => decompileHighLevelBytes(buildNef({ script })));
});

console.log("Boundary tests loaded");
