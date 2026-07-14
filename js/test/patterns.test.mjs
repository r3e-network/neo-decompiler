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
  assert.ok(info.evidence.some((entry) => entry.source === "nef.header.compiler"));
});

test("pattern analysis accepts canonical raw manifest standard field names", () => {
  const info = identifyPatterns(
    nef(),
    [],
    { supportedstandards: ["NEP-17"], abi: { methods: [], events: [] } },
  );
  assert.deepEqual(info.standards, ["NEP-17"]);
  assert.deepEqual(info.patterns, ["NEP-17"]);
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
  assert.equal(info.confidence, "low");
  assert.ok(info.evidence.some((entry) => entry.source === "nef.header.source"));
});

test("pattern analysis normalizes source paths and URI suffixes", () => {
  for (const [source, expected] of [
    ["C:\\contracts\\Token.cs", "C#"],
    ["/contracts/Token.csproj", "C#"],
    ["/contracts/Token.py?build=42", "Python"],
    ["src/token.go#source", "Go"],
    ["src/token.rs#source", "Rust"],
    ["src/token.java#source", "Java"],
    ["src/token.tsx?source=embedded", "TypeScript/JavaScript"],
    ["src/token.jsx#source", "TypeScript/JavaScript"],
  ]) {
    const info = identifyPatterns(nef("", source), [], null);
    assert.equal(info.language, expected);
  }
});

test("pattern analysis infers Rust from compiler metadata", () => {
  const info = identifyPatterns(nef("neo-rustc 1"), [], null);
  assert.equal(info.language, "Rust");
  assert.equal(info.confidence, "medium");
});

test("pattern analysis infers Java from compiler metadata", () => {
  const info = identifyPatterns(nef("neo-java-compiler 1"), [], null);
  assert.equal(info.language, "Java");
  assert.equal(info.confidence, "medium");
});

test("pattern analysis keeps JavaScript ahead of the Java substring", () => {
  const info = identifyPatterns(nef("neo-javascript-compiler 1"), [], null);
  assert.equal(info.language, "TypeScript/JavaScript");
});

test("pattern analysis identifies signature and multisig syscalls", () => {
  const info = identifyPatterns(
    nef(),
    [{ opcode: { mnemonic: "SYSCALL" }, operand: { value: 0x3ADCD09E } }],
    null,
  );
  assert.deepEqual(info.patterns, ["multisig", "signature_verification"]);
  assert.ok(
    info.evidence.some(
      (entry) => entry.source === "syscall" && entry.value === "System.Crypto.CheckMultisig",
    ),
  );
});

test("pattern analysis identifies CheckWitness authorization", () => {
  const info = identifyPatterns(
    nef(),
    [{ opcode: { mnemonic: "SYSCALL" }, operand: { value: 0x8CEC27F8 } }],
    null,
  );
  assert.deepEqual(info.patterns, ["authorization"]);
  assert.ok(
    info.evidence.some(
      (entry) => entry.source === "syscall" && entry.value === "System.Runtime.CheckWitness",
    ),
  );
});

test("pattern analysis identifies caller and signer context syscalls", () => {
  const info = identifyPatterns(
    nef(),
    [
      { opcode: { mnemonic: "SYSCALL" }, operand: { value: 0x3C6E5339 } },
      { opcode: { mnemonic: "SYSCALL" }, operand: { value: 0x8B18F1AC } },
    ],
    null,
  );
  assert.deepEqual(info.patterns, ["caller_context", "signer_introspection"]);
  assert.ok(
    info.evidence.some(
      (entry) => entry.source === "syscall" && entry.value === "System.Runtime.GetCallingScriptHash",
    ),
  );
  assert.ok(
    info.evidence.some(
      (entry) => entry.source === "syscall" && entry.value === "System.Runtime.CurrentSigners",
    ),
  );
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

test("pattern analysis exposes ABI event behavior", () => {
  const info = identifyPatterns(
    nef(),
    [],
    {
      supportedStandards: [],
      abi: { methods: [], events: [{ name: "Updated", parameters: [] }] },
    },
  );
  assert.deepEqual(info.patterns, ["events"]);
  assert.ok(info.evidence.some((entry) => entry.source === "manifest.abi.events" && entry.value === "1"));
});

test("pattern analysis identifies token transfer behavior from paired ABI method and event", () => {
  const info = identifyPatterns(nef(), [], {
    supportedStandards: [],
    abi: {
      methods: [{ name: "transfer" }],
      events: [{ name: "Transfer", parameters: [] }],
    },
  });
  assert.deepEqual(info.patterns, ["events", "token_transfers"]);
  assert.ok(info.evidence.some((entry) =>
    entry.source === "manifest.abi.methods" && entry.value === "transfer + Transfer"
  ));
});

test("pattern analysis identifies ownership behavior from paired ABI methods", () => {
  const info = identifyPatterns(
    nef(),
    [],
    {
      supportedStandards: [],
      abi: {
        methods: [
          { name: "owner", parameters: [], returnType: "Hash160" },
          { name: "transferOwnership", parameters: [], returnType: "Boolean" },
        ],
        events: [],
      },
    },
  );
  assert.deepEqual(info.patterns, ["ownership"]);
});

test("pattern analysis identifies explicit token lifecycle behaviors", () => {
  const info = identifyPatterns(
    nef(),
    [],
    {
      supportedStandards: [],
      abi: {
        methods: [{ name: "mint" }, { name: "burn" }, { name: "pause" }, { name: "unpause" }],
        events: [],
      },
    },
  );
  assert.deepEqual(info.patterns, ["burning", "minting", "pausable"]);
  assert.ok(
    info.evidence.some(
      (entry) => entry.source === "manifest.abi.methods" && entry.value === "pause,unpause",
    ),
  );
});

test("pattern analysis identifies the NEP-24 royalty method", () => {
  const info = identifyPatterns(
    nef(),
    [],
    {
      supportedStandards: [],
      abi: { methods: [{ name: "royaltyInfo" }], events: [] },
    },
  );
  assert.deepEqual(info.standards, ["NEP-24"]);
  assert.deepEqual(info.patterns, ["NEP-24", "royalties"]);
  assert.ok(info.evidence.some((entry) => entry.value === "royaltyInfo"));
});

test("pattern analysis identifies token payment receiver callbacks without guessing a standard", () => {
  const info = identifyPatterns(nef(), [], {
    name: "Receiver",
    abi: {
      methods: [{ name: "onNEP17Payment", parameters: [], returntype: "Void" }],
      events: [],
    },
  });
  assert.deepEqual(info.standards, []);
  assert.deepEqual(info.patterns, ["token_receiver"]);
  assert.ok(info.evidence.some((entry) =>
    entry.source === "manifest.abi.methods" && entry.value === "onNEP17Payment"
  ));
});

test("pattern analysis ignores malformed manifest collections", () => {
  const info = identifyPatterns(
    nef(),
    [],
    {
      supportedStandards: "NEP-17",
      abi: { methods: [null, 42], events: [null, {}] },
      permissions: [null, { contract: "*" }],
    },
  );
  assert.deepEqual(info.standards, []);
  assert.deepEqual(info.patterns, ["call_permissions", "events", "wildcard_permissions"]);
});

test("pattern analysis identifies external-call bytecode signals", () => {
  const info = identifyPatterns(
    { ...nef(), methodTokens: [{ method: "transfer" }] },
    [{ opcode: { mnemonic: "CALLT" }, operand: { value: 0 } }],
    null,
  );
  assert.deepEqual(info.patterns, ["external_calls", "method_tokens"]);
  assert.ok(info.evidence.some((entry) => entry.source === "bytecode.calls"));
  assert.deepEqual(
    info.evidence,
    [...info.evidence].sort((left, right) =>
      left.source.localeCompare(right.source) || left.value.localeCompare(right.value),
    ),
  );
});

test("pattern analysis identifies native oracle calls from method tokens", () => {
  const info = identifyPatterns(
    {
      ...nef(),
      methodTokens: [{
        hash: [
          0x58, 0x87, 0x17, 0x11, 0x7E, 0x0A, 0xA8, 0x10, 0x72, 0xAF, 0xAB,
          0x71, 0xD2, 0xDD, 0x89, 0xFE, 0x7C, 0x4B, 0x92, 0xFE,
        ],
        method: "Request",
      }],
    },
    [],
    null,
  );
  assert.deepEqual(info.patterns, ["method_tokens", "native_contract_calls", "oracle"]);
  assert.ok(info.evidence.some((entry) => entry.value === "OracleContract::Request"));
});

test("pattern analysis identifies upgradeable native contracts", () => {
  const info = identifyPatterns(
    {
      ...nef(),
      methodTokens: [{
        hash: Uint8Array.from([
          0xFD, 0xA3, 0xFA, 0x43, 0x46, 0xEA, 0x53, 0x2A, 0x25, 0x8F, 0xC4,
          0x97, 0xDD, 0xAD, 0xDB, 0x64, 0x37, 0xC9, 0xFD, 0xFF,
        ]),
        method: "Update",
      }],
    },
    [],
    null,
  );
  assert.deepEqual(info.patterns, [
    "contract_management",
    "method_tokens",
    "native_contract_calls",
    "upgradeable",
  ]);
});

test("pattern analysis identifies native role management calls", () => {
  const info = identifyPatterns(
    {
      ...nef(),
      methodTokens: [{
        hash: Uint8Array.from([
          0xE2, 0x95, 0xE3, 0x91, 0x54, 0x4C, 0x17, 0x8A, 0xD9, 0x4F,
          0x03, 0xEC, 0x4D, 0xCD, 0xFF, 0x78, 0x53, 0x4E, 0xCF, 0x49,
        ]),
        method: "DesignateAsRole",
      }],
    },
    [],
    null,
  );
  assert.deepEqual(info.patterns, ["method_tokens", "native_contract_calls", "role_management"]);
});

test("pattern analysis identifies native system-management contracts", () => {
  const cases = [
    ["7bc681c0a1f71d543457b68bba8d5f9fdd4e5ecc", "BlockAccount", "policy_management"],
    ["9f040ea4a8448f015af645659b0fb2ae7dc500ae", "BalanceOf", "token_management"],
    ["bef2043140362a77c15099c7e64c12f700b665da", "CurrentHash", "ledger"],
    ["3bec3531119bbad76dd044920b0de6c3194fe1c1", "BalanceOf", "notary"],
    ["c13a56c98353a7ea6a324d9a835d1b5bf2266315", "OnNEP11Payment", "treasury"],
  ];
  for (const [hex, method, pattern] of cases) {
    const info = identifyPatterns(
      { ...nef(), methodTokens: [{ hash: Uint8Array.from(Buffer.from(hex, "hex")), method }] },
      [],
      null,
    );
    assert.ok(info.patterns.includes(pattern), `${method} should identify ${pattern}`);
  }
});

test("C# rendering widens direct nullable parameter aliases", () => {
  const rendered = renderCSharpContract(
    [
      "contract NullableAlias {",
      "    fn valueOrDefault(value: int) -> int {",
      "        let local = value;",
      "        if (local is null) {",
      "            return 0;",
      "        }",
      "        return value;",
      "    }",
      "}",
    ].join("\n"),
  );
  assert.match(rendered, /BigInteger valueOrDefault\(dynamic @value\)/);
});

test("C# rendering can opt into conservative typed declarations", () => {
  const source = [
    "contract Typed {",
    "fn main() -> void {",
    "    let count = 0;",
    "    count = count + 1;",
    "    let comparison = count * 2 > 1;",
    "    let values = new_array_t(2, integer);",
    "    let unknown = call();",
    "}",
    "}",
  ].join("\n");
  const defaultRendered = renderCSharpContract(source);
  const typedRendered = renderCSharpContract(source, null, { typedDeclarations: true });
  assert.match(defaultRendered, /var count = 0;/);
  assert.match(typedRendered, /BigInteger count = 0;/);
  assert.match(typedRendered, /bool comparison = count \* 2 > 1;/);
  assert.match(typedRendered, /BigInteger\[\] values = new BigInteger\[/);
  assert.match(typedRendered, /dynamic unknown = call\(\);/);
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

test("C# rendering recognizes additional Neo runtime and crypto syscalls", () => {
  const source = [
    "contract Runtime {",
    'fn inspect() -> any {',
    '    let signers = syscall("System.Runtime.CurrentSigners");',
    '    let random = syscall("System.Runtime.GetRandom");',
    '    let valid = syscall("System.Crypto.CheckSig", key, signature);',
    '    let loaded = syscall("System.Runtime.LoadScript", script, flags, args);',
    '    return syscall("System.Contract.GetCallFlags");',
    "}",
    "}",
  ].join("\n");
  const csharp = renderCSharpContract(source);
  assert.match(csharp, /Runtime\.CurrentSigners\(\)/);
  assert.match(csharp, /Runtime\.GetRandom\(\)/);
  assert.match(csharp, /Crypto\.CheckSig\(key, signature\)/);
  assert.match(csharp, /Runtime\.LoadScript\(script, flags, args\)/);
  assert.match(csharp, /return Contract\.GetCallFlags\(\);/);
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

test("C# rendering escapes control characters in manifest string literals", () => {
  const csharp = renderCSharpContract("contract Token {\n}", {
    extra: { Note: "line\0\t\nnext\rvalue\u0001\u2028" },
  });
  assert.match(csharp, /\[ManifestExtra\("Note", "line\\0\\t\\nnext\\rvalue\\u0001\\u2028"\)\]/);
  assert.doesNotMatch(csharp, /ManifestExtra\("Note", "line\n/);
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

test("C# rendering escapes contextual and newer keyword identifiers", () => {
  const csharp = renderCSharpContract(
    "contract Token {\nfn record(await: int) -> void {\n}\n}",
  );
  assert.match(csharp, /public static void @record\(BigInteger @await\)/);
});

test("C# rendering accepts canonical ABI type aliases in direct high-level input", () => {
  const csharp = renderCSharpContract(
    "contract Aliases {\nfn convert(flag: boolean, count: integer, payload: bytearray, context: interopinterface) -> integer {\n}\n}",
  );
  assert.match(
    csharp,
    /public static BigInteger convert\(bool flag, BigInteger count, ByteString payload, object context\)/,
  );
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
  assert.match(csharp, /List<object>\)items\)\.Add\(value\)/);
  assert.match(csharp, /map\.ContainsKey\(key\)/);
});

test("C# rendering keeps CLEARITEMS compatible with array and map receivers", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn clear() {",
    "    clear_items(items);",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /\(\(dynamic\)items\)\.Clear\(\);/);
});

test("C# rendering uses inferred collection types for removal helpers", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn mutate() {",
    "    let items = new_array(2);",
    "    let map = Map();",
    "    remove_item(items, index);",
    "    remove_item(map, key);",
    "    clear_items(items);",
    "    clear_items(map);",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: true });
  assert.match(csharp, /List<object>\)items\)\.RemoveAt\(\(int\)\(index\)\)/);
  assert.match(csharp, /map\.Remove\(key\)/);
  assert.match(csharp, /List<object>\)items\)\.Clear\(\)/);
  assert.match(csharp, /map\.Clear\(\)/);
});

test("C# rendering lowers common Neo math and byte helpers", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn math(a, b, m, data) -> any {",
    "    let x = abs(a);",
    "    let sign = sign(a);",
    "    let y = min(a, b);",
    "    let z = modmul(a, b, m);",
    "    let slice = substr(data, 1, 2);",
    "    return within(x, y, z);",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /BigInteger\.Abs\(a\)/);
  assert.match(csharp, /\(a\)\.Sign/);
  assert.match(csharp, /BigInteger\.Min\(a, b\)/);
  assert.match(csharp, /Helper\.ModMultiply\(a, b, m\)/);
  assert.match(csharp, /Helper\.Range\(data, \(int\)\(1\), \(int\)\(2\)\)/);
  assert.match(csharp, /Helper\.Within\(x, y, z\)/);
  assert.doesNotMatch(csharp, /\b(?:abs|min|modmul|substr|within)\(/);
});

test("C# rendering lowers power and inferred list pop helpers", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn math(a, b) -> any {",
    "    let items = new_array(2);",
    "    let value = pow(a, b);",
    "    return pop_item(items);",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: true });
  assert.match(csharp, /BigInteger\.Pow\(a, \(int\)\(b\)\)/);
  assert.match(csharp, /List<object>\)items\)\.PopItem\(\)/);
  assert.doesNotMatch(csharp, /\b(?:pow|pop_item)\(/);
});

test("C# rendering lowers packed map and struct literals", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn build() -> any {",
    "    let map = Map(1: 2, 3: 4);",
    "    let structure = Struct(1, map);",
    "    return structure;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /new Map<object, object> \{ \[1\] = 2, \[3\] = 4 \}/);
  assert.match(csharp, /new object\[\] \{ 1, map \}/);
  assert.doesNotMatch(csharp, /\b(?:Map|Struct)\([^)]*:/);
});

test("C# rendering lowers an empty VM struct to a framework-compatible array", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn build() -> any {",
    "    let value = Struct();",
    "    return value;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /var @value = new object\[\] \{ \};/);
  assert.doesNotMatch(csharp, /new Struct\(\)/);
});

test("C# rendering lowers Neo concatenation outside string literals", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn join(a, b) -> string {",
    '    let text = "cat" cat a cat b;',
    "    return text;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /var text = "cat" \+ a \+ b;/);
  assert.doesNotMatch(csharp, /\s+cat\s+/);
});

test("C# rendering preserves typed buffer allocations", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn build() -> any {",
    '    let a = new_array_t(size, "buffer");',
    "    let b = new_buffer(size);",
    "    return a;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /new byte\[\(int\)\(size\)\]/);
  assert.equal((csharp.match(/new byte\[\(int\)\(size\)\]/g) ?? []).length, 2);
});

test("C# rewrites do not alter pseudo-operation text inside literals", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn text() -> string {",
    '    return "new_array(2) syscall(\\"System.Storage.Get\\") has_key(x, y)";',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /new_array\(2\)/);
  assert.match(csharp, /syscall\(\\"System\.Storage\.Get\\"\)/);
  assert.match(csharp, /has_key\(x, y\)/);
  assert.doesNotMatch(csharp, /new object\[\(int\)\(2\)\]/);
});

test("C# rendering keeps ABORT distinct from catchable THROW", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn fail() {",
    "    abort(\"bad\");",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /throw new InvalidOperationException\(Convert\.ToString\("bad"\)\);/);
  assert.doesNotMatch(csharp, /ABORT/);
});

test("C# rendering lowers THROW and ASSERT forms", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn fail(value) {",
    "    assert(value > 0);",
    '    assert(value, "bad" cat value);',
    "    throw(value);",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /ExecutionEngine\.Assert\(\(bool\)\(object\)\(value > 0\)\);/);
  assert.match(csharp, /if \(!\(bool\)\(object\)\(value\)\) throw new InvalidOperationException/);
  assert.match(csharp, /throw new Exception\(Convert\.ToString\(value\)\);/);
  assert.doesNotMatch(csharp, /assert\(/);
  assert.doesNotMatch(csharp, /throw\(value\)/);
});

test("C# rendering lowers native qualified calls outside literals", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn call() -> any {",
    "    let value = GasToken::Transfer(from, to, amount);",
    '    return "GasToken::Transfer(x)";',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /GasToken\.Transfer\(from, to, amount\)/);
  assert.match(csharp, /GasToken::Transfer\(x\)/);
});
