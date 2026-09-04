import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";

import { createNeoDecompilerClient } from "../dist/index.js";

test("client maps camelCase info options to the wasm ABI", () => {
  let receivedOptions;
  const client = createNeoDecompilerClient({
    infoReport(_nefBytes, options) {
      receivedOptions = options;
      return { compiler: "test" };
    },
    disasmReport() {
      return { instructions: [], warnings: [] };
    },
    decompileReport() {
      return {
        script_hash_le: "",
        script_hash_be: "",
        csharp: "",
        high_level: "",
        pseudocode: "",
        instructions: [],
        method_tokens: [],
        manifest: null,
        analysis: { call_graph: { methods: [], edges: [] }, method_contracts: { methods: [] }, xrefs: { methods: [] }, types: { methods: [], statics: [] } },
        warnings: [],
      };
    },
    initPanicHook() {},
  });

  const nefBytes = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
  const result = client.infoReport(nefBytes, {
    manifestJson: "{\"name\":\"demo\"}",
    strictManifest: true,
  });

  assert.deepEqual(receivedOptions, {
    manifest_json: "{\"name\":\"demo\"}",
    strict_manifest: true,
  });
  assert.equal(result.compiler, "test");
});

test("client maps decompile options to the wasm ABI", () => {
  let receivedOptions;
  const client = createNeoDecompilerClient({
    infoReport() {
      return { compiler: "test" };
    },
    disasmReport() {
      return { instructions: [], warnings: [] };
    },
    decompileReport(_nefBytes, options) {
      receivedOptions = options;
      return {
        script_hash_le: "",
        script_hash_be: "",
        csharp: "",
        high_level: "",
        pseudocode: "",
        instructions: [],
        method_tokens: [],
        manifest: null,
        analysis: { call_graph: { methods: [], edges: [] }, method_contracts: { methods: [] }, xrefs: { methods: [] }, types: { methods: [], statics: [] } },
        warnings: [],
      };
    },
    initPanicHook() {},
  });

  client.decompileReport(new Uint8Array([1]), {
    manifestJson: "{\"name\":\"demo\"}",
    strictManifest: true,
    failOnUnknownOpcodes: true,
    inlineSingleUseTemps: true,
    typedDeclarations: false,
    outputFormat: "highLevel",
  });

  assert.deepEqual(receivedOptions, {
    manifest_json: "{\"name\":\"demo\"}",
    strict_manifest: true,
    fail_on_unknown_opcodes: true,
    inline_single_use_temps: true,
    typed_declarations: false,
    output_format: "highLevel",
  });
});

test("client normalizes safe wasm integers and retains exact wide operands", () => {
  const raw = () => ({
    script_length: 2n,
    instructions: [{ offset: 0n, operand_value: { type: "I64", value: 9223372036854775807n } }],
    analysis: { call_graph: { methods: [{ offset: 0n }] } },
    limits: [9007199254740991n, -9007199254740991n, 9007199254740992n, -9007199254740992n],
  });
  const client = createNeoDecompilerClient({
    infoReport: raw,
    disasmReport: raw,
    decompileReport: raw,
    initPanicHook() {},
  });
  for (const method of ["infoReport", "disasmReport", "decompileReport"]) {
    const report = client[method](new Uint8Array());
    assert.equal(report.script_length, 2);
    assert.equal(report.instructions[0].offset, 0);
    assert.equal(report.analysis.call_graph.methods[0].offset, 0);
    assert.equal(report.instructions[0].operand_value.value, 9223372036854775807n);
    assert.deepEqual(report.limits, [9007199254740991, -9007199254740991, 9007199254740992n, -9007199254740992n]);
  }
});

test("client preserves nested map data and own __proto__ keys without changing prototypes", () => {
  const client = createNeoDecompilerClient({
    infoReport() {
      return { manifest: { extra: new Map([
        ["__proto__", new Map([["marker", "metadata"]])],
        ["nested", new Map([["safe", 7n], ["wide", 9223372036854775807n]])],
      ]) } };
    },
  });
  const extra = client.infoReport(new Uint8Array()).manifest.extra;
  assert.equal(Object.getPrototypeOf(extra), Object.prototype);
  assert.equal(Object.hasOwn(extra, "__proto__"), true);
  assert.deepEqual(extra.__proto__, { marker: "metadata" });
  assert.equal(extra.marker, undefined);
  assert.deepEqual(extra.nested, { safe: 7, wide: 9223372036854775807n });
});

test("package version stays in sync with Cargo.toml", () => {
  execFileSync("node", ["./scripts/sync-version.mjs", "--check"], {
    cwd: new URL("../", import.meta.url),
    stdio: "pipe",
  });
});

test("package metadata is configured for public provenance publishing", () => {
  const packageJson = JSON.parse(
    readFileSync(new URL("../package.json", import.meta.url), "utf8"),
  );

  assert.equal(packageJson.publishConfig?.provenance, true);
  assert.equal(packageJson.publishConfig?.access, "public");
});
