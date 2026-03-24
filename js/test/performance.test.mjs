/**
 * Performance and Stress Tests for Neo Decompiler JS
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

function benchmark(name, fn, iterations = 100) {
  const times = [];
  
  // Warmup
  for (let i = 0; i < 10; i++) fn();
  
  // Measure
  for (let i = 0; i < iterations; i++) {
    const start = process.hrtime.bigint();
    fn();
    const end = process.hrtime.bigint();
    times.push(Number(end - start) / 1_000_000); // Convert to ms
  }
  
  const avg = times.reduce((a, b) => a + b, 0) / times.length;
  const min = Math.min(...times);
  const max = Math.max(...times);
  
  console.log(`  ${name}: avg=${avg.toFixed(3)}ms, min=${min.toFixed(3)}ms, max=${max.toFixed(3)}ms`);
  return { avg, min, max };
}

// ─── Performance Tests ──────────────────────────────────────────────────────

test("performance: small contract decompilation", () => {
  const nef = buildValidNef(new Uint8Array([
    0x57, 0x01, 0x01,
    0x78, 0x11, 0x9e, 0x70,
    0x68, 0x40,
  ]));
  
  const result = benchmark("small contract", () => {
    decompileHighLevelBytes(nef);
  }, 1000);
  
  assert.ok(result.avg < 10, `should complete in < 10ms on average (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: medium contract (1KB)", () => {
  const script = new Uint8Array(1000);
  script[0] = 0x57; // INITSLOT
  script[1] = 0x10; // 16 locals
  script[2] = 0x04; // 4 args
  
  // Fill with arithmetic operations
  for (let i = 3; i < 995; i += 3) {
    script[i] = 0x11; // PUSH1
    script[i + 1] = 0x12; // PUSH2
    script[i + 2] = 0x9e; // ADD
  }
  script[999] = 0x40; // RET
  
  const nef = buildValidNef(script);
  
  const result = benchmark("1KB contract", () => {
    decompileHighLevelBytes(nef);
  }, 100);
  
  assert.ok(result.avg < 50, `should complete in < 50ms (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: large contract (10KB)", () => {
  const script = new Uint8Array(10000);
  script[0] = 0x57;
  script[1] = 0x00;
  script[2] = 0x00;
  
  for (let i = 3; i < 9995; i++) {
    script[i] = 0x21; // NOP
  }
  script[9999] = 0x40;
  
  const nef = buildValidNef(script);
  
  const result = benchmark("10KB contract", () => {
    decompileHighLevelBytes(nef);
  }, 10);
  
  assert.ok(result.avg < 500, `should complete in < 500ms (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: complex control flow", () => {
  const script = [];
  script.push(0x57, 0x10, 0x04); // INITSLOT 16 locals, 4 args
  
  // Create many nested ifs
  for (let i = 0; i < 50; i++) {
    script.push(0x11); // PUSH1
    script.push(0x26, 0x05); // JMPIFNOT +5
    script.push(0x11, 0x12, 0x9e); // PUSH1 PUSH2 ADD
    script.push(0x22, 0x03); // JMP +3
    script.push(0x12, 0x13, 0x9e); // PUSH2 PUSH3 ADD
  }
  
  // Create loops
  for (let i = 0; i < 20; i++) {
    script.push(0x11); // PUSH1 (condition)
    script.push(0x26, 0x08); // JMPIFNOT +8 (exit)
    script.push(0x11, 0x12, 0x9e); // body
    script.push(0x22, 0xf7); // JMP -9 (loop)
  }
  
  script.push(0x40); // RET
  
  const nef = buildValidNef(new Uint8Array(script));
  
  const result = benchmark("complex control flow", () => {
    decompileHighLevelBytes(nef);
  }, 50);
  
  assert.ok(result.avg < 100, `should complete in < 100ms (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: full analysis pipeline", () => {
  const script = new Uint8Array(5000);
  script[0] = 0x57;
  script[1] = 0x10;
  script[2] = 0x04;
  
  for (let i = 3; i < 4995; i += 4) {
    script[i] = 0x68; // LDLOC0
    script[i + 1] = 0x11; // PUSH1
    script[i + 2] = 0x9e; // ADD
    script[i + 3] = 0x70; // STLOC0
  }
  script[4999] = 0x40;
  
  const nef = buildValidNef(script);
  
  const result = benchmark("full analysis", () => {
    analyzeBytes(nef);
  }, 20);
  
  assert.ok(result.avg < 1000, `should complete in < 1000ms (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: disassembly only", () => {
  const script = new Uint8Array(10000).fill(0x21);
  script[0] = 0x57;
  script[1] = 0x00;
  script[2] = 0x00;
  script[9999] = 0x40;
  
  const result = benchmark("disassembly 10KB", () => {
    disassembleScript(script);
  }, 100);
  
  assert.ok(result.avg < 10, `should complete in < 10ms (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: syscall-heavy contract", () => {
  const script = [];
  script.push(0x57, 0x00, 0x00);
  
  // Many syscalls
  for (let i = 0; i < 100; i++) {
    script.push(0x41, 0xb7, 0xc3, 0x88, 0x03); // GetTime
    script.push(0x41, 0xcf, 0xe7, 0x47, 0x96); // Log
  }
  script.push(0x40);
  
  const nef = buildValidNef(new Uint8Array(script));
  
  const result = benchmark("syscall-heavy", () => {
    analyzeBytes(nef);
  }, 50);
  
  assert.ok(result.avg < 50, `should complete in < 50ms (was ${result.avg.toFixed(3)}ms)`);
});

test("performance: many method tokens", () => {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64));
  data.push(0); // source
  data.push(0); // reserved
  
  // 100 tokens
  writeVarint(data, 100);
  for (let i = 0; i < 100; i++) {
    data.push(...new Uint8Array(20).fill(i)); // hash
    const methodName = `m${i}`; // Short name for consistency
    writeVarint(data, methodName.length); // method name length
    data.push(...Buffer.from(methodName));
    data.push(i % 16, 0); // params
    data.push(i % 2); // has return
    data.push(0x0f); // call flags
  }
  
  data.push(0, 0); // reserved word
  data.push(2, 0x11, 0x40); // script
  data.push(...computeChecksum(data));
  
  const nef = new Uint8Array(data);
  
  const result = benchmark("100 method tokens", () => {
    parseNef(nef);
  }, 100);
  
  assert.ok(result.avg < 10, `should complete in < 10ms (was ${result.avg.toFixed(3)}ms)`);
});

test("stress: rapid sequential processing", () => {
  const nef = buildValidNef(new Uint8Array([0x57, 0x01, 0x01, 0x78, 0x11, 0x9e, 0x70, 0x68, 0x40]));
  
  const start = Date.now();
  for (let i = 0; i < 10000; i++) {
    decompileHighLevelBytes(nef);
  }
  const elapsed = Date.now() - start;
  
  console.log(`  10000 iterations: ${elapsed}ms (${(elapsed/10000).toFixed(3)}ms avg)`);
  assert.ok(elapsed < 30000, `should complete in < 30s (was ${elapsed}ms)`);
});

test("stress: memory stability over many iterations", () => {
  const nef = buildValidNef(new Uint8Array(5000).fill(0x21));
  
  const before = process.memoryUsage();
  
  for (let i = 0; i < 1000; i++) {
    analyzeBytes(nef);
  }
  
  // Force GC if available
  if (global.gc) global.gc();
  
  const after = process.memoryUsage();
  const heapGrowth = (after.heapUsed - before.heapUsed) / 1024 / 1024;
  
  console.log(`  Heap growth: ${heapGrowth.toFixed(2)}MB`);
  assert.ok(heapGrowth < 100, `heap should not grow unboundedly (${heapGrowth.toFixed(2)}MB)`);
});

test("stress: concurrent processing", async () => {
  const scripts = [
    buildValidNef(new Uint8Array([0x11, 0x40])),
    buildValidNef(new Uint8Array([0x12, 0x40])),
    buildValidNef(new Uint8Array([0x13, 0x40])),
  ];
  
  const promises = [];
  for (let i = 0; i < 100; i++) {
    promises.push(new Promise((resolve) => {
      const nef = scripts[i % 3];
      const result = analyzeBytes(nef);
      resolve(result);
    }));
  }
  
  const start = Date.now();
  const results = await Promise.all(promises);
  const elapsed = Date.now() - start;
  
  console.log(`  100 concurrent: ${elapsed}ms`);
  assert.equal(results.length, 100);
});

test("stress: increasingly complex contracts", () => {
  const sizes = [100, 500, 1000, 5000, 10000];
  
  for (const size of sizes) {
    const script = new Uint8Array(size);
    script[0] = 0x57;
    script[1] = 0x00;
    script[2] = 0x00;
    script.fill(0x21, 3, size - 1);
    script[size - 1] = 0x40;
    
    const nef = buildValidNef(script);
    
    const start = Date.now();
    decompileHighLevelBytes(nef);
    const elapsed = Date.now() - start;
    
    console.log(`  ${size} bytes: ${elapsed}ms`);
    assert.ok(elapsed < 1000, `${size} bytes should complete in < 1000ms`);
  }
});

test("stress: deep nesting levels", () => {
  for (const depth of [10, 50, 100, 200]) {
    const script = [0x57, 0x00, 0x00];
    
    for (let i = 0; i < depth; i++) {
      script.push(0x11); // PUSH1
      script.push(0x26, 0x05); // JMPIFNOT +5
      script.push(0x21); // NOP
    }
    for (let i = 0; i < depth; i++) {
      script.push(0x40); // RET for each level (simplified)
    }
    
    const nef = buildValidNef(new Uint8Array(script));
    
    const start = Date.now();
    try {
      decompileHighLevelBytes(nef);
    } catch (e) {
      // Deep nesting may fail but should not crash
    }
    const elapsed = Date.now() - start;
    
    console.log(`  nesting depth ${depth}: ${elapsed}ms`);
    assert.ok(elapsed < 5000, `depth ${depth} should complete in < 5s`);
  }
});

console.log("Performance tests loaded");
