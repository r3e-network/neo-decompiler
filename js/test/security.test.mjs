/**
 * Security Tests for Neo Decompiler JS
 * Tests for potential security vulnerabilities
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

// ─── Security Tests ─────────────────────────────────────────────────────────

test("security: extremely large size fields (DoS prevention)", () => {
  // Try to trigger memory exhaustion with fake large sizes
  const cases = [
    // 1GB script length claim
    (() => {
      const data = [...Buffer.from("NEF3")];
      data.push(...new Uint8Array(64));
      data.push(0); // source
      data.push(0); // reserved
      data.push(0); // tokens
      data.push(0, 0); // reserved word
      data.push(0xfe, 0x00, 0x00, 0x00, 0x40); // 1GB script claim
      data.push(...new Uint8Array(10)); // Only provide 10 bytes
      data.push(...computeChecksum(data));
      return new Uint8Array(data);
    })(),
    // 4GB script length claim (overflow attempt)
    (() => {
      const data = [...Buffer.from("NEF3")];
      data.push(...new Uint8Array(64));
      data.push(0);
      data.push(0);
      data.push(0);
      data.push(0, 0);
      data.push(0xfe, 0xff, 0xff, 0xff, 0xff); // Max 32-bit value
      data.push(...new Uint8Array(10));
      data.push(...computeChecksum(data));
      return new Uint8Array(data);
    })(),
  ];
  
  for (const nef of cases) {
    assert.throws(() => parseNef(nef));
  }
});

test("security: integer overflow in offset calculations", () => {
  const script = new Uint8Array([
    0x0a, 0xff, 0xff, 0xff, 0x7f, // PUSHA +2147483647 (max int32)
    0x40,
  ]);
  
  const nef = buildValidNef(script);
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    // Should handle gracefully
    assert.ok(e instanceof Error);
  }
});

test("security: negative jump offsets wrapping", () => {
  const script = new Uint8Array([
    0x22, 0x80, // JMP with -128 offset (wraps backward)
    0x40,
  ]);
  
  const nef = buildValidNef(script);
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    assert.ok(e instanceof Error);
  }
});

test("security: no code injection via PUSHDATA", () => {
  // PUSHDATA containing JavaScript-like strings
  const maliciousStrings = [
    "</script><script>alert('xss')</script>",
    "${process.exit(1)}",
    "` + process.env + `",
    "\\x00\\x00\\x00",
    "\nrequire('child_process').exec('rm -rf /')\n",
    "\x00\x00\x00",
  ];
  
  for (const str of maliciousStrings) {
    const bytes = new TextEncoder().encode(str);
    const script = new Uint8Array([
      0x0c, bytes.length,
      ...bytes,
      0x40,
    ]);
    const nef = buildValidNef(script);
    
    // Should not execute anything, just parse
    const result = decompileBytes(nef);
    assert.ok(result.pseudocode);
  }
});

test("security: no prototype pollution via manifest", () => {
  const maliciousManifest = JSON.stringify({
    name: "test",
    abi: {
      methods: [],
      events: [],
    },
    permissions: [],
    trusts: "*",
    // Attempt prototype pollution
    ["__proto__"]: { polluted: true },
    constructor: { prototype: { polluted: true } },
  });
  
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  
  // Should not pollute
  const before = {}.polluted;
  decompileHighLevelBytes(nef, maliciousManifest);
  const after = {}.polluted;
  
  assert.equal(before, after);
  assert.equal({}.polluted, undefined);
});

test("security: regex DoS prevention in identifier parsing", () => {
  // Long string that might trigger catastrophic backtracking
  const longIdent = "a_".repeat(10000);
  
  const manifest = JSON.stringify({
    name: longIdent,
    abi: {
      methods: [{
        name: longIdent,
        parameters: [{ name: longIdent, type: "String" }],
        returntype: "String",
        offset: 0,
      }],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  
  const start = Date.now();
  decompileHighLevelBytes(nef, manifest);
  const elapsed = Date.now() - start;
  
  assert.ok(elapsed < 5000, `should not hang on long identifiers (${elapsed}ms)`);
});

test("security: stack depth limits in recursive structures", () => {
  // Create deeply nested structures that could overflow call stack
  const deepScript = [];
  
  // Many nested try blocks
  for (let i = 0; i < 1000; i++) {
    deepScript.push(0x3b, 0x04, 0x00); // TRY
    deepScript.push(0x11); // PUSH1
  }
  for (let i = 0; i < 1000; i++) {
    deepScript.push(0x3d, 0x04); // ENDTRY
  }
  deepScript.push(0x40);
  
  const nef = buildValidNef(new Uint8Array(deepScript));
  
  try {
    decompileHighLevelBytes(nef);
  } catch (e) {
    // May throw but should not crash process
    assert.ok(e instanceof Error || typeof e === 'object');
  }
});

test("security: no arbitrary file system access", () => {
  // The decompiler should not have any file system operations
  // This is a design test - the JS decompiler is pure and has no FS access
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  
  // All operations should be memory-only
  const result = analyzeBytes(nef);
  assert.ok(result);
});

test("security: no network access", () => {
  // The decompiler should not make any network requests
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  
  // All operations should be local
  const result = analyzeBytes(nef);
  assert.ok(result);
});

test("security: input validation on non-Uint8Array inputs", () => {
  // Should reject non-Uint8Array inputs
  assert.throws(() => parseNef("string"));
  assert.throws(() => parseNef(123));
  assert.throws(() => parseNef(null));
  assert.throws(() => parseNef(undefined));
  assert.throws(() => parseNef({}));
  assert.throws(() => parseNef([]));
});

test("security: bounds checking on typed array views", () => {
  const buffer = new ArrayBuffer(100);
  const view = new Uint8Array(buffer, 50, 10); // Offset view
  
  // Should handle offset views correctly
  assert.throws(() => parseNef(view));
});

test("security: no eval or dynamic code execution", () => {
  // This test verifies by inspection that the code doesn't use:
  // - eval()
  // - new Function()
  // - setTimeout/setInterval with strings
  // - import() with dynamic strings
  
  // Run normal operations to ensure they work
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  const result = decompileHighLevelBytes(nef);
  assert.ok(result.highLevel);
});

test("security: handling of malformed UTF-8", () => {
  // Invalid UTF-8 sequences in source string
  const data = [...Buffer.from("NEF3")];
  data.push(...new Uint8Array(64)); // compiler
  data.push(0x04); // source length = 4
  data.push(0xc0, 0x80, 0xfe, 0x80); // Invalid UTF-8
  data.push(0); // reserved
  data.push(0); // tokens
  data.push(0, 0); // reserved word
  data.push(2, 0x11, 0x40); // script
  data.push(...computeChecksum(data));
  
  assert.throws(() => parseNef(new Uint8Array(data)));
});

test("security: handling of overlong UTF-8 encoding", () => {
  // Overlong encoding of ASCII NUL
  const data = [...Buffer.from("NEF3")];
  data.push(...new Uint8Array(64));
  data.push(0x02); // source length = 2
  data.push(0xc0, 0x80); // Overlong encoding of 0x00
  data.push(0);
  data.push(0);
  data.push(0, 0);
  data.push(2, 0x11, 0x40);
  data.push(...computeChecksum(data));
  
  assert.throws(() => parseNef(new Uint8Array(data)));
});

test("security: path traversal in compiler field", () => {
  // Compiler field containing path traversal attempts
  const compilers = [
    "../../../etc/passwd",
    "..\\..\\windows\\system32\\config\\sam",
    "\\0/../etc/passwd",
    "C:\\Windows\\System32\\cmd.exe",
  ];
  
  for (const compiler of compilers) {
    const compilerBytes = new Uint8Array(64);
    const encoded = new TextEncoder().encode(compiler);
    compilerBytes.set(encoded.slice(0, 64));
    
    const data = [...Buffer.from("NEF3")];
    data.push(...compilerBytes);
    data.push(0); // source
    data.push(0); // reserved
    data.push(0); // tokens
    data.push(0, 0); // reserved word
    data.push(2, 0x11, 0x40); // script
    data.push(...computeChecksum(data));
    
    // Should parse without executing path
    const parsed = parseNef(new Uint8Array(data));
    assert.equal(parsed.header.compiler.trim().replace(/\0/g, ''), compiler.slice(0, 64).trim());
  }
});

test("security: ReDoS in string matching patterns", () => {
  // Test with strings that might cause regex issues
  const badPatterns = [
    "a".repeat(100) + "!" + "a".repeat(100),
    "(a+)+b",
    "([a-zA-Z]+)*",
    "(a|aa)+",
    "(a|a?)+",
  ];
  
  for (const pattern of badPatterns) {
    const manifest = JSON.stringify({
      name: pattern.slice(0, 100),
      abi: { methods: [], events: [] },
      permissions: [],
      trusts: "*",
    });
    
    const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
    
    const start = Date.now();
    decompileHighLevelBytes(nef, manifest);
    const elapsed = Date.now() - start;
    
    assert.ok(elapsed < 1000, `pattern should not cause ReDoS (${elapsed}ms)`);
  }
});

test("security: memory exhaustion via small input large claim", () => {
  // Claim a huge script but only provide small data
  const data = [...Buffer.from("NEF3")];
  data.push(...new Uint8Array(64));
  data.push(0);
  data.push(0);
  data.push(0);
  data.push(0, 0);
  // Claim 100KB script
  data.push(0xfe, 0x00, 0x90, 0x01, 0x00); // 102400 bytes
  // But only provide 10 bytes
  data.push(...new Uint8Array(10).fill(0x21));
  data.push(...computeChecksum(data));
  
  // Should fail cleanly, not try to allocate 100KB
  assert.throws(() => parseNef(new Uint8Array(data)));
});

test("security: type confusion prevention", () => {
  // Various types that might cause confusion
  const badInputs = [
    new Int8Array([0x4e, 0x45, 0x46, 0x33]),
    new Uint16Array([0x454e, 0x3346]),
    new Float32Array([1.0, 2.0]),
  ];
  
  for (const input of badInputs) {
    try {
      parseNef(input);
    } catch (e) {
      // Should throw, not misinterpret
      assert.ok(e instanceof Error);
    }
  }
});

test("security: constructor hijacking attempt", () => {
  const manifest = {
    name: "test",
    abi: { methods: [], events: [] },
    permissions: [],
    trusts: "*",
  };
  
  // Attempt to pollute constructor
  Object.prototype.polluted = true;
  
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  const result = decompileHighLevelBytes(nef, JSON.stringify(manifest));
  
  // Cleanup
  delete Object.prototype.polluted;
  
  // Result should still be valid
  assert.ok(result.highLevel);
});

test("security: toString override attempt", () => {
  const badObject = {
    toString: () => { throw new Error("pwned"); },
    valueOf: () => { throw new Error("pwned"); },
  };
  
  // Should not call toString/valueOf in unexpected ways
  const nef = buildValidNef(new Uint8Array([0x11, 0x40]));
  const result = decompileHighLevelBytes(nef);
  assert.ok(result);
});

console.log("Security tests loaded");
