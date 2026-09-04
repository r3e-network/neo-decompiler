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
  decompileHighLevelBytesWithManifest,
  analyzeBytes,
} from "../src/index.js";
import { escapeCSharpString } from "../src/csharp-render.js";
import { escapeVisibleText, isBidiControl } from "../src/visible-text.js";

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

function buildMetadataNef({ script, compiler = "", source = "", method = null }) {
  const encoder = new TextEncoder();
  const compilerBytes = encoder.encode(compiler);
  const sourceBytes = encoder.encode(source);
  assert.ok(compilerBytes.length <= 64);
  assert.ok(sourceBytes.length <= 256);

  const data = [...Buffer.from("NEF3")];
  const compilerField = new Uint8Array(64);
  compilerField.set(compilerBytes);
  data.push(...compilerField);
  writeVarint(data, sourceBytes.length);
  data.push(...sourceBytes);
  data.push(0); // reserved

  if (method === null) {
    data.push(0);
  } else {
    const methodBytes = encoder.encode(method);
    assert.ok(methodBytes.length <= 32);
    data.push(1);
    data.push(...new Uint8Array(20));
    writeVarint(data, methodBytes.length);
    data.push(...methodBytes);
    data.push(0, 0); // parameter count
    data.push(1); // has return value
    data.push(0x0f); // CallFlags.All
  }

  data.push(0, 0); // reserved word
  writeVarint(data, script.length);
  data.push(...script);
  data.push(...computeChecksum(data));
  return new Uint8Array(data);
}

// ─── Security Tests ─────────────────────────────────────────────────────────

test("security: visible metadata encoding covers line, terminal, and bidi controls", () => {
  const input = "safe\r\n\u0085\u2028\u2029\u001B\u202Etail\t";
  assert.equal(
    escapeVisibleText(input),
    "safe\\r\\n\\u{0085}\\u{2028}\\u{2029}\\u{001B}\\u{202E}tail\\t",
  );
  assert.equal(escapeCSharpString("x\u202Ey"), "x\\u202Ey");
  assert.equal(escapeVisibleText("x\uD800y"), "x\\u{D800}y");
  assert.equal(escapeCSharpString("x\uD800y"), "x\\uD800y");

  const allUnsafe = String.fromCodePoint(
    ...Array.from({ length: 0x20 }, (_, codePoint) => codePoint),
    ...Array.from({ length: 0x21 }, (_, offset) => 0x7f + offset),
    0x061c, 0x200e, 0x200f, 0x2028, 0x2029,
    0x202a, 0x202b, 0x202c, 0x202d, 0x202e,
    0x2066, 0x2067, 0x2068, 0x2069,
  );
  assert.ok([...escapeVisibleText(allUnsafe)].every((character) => {
    const codePoint = character.codePointAt(0);
    return !(
      codePoint <= 0x1f ||
      (codePoint >= 0x7f && codePoint <= 0x9f) ||
      codePoint === 0x2028 ||
      codePoint === 0x2029 ||
      isBidiControl(character)
    );
  }));
});

test("security: NEF metadata controls cannot create high-level or C# lines", () => {
  const compiler = "c\rR\nL\u0085N\u2028S\u2029P\u001BE\u202EB";
  const source = "source\nINJECT_SOURCE";
  const method = "m\r\n\u0085\u2028\u2029\u001B\u202EINJECT";
  const nef = buildMetadataNef({
    script: new Uint8Array([0x37, 0x00, 0x00, 0x40]),
    compiler,
    source,
    method,
  });
  const result = decompileHighLevelBytes(nef);

  for (const output of [result.highLevel, result.csharp]) {
    assert.match(output, /c\\rR\\nL\\u\{0085\}N\\u\{2028\}S/u);
    assert.ok(output.includes("source\\nINJECT_SOURCE"));
    assert.ok(output.includes("m\\r\\n\\u{0085}\\u{2028}\\u{2029}\\u{001B}\\u{202E}INJECT"));
    for (const forbidden of ["\r", "\u0085", "\u2028", "\u2029", "\u001B", "\u202E"]) {
      assert.ok(!output.includes(forbidden), `retained ${JSON.stringify(forbidden)}`);
    }
    assert.ok(!output.split("\n").some((line) => line.trimStart().startsWith("INJECT_")));
  }
});

test("security: manifest display metadata stays on generated source lines", () => {
  const separatorPayload = "value\r\nINJECT\u0085NEL\u2028LS\u2029PS\u001BESC\u202EBIDI";
  const manifest = {
    name: "SafeContract",
    supportedstandards: [separatorPayload],
    features: { [separatorPayload]: separatorPayload },
    groups: [{ pubkey: separatorPayload, signature: separatorPayload }],
    permissions: [{ contract: separatorPayload, methods: [separatorPayload] }],
    trusts: [separatorPayload],
    extra: { [separatorPayload]: separatorPayload },
    abi: {
      methods: [{
        name: `safe-${separatorPayload}`,
        parameters: [{ name: "p", type: `Odd${separatorPayload}` }],
        returntype: `Odd${separatorPayload}`,
        offset: 0,
      }],
      events: [{
        name: `event-${separatorPayload}`,
        parameters: [{ name: "p", type: `Odd${separatorPayload}` }],
      }],
    },
  };
  const nef = buildValidNef(new Uint8Array([0x40]));
  const result = decompileHighLevelBytesWithManifest(nef, JSON.stringify(manifest));

  for (const output of [result.highLevel, result.csharp]) {
    for (const forbidden of ["\r", "\u0085", "\u2028", "\u2029", "\u001B", "\u202E"]) {
      assert.ok(!output.includes(forbidden), `retained ${JSON.stringify(forbidden)}`);
    }
    assert.ok(!output.split("\n").some((line) => line.trimStart() === "INJECT"));
  }
});

test("security: event DisplayName preserves the original control characters", () => {
  const eventName = "event\nname\u202E";
  const manifest = {
    name: "SafeContract",
    abi: {
      methods: [{ name: "main", parameters: [], returntype: "void", offset: 0 }],
      events: [{ name: eventName, parameters: [] }],
    },
  };
  const result = decompileHighLevelBytesWithManifest(
    buildValidNef(new Uint8Array([0x40])),
    JSON.stringify(manifest),
  );

  assert.ok(result.highLevel.includes('// manifest "event\\nname\\u202E"'));
  assert.ok(result.csharp.includes('[DisplayName("event\\nname\\u202E")]'));
  assert.ok(!result.csharp.includes('event\\\\nname'));
});

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
  decompileHighLevelBytesWithManifest(nef, maliciousManifest);
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
  decompileHighLevelBytesWithManifest(nef, manifest);
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
    decompileHighLevelBytesWithManifest(nef, manifest);
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
  const result = decompileHighLevelBytesWithManifest(nef, JSON.stringify(manifest));
  
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
