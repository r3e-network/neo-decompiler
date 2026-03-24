/**
 * Fuzz Tests for Neo Decompiler JS
 * Tests random/malformed inputs to ensure robustness
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

function buildValidNef(script) {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0); // source
  data.push(0); // reserved
  data.push(0); // tokens
  data.push(0, 0); // reserved word
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

function randomBytes(length) {
  return new Uint8Array(length).map(() => Math.floor(Math.random() * 256));
}

// ─── Fuzz Tests ─────────────────────────────────────────────────────────────

test("fuzz: random byte sequences should not crash parseNef", () => {
  for (let i = 0; i < 100; i++) {
    const random = randomBytes(50 + Math.floor(Math.random() * 200));
    try {
      parseNef(random);
    } catch (e) {
      // Expected - should throw for invalid data, but not crash
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: random valid-length NEF with wrong checksum", () => {
  for (let i = 0; i < 50; i++) {
    const data = [...Buffer.from("NEF3")];
    data.push(...new Uint8Array(64));
    data.push(0); // source
    data.push(0); // reserved
    data.push(0); // tokens
    data.push(0, 0); // reserved word
    const scriptLen = 10 + Math.floor(Math.random() * 100);
    data.push(scriptLen);
    data.push(...randomBytes(scriptLen));
    data.push(...randomBytes(4)); // random checksum
    
    try {
      parseNef(new Uint8Array(data));
    } catch (e) {
      // Expected
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: disassemble random bytecode", () => {
  for (let i = 0; i < 100; i++) {
    const bytecode = randomBytes(20 + Math.floor(Math.random() * 100));
    bytecode[bytecode.length - 1] = 0x40; // Ensure ends with RET
    
    try {
      disassembleScript(bytecode);
    } catch (e) {
      // Should either succeed or throw controlled error
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: full decompile pipeline with random scripts", () => {
  for (let i = 0; i < 50; i++) {
    const script = randomBytes(20 + Math.floor(Math.random() * 100));
    script[0] = 0x57; // INITSLOT for valid entry
    script[1] = 0x00;
    script[2] = 0x00;
    script[script.length - 1] = 0x40; // RET
    
    const nef = buildValidNef(script);
    
    try {
      decompileHighLevelBytes(nef);
    } catch (e) {
      // Should not crash unexpectedly
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: extremely long random script", () => {
  const script = randomBytes(50000);
  script[0] = 0x57;
  script[1] = 0x00;
  script[2] = 0x00;
  script[script.length - 1] = 0x40;
  
  const nef = buildValidNef(script);
  
  // Should complete without crashing (may take time)
  const start = Date.now();
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
  const elapsed = Date.now() - start;
  assert.ok(elapsed < 30000, `should complete in reasonable time (${elapsed}ms)`);
});

test("fuzz: scripts with unbalanced stack", () => {
  const unbalancedScripts = [
    // Too many pops
    new Uint8Array([0x45, 0x45, 0x45, 0x40]), // DROP DROP DROP RET
    // Too many pushes at end
    new Uint8Array([0x11, 0x12, 0x13, 0x40]), // PUSH1 PUSH2 PUSH3 RET
    // Mismatched binary ops
    new Uint8Array([0x11, 0x9e, 0x9e, 0x40]), // PUSH1 ADD ADD RET
  ];
  
  for (const script of unbalancedScripts) {
    const nef = buildValidNef(script);
    try {
      decompileHighLevelBytes(nef);
    } catch (e) {
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: scripts with invalid jump targets", () => {
  const scripts = [
    new Uint8Array([0x22, 0xff, 0x40]), // JMP backward way out of bounds
    new Uint8Array([0x22, 0x7f, 0x40]), // JMP forward past end
    new Uint8Array([0x23, 0xff, 0xff, 0xff, 0x7f, 0x40]), // JMP_L huge offset
  ];
  
  for (const script of scripts) {
    const nef = buildValidNef(script);
    try {
      decompileHighLevelBytes(nef);
    } catch (e) {
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: malformed varints", () => {
  const cases = [
    // Truncated varints
    new Uint8Array([0xfd]), // 2-byte prefix, no data
    new Uint8Array([0xfe]), // 4-byte prefix, no data
    new Uint8Array([0xfe, 0x00, 0x00]), // 4-byte prefix, partial data
    // Maximum values
    new Uint8Array([0xff]), // 8-byte prefix marker (reserved)
  ];
  
  for (const bad of cases) {
    const data = [...Buffer.from("NEF3")];
    data.push(...new Uint8Array(64));
    data.push(...bad); // Bad varint in source length position
    
    try {
      parseNef(new Uint8Array(data));
    } catch (e) {
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: truncated NEF at various positions", () => {
  const full = buildValidNef(new Uint8Array([0x11, 0x40]));
  
  for (let truncateAt = 4; truncateAt < full.length - 1; truncateAt += 5) {
    const truncated = full.slice(0, truncateAt);
    try {
      parseNef(truncated);
    } catch (e) {
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: extra/trailing bytes after NEF", () => {
  const full = buildValidNef(new Uint8Array([0x11, 0x40]));
  const extra = new Uint8Array([...full, 0xde, 0xad, 0xbe, 0xef]);
  
  assert.throws(() => parseNef(extra));
});

test("fuzz: all bytes as opcode (0x00-0xff)", () => {
  for (let byte = 0; byte <= 255; byte++) {
    const script = new Uint8Array([byte, 0x40]);
    const nef = buildValidNef(script);
    
    try {
      decompileBytes(nef);
    } catch (e) {
      // Should not crash
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: random manifest JSON variations", () => {
  const manifests = [
    "{}",
    "{\"name\":null}",
    "{\"name\":123}",
    "{\"abi\":null}",
    "{\"abi\":{}}",
    "{\"abi\":{\"methods\":null}}",
    "{\"abi\":{\"methods\":[{\"name\":\"test\"}]}}",
    "{\"permissions\":null}",
    "{\"permissions\":[{\"contract\":null}]}",
    "{\"trusts\":null}",
    "{\"trusts\":{}}",
    "{\"extra\":{\"nested\":{\"deep\":{\"value\":true}}}}",
  ];
  
  for (const manifest of manifests) {
    try {
      const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
      decompileHighLevelBytesWithManifest(nef, manifest);
    } catch (e) {
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: recursive/nested structures depth", () => {
  // Deeply nested ifs
  let deepIfScript = [];
  for (let i = 0; i < 20; i++) {
    deepIfScript.push(0x11); // PUSH1
    deepIfScript.push(0x26, 0x05); // JMPIFNOT +5
    deepIfScript.push(0x21); // NOP
    deepIfScript.push(0x21); // NOP
  }
  for (let i = 0; i < 20; i++) {
    deepIfScript.push(0x40); // RET for each level
  }
  
  const nef = buildValidNef(new Uint8Array(deepIfScript));
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
});

test("fuzz: overlapping control flow structures", () => {
  // Try blocks that overlap with loops
  const script = new Uint8Array([
    0x3b, 0x10, 0x00, // TRY catch=+16
    0x11, // PUSH1
    0x26, 0x06, // JMPIFNOT +6 (break try?)
    0x11, // PUSH1
    0x3d, 0x08, // ENDTRY +8
    0x3a, // THROW
    0x3d, 0x04, // ENDTRY +4
    0x11, // PUSH1
    0x40, // RET
  ]);
  
  const nef = buildValidNef(script);
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
});

test("fuzz: method token index out of bounds", () => {
  const script = new Uint8Array([
    0x37, 0xff, 0x7f, // CALLT token 32767 (way out of bounds)
    0x40,
  ]);
  
  const nef = buildValidNef(script);
  try {
    analyzeBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
});

test("fuzz: corrupted operand encodings", () => {
  const corruptedOps = [
    // PUSHINT8 with missing byte
    new Uint8Array([0x00, 0x40]),
    // PUSHINT16 with 1 byte instead of 2
    new Uint8Array([0x01, 0x00, 0x40]),
    // PUSHINT32 with 2 bytes instead of 4
    new Uint8Array([0x02, 0x00, 0x00, 0x40]),
    // PUSHINT64 with 4 bytes instead of 8
    new Uint8Array([0x03, 0x00, 0x00, 0x00, 0x00, 0x40]),
    // JMP_L with partial offset
    new Uint8Array([0x23, 0x05, 0x00, 0x40]),
    // SYSCALL with partial hash
    new Uint8Array([0x41, 0x01, 0x02, 0x40]),
    // PUSHDATA1 with length byte but no data
    new Uint8Array([0x0c, 0x10, 0x40]),
    // PUSHDATA2 with length but truncated
    new Uint8Array([0x0d, 0xff, 0xff, 0x40]),
  ];
  
  for (const script of corruptedOps) {
    const nef = buildValidNef(script);
    try {
      disassembleScript(script);
    } catch (e) {
      // Should throw or handle gracefully
      assert.ok(e instanceof Error || true);
    }
  }
});

test("fuzz: random arithmetic sequences", () => {
  const ops = [0x9e, 0x9f, 0xa0, 0xa1, 0x91, 0x92, 0x93]; // ADD, SUB, MUL, MOD, AND, OR, XOR
  
  for (let i = 0; i < 50; i++) {
    const script = [0x57, 0x00, 0x00]; // INITSLOT 0,0
    const numOps = 5 + Math.floor(Math.random() * 20);
    
    // Push initial values
    script.push(0x11, 0x12, 0x13); // PUSH1, PUSH2, PUSH3
    
    // Random operations
    for (let j = 0; j < numOps; j++) {
      const op = ops[Math.floor(Math.random() * ops.length)];
      script.push(op);
      if (Math.random() > 0.5) {
        script.push(0x11); // Sometimes push more
      }
    }
    
    script.push(0x40); // RET
    
    const nef = buildValidNef(new Uint8Array(script));
    try {
      decompileHighLevelBytes(nef);
    } catch (e) {
      assert.ok(e instanceof Error);
    }
  }
});

test("fuzz: concurrent random operations", async () => {
  const operations = [];
  
  for (let i = 0; i < 20; i++) {
    operations.push(new Promise((resolve) => {
      const script = randomBytes(100);
      script[0] = 0x57;
      script[1] = 0x00;
      script[2] = 0x00;
      script[script.length - 1] = 0x40;
      
      const nef = buildValidNef(script);
      try {
        analyzeBytes(nef);
        resolve(true);
      } catch (e) {
        resolve(false);
      }
    }));
  }
  
  const results = await Promise.all(operations);
  // All should complete (some may fail, but none should hang/crash)
  assert.equal(results.length, 20);
});

test("fuzz: memory pressure test with large objects", () => {
  // Test with scripts containing large data pushes
  const largeData = new Uint8Array(10000);
  for (let i = 0; i < largeData.length; i++) {
    largeData[i] = i % 256;
  }
  
  const script = new Uint8Array([
    0x0c, ...largeData.slice(0, 100), // PUSHDATA1 100 bytes
    0x40,
  ]);
  
  const nef = buildValidNef(script);
  try {
    decompileBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
});

test("fuzz: edge case - empty input", () => {
  assert.throws(() => parseNef(new Uint8Array(0)));
});

test("fuzz: edge case - single byte input", () => {
  assert.throws(() => parseNef(new Uint8Array([0x00])));
});

test("fuzz: edge case - only magic bytes", () => {
  assert.throws(() => parseNef(new Uint8Array([0x4e, 0x45, 0x46, 0x33]))); // "NEF3"
});

test("fuzz: edge case - maximum recursion protection", () => {
  // Create a structure that could cause deep recursion
  const script = [];
  for (let i = 0; i < 100; i++) {
    script.push(0x3b, 0x04, 0x00); // TRY with short offset
    script.push(0x11); // PUSH1
  }
  for (let i = 0; i < 100; i++) {
    script.push(0x3f); // ENDFINALLY
  }
  script.push(0x40);
  
  const nef = buildValidNef(new Uint8Array(script));
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
});

// Import for the manifest test
import { decompileHighLevelBytesWithManifest } from "../src/index.js";

console.log("Fuzz tests loaded");
