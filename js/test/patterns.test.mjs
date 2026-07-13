import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  decompileBytes,
  decompileBytesWithManifest,
  identifyPatterns,
  renderCSharpContract,
} from "../src/index.js";

function nef(compiler = "", source = "") {
  return { header: { compiler, source } };
}

function buildNef(script = [0x40], compiler = "Neo.Compiler.CSharp") {
  const data = [...Buffer.from("NEF3")];
  const compilerBytes = new Uint8Array(64);
  compilerBytes.set(Buffer.from(compiler));
  data.push(...compilerBytes, 0, 0, 0, 0, 0, script.length, ...script);
  const first = createHash("sha256").update(Buffer.from(data)).digest();
  const second = createHash("sha256").update(first).digest();
  data.push(...second.subarray(0, 4));
  return Uint8Array.from(data);
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

test("basic JS decompile APIs expose the same pattern summary", () => {
  const bytes = buildNef();
  const basic = decompileBytes(bytes);
  const withManifest = decompileBytesWithManifest(bytes, {
    name: "Token",
    supportedstandards: ["NEP-17"],
    abi: { methods: [], events: [] },
  });
  assert.equal(basic.patterns.language, "C#");
  assert.deepEqual(withManifest.patterns.standards, ["NEP-17"]);
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

test("C# rendering preserves ABI events as framework events", () => {
  const source = [
    "contract Token {",
    "    event Transfer(from: hash160, amount: int);",
    '    event class(); // manifest "class"',
    "}",
  ].join("\n");
  const csharp = renderCSharpContract(source);
  assert.match(csharp, /public static event Action<UInt160, BigInteger> Transfer;/);
  assert.match(csharp, /\[DisplayName\("class"\)\]/);
  assert.match(csharp, /public static event Action @class;/);
});

test("C# rendering emits manifest class attributes", () => {
  const csharp = renderCSharpContract(
    "contract Token {\n}",
    {
      supportedStandards: ["NEP-17", "NEP-11"],
      extra: { Email: "owner@example.com", Version: 2, Nested: { ignored: true } },
    },
  );
  assert.match(csharp, /\[SupportedStandards\("NEP-17", "NEP-11"\)\]/);
  assert.match(csharp, /\[ManifestExtra\("Email", "owner@example.com"\)\]/);
  assert.match(csharp, /\[ManifestExtra\("Version", "2"\)\]/);
  assert.doesNotMatch(csharp, /Nested/);
});

test("C# rendering preserves safe ABI methods", () => {
  const csharp = renderCSharpContract(
    "contract Token {\nfn balanceOf(account: hash160) -> int {\n}\n}",
    {
      supportedStandards: [],
      abi: {
        methods: [{ name: "balanceOf", safe: true, parameters: [], returnType: "Integer" }],
        events: [],
      },
    },
  );
  assert.match(csharp, /\[Safe\]\npublic static BigInteger balanceOf\(UInt160 account\)/);
});

test("C# rendering preserves raw names for sanitized ABI methods", () => {
  const csharp = renderCSharpContract(
    "contract Token {\nfn balance_of(UInt160 account) -> int {\n}\n}",
    {
      supportedStandards: [],
      abi: {
        methods: [{ name: "balance-of", safe: true, parameters: [], returnType: "Integer" }],
        events: [],
      },
    },
  );
  assert.match(csharp, /\[DisplayName\("balance-of"\)\]/);
});

test("C# rendering lowers unambiguous collection helpers", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn build() -> any {",
    "    let items = new_array(2);",
    "    append(items, value);",
    "    return has_key(map, key);",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /new object\[\(int\)\(2\)\]/);
  assert.match(csharp, /items\.Add\(value\)/);
  assert.match(csharp, /map\.ContainsKey\(key\)/);
});
