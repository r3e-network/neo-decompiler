/**
 * Integration Tests for Neo Decompiler JS
 * Tests complete workflows and real-world scenarios
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
  parseManifest,
} from "../src/index.js";

// ─── Test Helpers ───────────────────────────────────────────────────────────

function computeChecksum(payload) {
  const first = createHash("sha256").update(Buffer.from(payload)).digest();
  const second = createHash("sha256").update(first).digest();
  return Array.from(second.subarray(0, 4));
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
  const script = Array.from(opts.script ?? [0x11, 0x40]); // PUSH1; RET
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from(opts.compiler ?? "test"), 0);
  data.push(...compiler);
  
  // Source
  const source = opts.source ?? "";
  writeVarint(data, Buffer.byteLength(source));
  data.push(...Buffer.from(source));
  
  // Reserved byte
  data.push(0);
  
  // Method tokens
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
  
  // Reserved word
  data.push(0x00, 0x00);
  
  // Script
  writeVarint(data, script.length);
  data.push(...script);
  
  // Checksum
  const checksum = computeChecksum(data);
  data.push(...checksum);
  
  return new Uint8Array(data);
}

// ─── Integration Test Suite ─────────────────────────────────────────────────

test("integration: complete workflow from bytes to analysis output", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x01, // INITSLOT 1 local, 1 arg
    0x78, // LDARG0
    0x11, // PUSH1
    0x9e, // ADD
    0x70, // STLOC0
    0x68, // LDLOC0
    0x40, // RET
  ]);
  
  const nef = buildNef({ script });
  
  // Full analysis
  const result = analyzeBytes(nef);
  
  assert.ok(result.nef, "should parse NEF");
  assert.ok(result.instructions.length > 0, "should disassemble");
  assert.ok(result.pseudocode, "should produce pseudocode");
  assert.ok(result.callGraph, "should build call graph");
  assert.ok(result.xrefs, "should build xrefs");
  assert.ok(result.types, "should infer types");
  
  // Also test high-level decompilation separately
  const hlResult = decompileHighLevelBytes(nef);
  assert.ok(hlResult.highLevel, "should produce high-level output");
  assert.match(hlResult.highLevel, /contract/, "should have contract declaration");
  assert.match(hlResult.highLevel, /fn/, "should have function declaration");
});

test("integration: ERC-20 like token contract simulation", () => {
  // Simulates a simple token transfer function
  const script = new Uint8Array([
    0x57, 0x02, 0x03, // INITSLOT 2 locals, 3 args (from, to, amount)
    // Check if amount > 0
    0x7a, // LDARG2 (amount)
    0x10, // PUSH0
    0xa5, // GT
    0x26, 0x05, // JMPIFNOT +5 (to error)
    // Continue execution
    0x11, // PUSH1
    0x70, // STLOC0 (success flag)
    0x22, 0x03, // JMP +3
    // Error case
    0x10, // PUSH0
    0x70, // STLOC0
    // Return
    0x68, // LDLOC0
    0x40, // RET
  ]);
  
  const manifest = JSON.stringify({
    name: "TestToken",
    abi: {
      methods: [{
        name: "transfer",
        parameters: [
          { name: "from", type: "Hash160" },
          { name: "to", type: "Hash160" },
          { name: "amount", type: "Integer" },
        ],
        returntype: "Boolean",
        offset: 0,
        safe: false,
      }],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  
  const result = decompileHighLevelBytesWithManifest(
    buildNef({ script }),
    manifest,
  );
  
  assert.match(result.highLevel, /TestToken/, "should have contract name");
  assert.match(result.highLevel, /transfer/, "should have transfer function");
  assert.match(result.highLevel, /from.*hash160/, "should have typed parameters");
  assert.match(result.highLevel, /if/, "should have conditional");
});

test("integration: multi-method contract with complex control flow", () => {
  const script = new Uint8Array([
    // Method 1: entry point
    0x57, 0x00, 0x01, // INITSLOT 0 locals, 1 arg
    0x78, // LDARG0
    0x26, 0x05, // JMPIFNOT +5
    0x34, 0x06, // CALL +6 (to helper)
    0x40, // RET
    0x11, // PUSH1
    0x40, // RET (default return)
    
    // Method 2: helper
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, // PUSH1
    0x70, // STLOC0
    0x68, // LDLOC0
    0x12, // PUSH2
    0x9e, // ADD
    0x40, // RET
  ]);
  
  const manifest = JSON.stringify({
    name: "MultiMethod",
    abi: {
      methods: [
        { name: "main", parameters: [{ name: "flag", type: "Boolean" }], returntype: "Integer", offset: 0 },
        { name: "helper", parameters: [], returntype: "Integer", offset: 9 },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  
  const result = analyzeBytes(buildNef({ script }), manifest);
  
  // Method detection may find 2-3 methods depending on heuristics
  assert.ok(result.callGraph.methods.length >= 2, `should detect at least 2 methods (found ${result.callGraph.methods.length})`);
  assert.ok(result.callGraph.edges.length >= 1, "should detect at least 1 call edge");
});

test("integration: syscall-heavy contract analysis", () => {
  const script = new Uint8Array([
    0x57, 0x00, 0x01, // INITSLOT 0, 1
    0x78, // LDARG0
    0x41, 0xf8, 0x27, 0xec, 0x8c, // SYSCALL CheckWitness
    0x26, 0x08, // JMPIFNOT +8 (fail if not witness)
    0x0c, 0x01, 0x41, // PUSHDATA1 "A"
    0x41, 0xcf, 0xe7, 0x47, 0x96, // SYSCALL Log
    0x11, // PUSH1
    0x40, // RET
    0x10, // PUSH0
    0x40, // RET
  ]);
  
  const result = analyzeBytes(buildNef({ script }));
  
  assert.equal(result.callGraph.edges.length, 2, "should detect 2 syscalls");
  assert.ok(result.callGraph.edges.some(e => e.target.name?.includes("CheckWitness")), "should have CheckWitness");
  assert.ok(result.callGraph.edges.some(e => e.target.name?.includes("Log")), "should have Log");
});

test("integration: storage contract with loops", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1, 0
    0x10, // PUSH0
    0x70, // STLOC0 (i = 0)
    // Loop start
    0x68, // LDLOC0
    0x13, // PUSH3
    0xb5, // LT (i < 3) - correct opcode
    0x26, 0x0a, // JMPIFNOT +10 (exit)
    // Loop body: storage put
    0x68, // LDLOC0
    0x68, // LDLOC0 (key = value = i)
    0x41, 0xe6, 0x3f, 0x18, 0x84, // SYSCALL Storage.Put
    0x68, // LDLOC0
    0x11, // PUSH1
    0x9e, // ADD (i + 1)
    0x70, // STLOC0
    0x22, 0xf1, // JMP -15 (back to loop start)
    // Exit
    0x40, // RET
  ]);
  
  const result = decompileHighLevelBytes(buildNef({ script }));
  
  // The loop detection may produce while/for or goto depending on pattern
  assert.ok(
    result.highLevel.includes("while") || 
    result.highLevel.includes("for") || 
    result.highLevel.includes("goto"),
    "should detect control flow"
  );
  assert.match(result.highLevel, /syscall.*Storage\.Put/, "should have storage put");
});

test("integration: exception handling contract", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1, 0
    0x3b, 0x08, 0x00, // TRY catch=+8, finally=0
    0x11, // PUSH1
    0x70, // STLOC0
    0x3d, 0x05, // ENDTRY +5
    0x70, // STLOC0 (catch: store exception)
    0x10, // PUSH0
    0x70, // STLOC0 (error indicator)
    0x3d, 0x00, // ENDTRY +0
    0x68, // LDLOC0
    0x40, // RET
  ]);
  
  const result = decompileHighLevelBytes(buildNef({ script }));
  
  assert.match(result.highLevel, /try/, "should have try block");
  assert.match(result.highLevel, /catch/, "should have catch block");
});

test("integration: complex nested structures", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1, 0
    // Outer if
    0x11, // PUSH1
    0x26, 0x14, // JMPIFNOT +20 (skip outer)
    // Inner if
    0x11, // PUSH1
    0x26, 0x08, // JMPIFNOT +8 (skip inner)
    // Inner body
    0x11, // PUSH1
    0x70, // STLOC0
    0x22, 0x06, // JMP +6
    // Inner else
    0x12, // PUSH2
    0x70, // STLOC0
    // Loop inside outer if
    0x10, // PUSH0
    0x70, // STLOC0
    0x68, // LDLOC0
    0x13, // PUSH3
    0xb5, // LT (correct opcode)
    0x26, 0x06, // JMPIFNOT +6
    0x11, // PUSH1
    0x9e, // ADD
    0x70, // STLOC0
    0x22, 0xf8, // JMP -8
    // Exit outer
    0x40, // RET
  ]);
  
  const result = decompileHighLevelBytes(buildNef({ script }));
  
  // Should properly structure nested ifs and control flow
  assert.match(result.highLevel, /if/, "should have if");
  // Loop may be while/for or goto depending on detection
  assert.ok(
    result.highLevel.includes("while") || 
    result.highLevel.includes("for") || 
    result.highLevel.includes("goto"),
    "should have loop control flow"
  );
});

test("integration: method token calls", () => {
  const hash = new Uint8Array([
    0xcf, 0x76, 0xe2, 0x8b, 0xd0, 0x06, 0x2c, 0x4a, 0x47, 0x8e,
    0xe3, 0x55, 0x61, 0x01, 0x13, 0x19, 0xf3, 0xcf, 0xa4, 0xd2,
  ]);
  
  const script = new Uint8Array([
    0x11, // PUSH1
    0x12, // PUSH2
    0x37, 0x00, 0x00, // CALLT token 0
    0x40, // RET
  ]);
  
  const nef = buildNef({
    script,
    tokens: [{
      hash,
      method: "transfer",
      params: 2,
      hasReturn: true,
      callFlags: 0x0f,
    }],
  });
  
  const result = analyzeBytes(nef);
  
  assert.equal(result.callGraph.edges.length, 1, "should have one edge");
  assert.equal(result.callGraph.edges[0].opcode, "CALLT", "should be CALLT");
  assert.equal(result.callGraph.edges[0].target.method, "transfer", "should resolve method name");
});

test("integration: array and map operations", () => {
  const script = new Uint8Array([
    0x57, 0x02, 0x00, // INITSLOT 2, 0
    // Create array [1, 2]
    0x11, // PUSH1
    0x12, // PUSH2
    0xc0, // PACK (2 items)
    0x70, // STLOC0
    // Create map {}
    0xc8, // NEWMAP
    0x71, // STLOC1
    // Get array[0]
    0x68, // LDLOC0
    0x10, // PUSH0
    0xce, // PICKITEM
    0x40, // RET
  ]);
  
  const result = decompileHighLevelBytes(buildNef({ script }));
  
  assert.match(result.highLevel, /\[.*\]/, "should have array literal");
  // Map may be shown as {} or Map()
  assert.ok(
    result.highLevel.includes("Map") || result.highLevel.includes("{}"),
    "should have map constructor"
  );
  assert.match(result.highLevel, /\[/, "should have indexing");
});

test("integration: full analysis output structure validation", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x01, // INITSLOT 1, 1
    0x78, // LDARG0
    0x70, // STLOC0
    0x68, // LDLOC0
    0x40, // RET
  ]);
  
  const manifest = JSON.stringify({
    name: "Test",
    abi: {
      methods: [{
        name: "test",
        parameters: [{ name: "input", type: "Integer" }],
        returntype: "Integer",
        offset: 0,
      }],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  
  const result = analyzeBytes(buildNef({ script }), manifest);
  
  // Validate callGraph structure
  assert.ok(Array.isArray(result.callGraph.methods), "callGraph.methods should be array");
  assert.ok(Array.isArray(result.callGraph.edges), "callGraph.edges should be array");
  assert.ok(result.callGraph.methods.every(m => typeof m.offset === "number"), "methods should have offsets");
  assert.ok(result.callGraph.methods.every(m => typeof m.name === "string"), "methods should have names");
  
  // Validate xrefs structure
  assert.ok(Array.isArray(result.xrefs.methods), "xrefs.methods should be array");
  
  // Validate types structure
  assert.ok(Array.isArray(result.types.methods), "types.methods should be array");
  assert.ok(Array.isArray(result.types.statics), "types.statics should be array");
  
  // With manifest, should have typed arguments
  assert.equal(result.types.methods[0].arguments[0], "integer", "should infer integer type from manifest");
});

console.log("Integration tests loaded");
