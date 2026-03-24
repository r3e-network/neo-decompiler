/**
 * Smoke Tests for Neo Decompiler JS
 * High-level sanity checks for critical functionality
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
  parseManifest,
} from "../src/index.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

function computeChecksum(payload) {
  const first = createHash("sha256").update(Buffer.from(payload)).digest();
  const second = createHash("sha256").update(first).digest();
  return new Uint8Array(second.subarray(0, 4));
}

function buildNefFromScript(scriptBytes) {
  const script = Array.from(scriptBytes);
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0); // source len
  data.push(0); // reserved
  data.push(0); // token count (varint = 0)
  data.push(0x00, 0x00); // reserved word
  
  // Script length (varint)
  if (script.length <= 0xfc) {
    data.push(script.length);
  } else {
    data.push(0xfd, script.length & 0xff, script.length >> 8);
  }
  data.push(...script);
  
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

// ─── Smoke Tests ────────────────────────────────────────────────────────────

test("SMOKE: Basic NEF parsing does not crash", () => {
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x40]));
  assert.doesNotThrow(() => parseNef(nef));
});

test("SMOKE: Basic disassembly does not crash", () => {
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x12, 0x9e, 0x40]));
  assert.doesNotThrow(() => disassembleScript(nef.slice(-8, -4))); // script portion
});

test("SMOKE: Full decompilation pipeline does not crash", () => {
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x40]));
  assert.doesNotThrow(() => decompileBytes(nef));
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
  assert.doesNotThrow(() => analyzeBytes(nef));
});

test("SMOKE: Empty script handled gracefully", () => {
  // Note: Empty scripts are invalid per NEF spec, should throw
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  data.push(...compiler);
  data.push(0); // source
  data.push(0); // reserved
  data.push(0); // tokens
  data.push(0x00, 0x00); // reserved word
  data.push(0); // empty script
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  assert.throws(() => parseNef(new Uint8Array(data)));
});

test("SMOKE: Large script handled without crash", () => {
  // 10KB of NOPs
  const largeScript = new Uint8Array(10240).fill(0x21);
  largeScript[largeScript.length - 1] = 0x40; // RET at end
  const nef = buildNefFromScript(largeScript);
  
  const result = decompileHighLevelBytes(nef);
  assert.ok(result.highLevel.length > 0, "should produce output");
  assert.ok(result.warnings.length === 0, "should not have warnings for valid script");
});

test("SMOKE: Deeply nested control flow handled", () => {
  let script = [0x57, 0x00, 0x00]; // INITSLOT 0,0
  for (let i = 0; i < 10; i++) {
    script.push(0x11); // PUSH1 (condition)
    script.push(0x26, 0x05); // JMPIFNOT +5
    script.push(0x21); // NOP (body)
    script.push(0x21); // NOP
    script.push(0x21); // NOP
  }
  script.push(0x40); // RET
  
  const nef = buildNefFromScript(new Uint8Array(script));
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("SMOKE: Many method tokens handled", () => {
  // This would require token support in buildNefFromScript
  // For now, just test basic parsing still works with tokens field
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x40]));
  const parsed = parseNef(nef);
  assert.ok(Array.isArray(parsed.methodTokens), "methodTokens should be array");
});

test("SMOKE: Unicode in manifest handled", () => {
  const manifest = JSON.stringify({
    name: "测试合约_Контракт_🚀",
    abi: {
      methods: [{
        name: "方法_метод",
        parameters: [{ name: "параметр_参数", type: "String" }],
        returntype: "String",
        offset: 0,
      }],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  
  assert.doesNotThrow(() => parseManifest(manifest));
  const parsed = parseManifest(manifest);
  assert.ok(parsed.name.includes("测试"));
});

test("SMOKE: All major opcodes can be disassembled", () => {
  // Test opcodes with proper operands where needed
  const opcodeTests = [
    { opcode: 0x00, extra: [0x00] }, // PUSHINT8 + byte
    { opcode: 0x0c, extra: [0x00] }, // PUSHDATA1 + 0 length (empty)
    { opcode: 0x21, extra: [] }, // NOP
    { opcode: 0x22, extra: [0x02] }, // JMP + 2 (jump to RET)
    { opcode: 0x34, extra: [0x02] }, // CALL + 2
    { opcode: 0x36, extra: [] }, // CALLA
    { opcode: 0x40, extra: [] }, // RET
    { opcode: 0x41, extra: [0x00, 0x00, 0x00, 0x00] }, // SYSCALL + 4 bytes
    { opcode: 0x4a, extra: [] }, // DUP
    { opcode: 0x50, extra: [] }, // SWAP
    { opcode: 0x56, extra: [0x01] }, // INITSSLOT + 1
    { opcode: 0x57, extra: [0x00, 0x00] }, // INITSLOT + 2 bytes
    { opcode: 0x68, extra: [] }, // LDLOC0
    { opcode: 0x70, extra: [] }, // STLOC0
    { opcode: 0x78, extra: [] }, // LDARG0
    { opcode: 0x9e, extra: [] }, // ADD
    { opcode: 0xc0, extra: [] }, // PACK
    { opcode: 0xc8, extra: [] }, // NEWMAP
  ];
  
  for (const { opcode, extra } of opcodeTests) {
    const script = new Uint8Array([opcode, ...extra, 0x40]); // opcode + operands + RET
    const nef = buildNefFromScript(script);
    assert.doesNotThrow(() => decompileBytes(nef), `opcode 0x${opcode.toString(16)} should not crash`);
  }
});

test("SMOKE: Invalid checksum properly rejected", () => {
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x40]));
  nef[nef.length - 1] ^= 0xff; // Corrupt checksum
  assert.throws(() => parseNef(nef), /checksum/);
});

test("SMOKE: Invalid magic properly rejected", () => {
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x40]));
  nef[0] = 0x58; // 'X' instead of 'N'
  assert.throws(() => parseNef(nef), /magic/);
});

test("SMOKE: Concurrent processing safe", async () => {
  const scripts = [
    new Uint8Array([0x11, 0x40]),
    new Uint8Array([0x12, 0x40]),
    new Uint8Array([0x13, 0x40]),
  ];
  
  const nefs = scripts.map(buildNefFromScript);
  
  // Process all concurrently
  const results = await Promise.all(
    nefs.map(nef => Promise.resolve(decompileHighLevelBytes(nef)))
  );
  
  assert.equal(results.length, 3);
  results.forEach((r, i) => {
    assert.ok(r.highLevel.includes(`script_entry`), `result ${i} should have entry`);
  });
});

test("SMOKE: Memory usage reasonable for moderate input", () => {
  const script = new Uint8Array(10000).fill(0x21);
  script[script.length - 1] = 0x40;
  const nef = buildNefFromScript(script);
  
  const before = process.memoryUsage();
  analyzeBytes(nef);
  const after = process.memoryUsage();
  
  const heapGrowth = (after.heapUsed - before.heapUsed) / 1024 / 1024;
  assert.ok(heapGrowth < 50, `heap growth ${heapGrowth.toFixed(2)}MB should be < 50MB`);
});

test("SMOKE: Output is deterministic", () => {
  const nef = buildNefFromScript(new Uint8Array([
    0x57, 0x01, 0x00,
    0x11, 0x70, 0x68, 0x40,
  ]));
  
  const result1 = decompileHighLevelBytes(nef);
  const result2 = decompileHighLevelBytes(nef);
  
  assert.equal(result1.highLevel, result2.highLevel, "output should be deterministic");
  assert.equal(result1.pseudocode, result2.pseudocode, "pseudocode should be deterministic");
});

test("SMOKE: All API functions exported and callable", () => {
  const nef = buildNefFromScript(new Uint8Array([0x11, 0x40]));
  
  // All exports should be callable
  assert.doesNotThrow(() => parseNef(nef));
  assert.doesNotThrow(() => disassembleScript(new Uint8Array([0x11, 0x40])));
  assert.doesNotThrow(() => decompileBytes(nef));
  assert.doesNotThrow(() => decompileHighLevelBytes(nef));
});

test("SMOKE: Malformed JSON manifest handled", () => {
  assert.throws(() => parseManifest("not valid json"));
  assert.throws(() => parseManifest("{ incomplete"));
});

test("SMOKE: Empty manifest fields handled", () => {
  const manifest = JSON.stringify({
    name: "",
    abi: { methods: [], events: [] },
    permissions: [],
    trusts: [],
  });
  
  assert.doesNotThrow(() => parseManifest(manifest));
  const parsed = parseManifest(manifest);
  assert.equal(parsed.name, "");
  assert.equal(parsed.abi.methods.length, 0);
});

console.log("Smoke tests loaded");
