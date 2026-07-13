import assert from "node:assert/strict";
import test from "node:test";

import { identifyPatterns, renderCSharpContract } from "../src/index.js";

function nef(compiler = "", source = "") {
  return { header: { compiler, source } };
}

test("pattern analysis treats declared NEP standards as authoritative", () => {
  const info = identifyPatterns(
    nef("Neo.Compiler.CSharp 3"),
    [],
    {
      supportedStandards: ["NEP-17"],
      abi: { methods: [], events: [] },
    },
  );
  assert.deepEqual(info.standards, ["NEP-17"]);
  assert.equal(info.language, "C#");
  assert.equal(info.confidence, "high");
});

test("pattern analysis keeps weak source metadata explainable", () => {
  const info = identifyPatterns(nef("", "contract.py"), [], null);
  assert.deepEqual(info.standards, []);
  assert.equal(info.language, "Python");
  assert.equal(info.confidence, "medium");
});

test("pattern analysis identifies wildcard call permissions", () => {
  const info = identifyPatterns(
    nef(),
    [],
    {
      supportedStandards: [],
      permissions: [{ contract: "*", methods: "*" }],
      abi: { methods: [], events: [] },
    },
  );
  assert.deepEqual(info.patterns, ["call_permissions", "wildcard_permissions"]);
  assert.equal(info.confidence, "medium");
});

test("C# rendering lowers known syscalls but preserves unknown ones", () => {
  const source = [
    "contract Token {",
    "fn get() -> any {",
    '    let context = syscall("System.Storage.GetContext");',
    '    return syscall("System.Storage.Get", context, key);',
    '    let raw = syscall("System.Custom.Unknown", key);',
    "}",
    "}",
  ].join("\n");
  const csharp = renderCSharpContract(source);
  assert.match(csharp, /Storage\.CurrentContext/);
  assert.match(csharp, /Storage\.Get\(context, key\)/);
  assert.match(csharp, /syscall\("System\.Custom\.Unknown", key\)/);
});
