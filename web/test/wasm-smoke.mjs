// This suite deliberately lives outside *.test.mjs: the ordinary unit suite
// needs only TypeScript, while release/CI explicitly require real built WASM.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

import { init } from "../dist/index.js";
import { stringifyReport } from "../report-format.js";

const wasmBytes = readFileSync(new URL("../dist/pkg/neo_decompiler_bg.wasm", import.meta.url));
const client = await init(wasmBytes);

function nef(script) {
  assert.ok(script.length < 0xfd);
  const compiler = Buffer.alloc(64);
  compiler.write("wasm-smoke");
  const payload = Buffer.concat([
    Buffer.from("NEF3"), compiler,
    Buffer.from([0, 0, 0, 0, 0, script.length]), Buffer.from(script),
  ]);
  const firstHash = createHash("sha256").update(payload).digest();
  const checksum = createHash("sha256").update(firstHash).digest().subarray(0, 4);
  return new Uint8Array(Buffer.concat([payload, checksum]));
}

test("built WebAssembly initializes and runs all report APIs", () => {
  const input = nef([0x11, 0x40]); // PUSH1; RET
  const info = client.infoReport(input);
  assert.equal(info.compiler, "wasm-smoke");
  assert.equal(info.script_length, 2);
  const disasm = client.disasmReport(input);
  assert.deepEqual(disasm.instructions.map(({ opcode }) => opcode), ["PUSH1", "RET"]);
  const decompile = client.decompileReport(input, { outputFormat: "all" });
  assert.equal(decompile.script_hash_be, info.script_hash_be);
  assert.match(decompile.csharp, /class /u);
  assert.ok(decompile.high_level.length > 0);
  assert.ok(decompile.pseudocode.length > 0);
});

test("built WebAssembly retains wide integer operands and the demo can display them", () => {
  const operand = Buffer.alloc(8);
  operand.writeBigInt64LE(9223372036854775807n);
  const input = nef([0x03, ...operand, 0x40]); // PUSHINT64; RET
  const report = client.disasmReport(input);
  assert.equal(report.instructions[0].operand_value.value, 9223372036854775807n);
  assert.match(stringifyReport(report), /"9223372036854775807"/u);
});

test("built WebAssembly preserves manifest maps, wide integers, and prototype-like keys", () => {
  const manifestJson = `{
    "name":"Smoke", "groups":[], "supportedstandards":[],
    "features":{"storage":true}, "abi":{"methods":[],"events":[]},
    "permissions":[], "trusts":[],
    "extra":{"Author":"Smoke","nested":{"safe":7,"wide":9223372036854775807},
      "__proto__":{"marker":"metadata"},"constructor":{"prototype":{"marker":"data"}}}
  }`;
  for (const report of [
    client.infoReport(nef([0x40]), { manifestJson }),
    client.decompileReport(nef([0x40]), { manifestJson }),
  ]) {
    assert.deepEqual(report.manifest.features, { storage: true });
    const extra = report.manifest.extra;
    assert.equal(Object.getPrototypeOf(extra), Object.prototype);
    assert.equal(Object.hasOwn(extra, "__proto__"), true);
    assert.deepEqual(extra.__proto__, { marker: "metadata" });
    assert.equal(extra.marker, undefined);
    assert.deepEqual(extra.nested, { safe: 7, wide: 9223372036854775807n });
    const displayed = JSON.parse(stringifyReport(report));
    assert.equal(displayed.manifest.extra.Author, "Smoke");
    assert.equal(displayed.manifest.extra.nested.wide, "9223372036854775807");
    assert.equal(Object.hasOwn(displayed.manifest.extra, "__proto__"), true);
  }
});

test("built WebAssembly reports bad input without poisoning subsequent analysis", () => {
  assert.throws(() => client.infoReport(new Uint8Array([1, 2, 3])));
  assert.equal(client.infoReport(nef([0x40])).script_length, 1);
});
