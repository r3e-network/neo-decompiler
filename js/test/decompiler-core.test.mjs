import assert from "node:assert/strict";
import test from "node:test";

import {
  decompileBytes,
  decompileBytesWithManifest,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
  disassembleScript,
  parseManifest,
  parseNef,
} from "../src/index.js";
import {
  SAMPLE_MANIFEST,
  buildNefFromScript,
  buildSampleNef,
  computeChecksum,
} from "./decompiler-fixtures.mjs";

test("parses a sample NEF and exposes method tokens", () => {
  const nef = parseNef(buildSampleNef());
  assert.equal(nef.header.compiler, "test");
  assert.equal(nef.methodTokens.length, 1);
  assert.equal(nef.methodTokens[0].method, "Transfer");
});

test("disassembles the sample script into instructions", () => {
  const nef = parseNef(buildSampleNef());
  const result = disassembleScript(nef.script);
  assert.equal(result.instructions[0].opcode.mnemonic, "PUSH0");
  assert.equal(result.instructions.at(-1).opcode.mnemonic, "RET");
});

test("decompiles the sample NEF into pseudocode", () => {
  const result = decompileBytes(buildSampleNef());
  assert.match(result.pseudocode, /0000: PUSH0/);
  assert.match(result.pseudocode, /0002: ADD/);
});

test("rejects a NEF with an invalid checksum", () => {
  const nef = buildSampleNef();
  nef[nef.length - 1] ^= 0xff;
  assert.throws(() => parseNef(nef), /checksum mismatch/);
});

test("supports tolerant and strict unknown opcode handling", () => {
  const nef = buildSampleNef();
  const corrupted = new Uint8Array(nef);
  const scriptOffset = corrupted.length - 8;
  corrupted[scriptOffset] = 0xff;
  const checksum = computeChecksum(corrupted.subarray(0, corrupted.length - 4));
  corrupted.set(checksum, corrupted.length - 4);

  const parsed = parseNef(corrupted);
  const tolerant = disassembleScript(parsed.script);
  assert.equal(tolerant.instructions[0].opcode.mnemonic, "UNKNOWN");
  assert.equal(tolerant.warnings.length, 1);
  assert.match(decompileBytes(corrupted).pseudocode, /UNKNOWN_0xFF/);

  assert.throws(
    () => disassembleScript(parsed.script, { failOnUnknownOpcodes: true }),
    /unknown opcode 0xFF/,
  );
});

test("notes tolerant unknown opcodes in high-level output", () => {
  const result = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0xff, 0x40])));
  assert.match(result.highLevel, /UNKNOWN_0xFF \(not yet translated\)/);
});

test("high-level header surfaces NEF compiler field", () => {
  // The NEF header carries `compiler` (a short string identifying
  // the compiler that produced the bytecode) and `source` (often a
  // repo URL or commit hash). Both are visible via `info` but were
  // dropped from the high-level header — readers had to run a
  // separate command to learn what produced the contract. Surface
  // them as `// compiler: ...` / `// source: ...` lines below the
  // script hash. Empty fields are silently skipped (the test
  // harness writes "test" as compiler and an empty source).
  const result = decompileHighLevelBytes(buildSampleNef());
  assert.match(result.highLevel, /\/\/ compiler: test/);
  assert.doesNotMatch(result.highLevel, /\/\/ source:/);
});

test("parses a manifest and exposes ABI methods", () => {
  const manifest = parseManifest(SAMPLE_MANIFEST);
  assert.equal(manifest.name, "SampleToken");
  assert.equal(manifest.abi.methods.length, 1);
  assert.equal(manifest.abi.methods[0].name, "symbol");
  assert.equal(manifest.abi.methods[0].offset, 0);
});

test("groups pseudocode by manifest methods when offsets are present", () => {
  const result = decompileBytesWithManifest(buildSampleNef(), SAMPLE_MANIFEST);
  assert.match(result.groupedPseudocode, /contract SampleToken \{/);
  assert.match(result.groupedPseudocode, /fn symbol\(\) \{/);
  assert.match(result.groupedPseudocode, /0000: PUSH0/);
  assert.match(result.groupedPseudocode, /0003: RET/);
});

test("emits a synthetic script_entry when manifest offsets skip bytecode entry", () => {
  const manifest = JSON.stringify({
    ...JSON.parse(SAMPLE_MANIFEST),
    abi: {
      methods: [
        {
          name: "symbol",
          parameters: [],
          returntype: "String",
          offset: 2,
          safe: true,
        },
      ],
      events: [],
    },
  });

  const result = decompileBytesWithManifest(buildSampleNef(), manifest);
  assert.match(result.groupedPseudocode, /fn script_entry\(\) \{/);
  assert.match(result.groupedPseudocode, /fn symbol\(\) \{/);
  assert.match(result.groupedPseudocode, /0000: PUSH0/);
  assert.match(result.groupedPseudocode, /0002: ADD/);
});

test("uses the first manifest method as the entry signature when offsets are missing", () => {
  const manifest = JSON.stringify({
    name: "OffsetMissing",
    abi: {
      methods: [
        {
          name: "main",
          parameters: [],
          returntype: "Integer",
        },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(buildSampleNef(), manifest);
  assert.match(result.highLevel, /fn main\(\) -> int \{/);
  assert.doesNotMatch(result.highLevel, /fn script_entry\(\)/);
});

test("renders manifest entry signatures with sanitized parameter names and pseudo-types", () => {
  const manifest = JSON.stringify({
    name: "Parametrized",
    abi: {
      methods: [
        {
          name: "deploy-contract",
          parameters: [
            { name: "owner-name", type: "Hash160" },
            { name: "amount", type: "Integer" },
          ],
          returntype: "Void",
          offset: 0,
          safe: false,
        },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });

  const result = decompileHighLevelBytesWithManifest(buildSampleNef(), manifest);
  assert.match(
    result.highLevel,
    /fn deploy_contract\(owner_name: hash160, amount: int\) \{/,
  );
});

test("manifest summary: a permission with no methods field renders methods=* (Rust parity)", () => {
  // Neo N3 defaults an absent `methods` to the `*` wildcard; JS previously
  // rendered the literal `methods=undefined`.
  const manifest = JSON.stringify({
    name: "PermDefault",
    abi: { methods: [], events: [] },
    permissions: [{ contract: "*" }],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(buildSampleNef(), manifest);
  assert.match(result.highLevel, /contract=\* methods=\*/);
  assert.doesNotMatch(result.highLevel, /methods=undefined/);
});

test("manifest summary: features keys render sorted (Rust BTreeMap parity)", () => {
  const manifest = JSON.stringify({
    name: "FeatureOrder",
    abi: { methods: [], events: [] },
    features: { storage: false, payable: false },
    permissions: [],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(buildSampleNef(), manifest);
  assert.match(result.highLevel, /features \{/);
  assert.ok(
    result.highLevel.indexOf("payable = false;") < result.highLevel.indexOf("storage = false;"),
    "features keys must be sorted (payable before storage)",
  );
});

test("manifest summary: extra keys render sorted (Rust BTreeMap parity)", () => {
  const manifest = JSON.stringify({
    name: "ExtraOrder",
    abi: { methods: [], events: [] },
    permissions: [],
    trusts: "*",
    extra: { Version: "1.0", Author: "alice", Description: "demo" },
  });
  const out = decompileHighLevelBytesWithManifest(buildSampleNef(), manifest).highLevel;
  assert.ok(out.indexOf("// Author:") < out.indexOf("// Description:"));
  assert.ok(out.indexOf("// Description:") < out.indexOf("// Version:"));
});

test("manifest summary: a method offset past the script gets an offset-bearing placeholder", () => {
  const manifest = JSON.stringify({
    name: "GhostOffset",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
        { name: "ghost", parameters: [], returntype: "Integer", offset: 9999 },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(buildSampleNef(), manifest);
  assert.match(
    result.highLevel,
    /\/\/ no instructions decoded for manifest method at offset 0x270F/,
  );
});

test("keeps manifest helper methods separate from the entry range", () => {
  const script = new Uint8Array([0x11, 0x40, 0x12, 0x40]);
  const manifest = JSON.stringify({
    name: "Multi",
    abi: {
      methods: [
        { name: "entry", parameters: [], returntype: "Integer", offset: 0 },
        { name: "other", parameters: [], returntype: "Integer", offset: 2 },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(buildNefFromScript(script), manifest);
  assert.match(result.highLevel, /fn entry\(\) -> int \{/);
  assert.match(result.highLevel, /fn other\(\) -> int \{/);
  assert.match(result.highLevel, /return 1;/);
  assert.match(result.highLevel, /return 2;/);
});
