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
        analysis: { call_graph: { methods: [], edges: [] }, xrefs: { methods: [] }, types: { methods: [], statics: [] } },
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
        analysis: { call_graph: { methods: [], edges: [] }, xrefs: { methods: [] }, types: { methods: [], statics: [] } },
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
    outputFormat: "highLevel",
  });

  assert.deepEqual(receivedOptions, {
    manifest_json: "{\"name\":\"demo\"}",
    strict_manifest: true,
    fail_on_unknown_opcodes: true,
    inline_single_use_temps: true,
    output_format: "highLevel",
  });
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
