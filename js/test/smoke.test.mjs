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

test("SMOKE: oversized manifest is rejected", () => {
  const padding = "x".repeat(0x10000);
  const oversized = JSON.stringify({ name: padding, abi: { methods: [], events: [] } });
  assert.throws(
    () => parseManifest(oversized),
    (err) => err.details.code === "FileTooLarge" && err.details.max === 0xffff,
  );
});

test("SMOKE: strict mode rejects non-canonical wildcards", () => {
  const badContract = JSON.stringify({
    name: "C",
    abi: { methods: [], events: [] },
    permissions: [{ contract: "all", methods: "*" }],
  });
  assert.throws(
    () => parseManifest(badContract, { strict: true }),
    (err) => err.details.code === "Validation",
  );
  assert.doesNotThrow(() => parseManifest(badContract));

  const badMethods = JSON.stringify({
    name: "C",
    abi: { methods: [], events: [] },
    permissions: [{ contract: "*", methods: "ALL" }],
  });
  assert.throws(
    () => parseManifest(badMethods, { strict: true }),
    (err) => err.details.code === "Validation",
  );

  const badTrusts = JSON.stringify({
    name: "C",
    abi: { methods: [], events: [] },
    trusts: "all",
  });
  assert.throws(
    () => parseManifest(badTrusts, { strict: true }),
    (err) => err.details.code === "Validation",
  );
});

test("SMOKE: manifest method offset/safe strict typing (matches Rust)", () => {
  // offset: Rust uses Option<i32>, rejects strings/objects.
  assert.throws(
    () =>
      parseManifest(
        JSON.stringify({
          name: "C",
          abi: {
            methods: [
              { name: "m", parameters: [], returntype: "Void", offset: "5" },
            ],
          },
        }),
      ),
    (err) => err.details.code === "InvalidType" && err.details.path.endsWith(".offset"),
  );

  // safe: Rust requires bool; JS no longer coerces truthy values.
  assert.throws(
    () =>
      parseManifest(
        JSON.stringify({
          name: "C",
          abi: {
            methods: [
              { name: "m", parameters: [], returntype: "Void", safe: 1 },
            ],
          },
        }),
      ),
    (err) => err.details.code === "InvalidType" && err.details.path.endsWith(".safe"),
  );

  // Negative offsets still treated as "no offset" (Neo N3 -1 convention) — both match.
  assert.doesNotThrow(() =>
    parseManifest(
      JSON.stringify({
        name: "C",
        abi: {
          methods: [{ name: "m", parameters: [], returntype: "Void", offset: -1 }],
        },
      }),
    ),
  );
});

test("SMOKE: manifest abi and feature/standards strict typing (matches Rust)", () => {
  // abi is required
  assert.throws(
    () => parseManifest(JSON.stringify({ name: "C" })),
    (err) => err.details.code === "MissingField" && err.details.path === "abi",
  );
  // abi cannot be null
  assert.throws(
    () => parseManifest(JSON.stringify({ name: "C", abi: null })),
    (err) => err.details.code === "MissingField" && err.details.path === "abi",
  );
  // features.storage must be boolean (Rust serde rejects coercion)
  assert.throws(
    () =>
      parseManifest(
        JSON.stringify({
          name: "C",
          abi: { methods: [], events: [] },
          features: { storage: "yes" },
        }),
      ),
    (err) => err.details.code === "InvalidType" && err.details.path === "features.storage",
  );
  // supportedstandards must be array
  assert.throws(
    () =>
      parseManifest(
        JSON.stringify({
          name: "C",
          abi: { methods: [], events: [] },
          supportedstandards: "NEP-17",
        }),
      ),
    (err) => err.details.code === "InvalidType" && err.details.path === "supportedstandards",
  );
});

test("SMOKE: manifest top-level name and groups required (matches Rust)", () => {
  // Top-level `name` is required: Rust ContractManifest.name is `String` (no default).
  assert.throws(
    () => parseManifest(JSON.stringify({ abi: { methods: [], events: [] } })),
    (err) => err.details.code === "MissingField" && err.details.path === "name",
  );

  // Groups require both pubkey and signature: Rust ManifestGroup struct mandates both.
  assert.throws(
    () =>
      parseManifest(
        JSON.stringify({
          name: "C",
          groups: [{ pubkey: "k" }],
          abi: { methods: [], events: [] },
        }),
      ),
    (err) =>
      err.details.code === "MissingField" &&
      err.details.path === "groups[0].signature",
  );
  assert.throws(
    () =>
      parseManifest(
        JSON.stringify({
          name: "C",
          groups: [{ signature: "s" }],
          abi: { methods: [], events: [] },
        }),
      ),
    (err) =>
      err.details.code === "MissingField" &&
      err.details.path === "groups[0].pubkey",
  );

  // Complete groups should still parse.
  assert.doesNotThrow(() =>
    parseManifest(
      JSON.stringify({
        name: "C",
        groups: [{ pubkey: "k", signature: "s" }],
        abi: { methods: [], events: [] },
      }),
    ),
  );
});

test("SMOKE: manifest required-field strictness matches Rust", () => {
  const cases = [
    {
      json: { name: "C", abi: { methods: [{ parameters: [], returntype: "Void" }] } },
      pathSuffix: ".name",
    },
    {
      json: { name: "C", abi: { methods: [{ name: "m", parameters: [{ type: "Integer" }], returntype: "Void" }] } },
      pathSuffix: ".name",
    },
    {
      json: { name: "C", abi: { methods: [{ name: "m", parameters: [{ name: "x" }], returntype: "Void" }] } },
      pathSuffix: ".type",
    },
    {
      json: { name: "C", abi: { events: [{ parameters: [] }] } },
      pathSuffix: ".name",
    },
    {
      json: { name: "C", abi: { events: [{ name: "E", parameters: [{ type: "Integer" }] }] } },
      pathSuffix: ".name",
    },
  ];
  for (const { json, pathSuffix } of cases) {
    assert.throws(
      () => parseManifest(JSON.stringify(json)),
      (err) =>
        err.details.code === "MissingField" && err.details.path.endsWith(pathSuffix),
      `expected rejection for ${JSON.stringify(json)} ending in ${pathSuffix}`,
    );
  }
});

test("SMOKE: manifest method missing returntype is rejected", () => {
  // Matches Rust spec: ManifestMethod.return_type has no #[serde(default)],
  // so missing returntype must error consistently in both implementations.
  const missing = JSON.stringify({
    name: "test",
    abi: { methods: [{ name: "m", parameters: [] }] },
  });
  assert.throws(
    () => parseManifest(missing),
    (err) =>
      err.details.code === "MissingField" &&
      err.details.path === "abi.methods[0].returntype",
  );

  const nullValue = JSON.stringify({
    name: "C",
    abi: { methods: [{ name: "m", parameters: [], returntype: null }] },
  });
  assert.throws(
    () => parseManifest(nullValue),
    (err) => err.details.code === "MissingField",
  );
});

test("SMOKE: strict mode accepts canonical wildcards and concrete entries", () => {
  const canonical = JSON.stringify({
    name: "C",
    abi: { methods: [], events: [] },
    permissions: [
      { contract: "*", methods: "*" },
      { contract: { hash: "0x1234567890abcdef1234567890abcdef12345678" }, methods: ["transfer"] },
    ],
    trusts: "*",
  });
  assert.doesNotThrow(() => parseManifest(canonical, { strict: true }));

  const arrayTrusts = JSON.stringify({
    name: "C",
    abi: { methods: [], events: [] },
    trusts: [],
  });
  assert.doesNotThrow(() => parseManifest(arrayTrusts, { strict: true }));
});

console.log("Smoke tests loaded");
