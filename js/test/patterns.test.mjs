import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  decompileBytes,
  decompileBytesWithManifest,
  decompileHighLevelBytesWithManifest,
  identifyPatterns,
  renderCSharpContract,
} from "../src/index.js";
import { inferDeclarationTypes } from "../src/csharp-types.js";
import { splitCallArguments } from "../src/csharp-expression.js";

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

function compareCodepoints(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

test("C# expression scanner preserves nested and quoted arguments", () => {
  assert.deepEqual(
    splitCallArguments('new object[] { "a,b", nested(1, 2) }, value'),
    ['new object[] { "a,b", nested(1, 2) }', "value"],
  );
  assert.deepEqual(splitCallArguments('"a,b", (left, right), value'), [
    '"a,b"',
    "(left, right)",
    "value",
  ]);
});

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
  assert.equal(info.language, null);
  assert.equal(info.confidence, "low");
  assert.ok(info.evidence.some((entry) => entry.source === "nef.header.source"));
});

test("pattern analysis recognizes only supported C# source paths", () => {
  for (const source of ["C:\\contracts\\Token.cs", "/contracts/Token.csproj"]) {
    const info = identifyPatterns(nef("", source), [], null);
    assert.equal(info.language, "C#");
  }
});

test("pattern analysis does not claim unsupported source renderers", () => {
  for (const [compiler, source] of [
    ["boa 1", "contract.py"],
    ["neo-go 1", "contract.go"],
    ["neo-rustc 1", "contract.rs"],
    ["neo-java-compiler 1", "contract.java"],
    ["neo-javascript-compiler 1", "contract.ts"],
    ["Neo.Compiler.Rust 1", "contract.rs"],
    ["Neo.Compiler.Java 1", "contract.java"],
  ]) {
    const info = identifyPatterns(nef(compiler, source), [], null);
    assert.equal(info.language, null, `${compiler} ${source}`);
  }
});

test("pattern analysis requires explicit C# compiler tokens", () => {
  for (const compiler of ["notcsharp", "CSharpX", "Neo.Compiler.CSharpX", "cscompiler"]) {
    const info = identifyPatterns(nef(compiler), [], null);
    assert.equal(info.language, null, compiler);
  }
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

test("pattern analysis identifies storage, runtime, and account syscalls", () => {
  const info = identifyPatterns(
    nef(),
    [
      0x31E85D92, // System.Storage.Get
      0x0AE30C39, // System.Storage.Local.Put
      0xEDC5582F, // System.Storage.Delete
      0x9AB830DF, // System.Storage.Find
      0x9CED089C, // System.Iterator.Next
      0xDC92494C, // System.Runtime.GetAddressVersion
      0x09E9336A, // System.Contract.CreateMultisigAccount
      0xBC8C5AC3, // System.Runtime.BurnGas
    ].map((value) => ({
      opcode: { mnemonic: "SYSCALL" },
      operand: { value },
    })),
    null,
  );
  assert.deepEqual(info.patterns, [
    "account_creation",
    "gas_management",
    "iterator_usage",
    "runtime_context",
    "storage",
    "storage_deletes",
    "storage_iteration",
    "storage_reads",
    "storage_writes",
  ]);
  assert.equal(
    info.evidence.filter((entry) => entry.source === "syscall").length,
    12,
  );
  assert.ok(info.evidence.some((entry) =>
    entry.source === "syscall" && entry.value === "System.Storage.Local.Put"
  ));
  assert.ok(info.evidence.some((entry) =>
    entry.source === "syscall" && entry.value === "System.Runtime.GetAddressVersion"
  ));
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

test("pattern analysis mirrors tolerant wildcard method selectors", () => {
  const hashContract = "0x" + "11".repeat(20);
  const omitted = identifyPatterns(nef(), [], {
    supportedStandards: [],
    permissions: [{ contract: hashContract }],
    abi: { methods: [], events: [] },
  });
  const nonCanonical = identifyPatterns(nef(), [], {
    supportedStandards: [],
    permissions: [{ contract: hashContract, methods: "all" }],
    abi: { methods: [], events: [] },
  });
  for (const info of [omitted, nonCanonical]) {
    assert.deepEqual(info.patterns, ["call_permissions", "wildcard_permissions"]);
  }
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
      abi: { methods: [null, 42], events: [null, {}, { name: 42 }] },
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
      compareCodepoints(left.source, right.source) || compareCodepoints(left.value, right.value),
    ),
  );
});

test("pattern evidence ordering matches Rust codepoint sorting", () => {
  const stdLibHash = Uint8Array.from([
    0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2,
    0xA0, 0x6A, 0x79, 0xE1, 0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
  ]);
  const info = identifyPatterns(
    { ...nef(), methodTokens: [
      { hash: stdLibHash, method: "strLen" },
      { hash: stdLibHash, method: "memorySearch" },
      { hash: stdLibHash, method: "stringSplit" },
    ] },
    [],
    null,
  );
  assert.deepEqual(
    info.evidence
      .filter((entry) => entry.source === "nef.method_tokens.native")
      .map((entry) => entry.value),
    ["StdLib::MemorySearch", "StdLib::StrLen", "StdLib::StringSplit"],
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
    "contract_lifecycle",
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

test("pattern analysis identifies fine-grained native behavior", () => {
  const cases = [
    ["1bf575ab1189688413610a35a12886cde0b66c72", "Sha256", "cryptography"],
    ["c0ef39cee0e4e925c6c2a06a79e1440dd86fceac", "JsonSerialize", "serialization"],
    ["c0ef39cee0e4e925c6c2a06a79e1440dd86fceac", "StringSplit", "string_operations"],
    ["bef2043140362a77c15099c7e64c12f700b665da", "GetBlock", "blockchain_queries"],
    ["f563ea40bc283d4d0e05c48ea305b3f2a07340ef", "Transfer", "native_token_calls"],
    ["fda3fa4346ea532a258fc497ddaddb6437c9fdff", "Deploy", "contract_lifecycle"],
    ["fda3fa4346ea532a258fc497ddaddb6437c9fdff", "GetContract", "contract_queries"],
  ];
  for (const [hex, method, pattern] of cases) {
    const info = identifyPatterns(
      { ...nef(), methodTokens: [{ hash: Uint8Array.from(Buffer.from(hex, "hex")), method }] },
      [],
      null,
    );
    assert.ok(info.patterns.includes(pattern), `${method} should identify ${pattern}`);
    assert.ok(info.evidence.some((entry) =>
      entry.source === "nef.method_tokens.pattern" &&
      entry.value.includes(pattern) && entry.value.includes(method)
    ));
  }
});

test("high-level C# output surfaces its inferred pattern summary", () => {
  const result = decompileHighLevelBytesWithManifest(
    buildNef(),
    {
      name: "Token",
      supportedstandards: ["NEP-17"],
      abi: { methods: [], events: [] },
    },
  );
  assert.match(result.csharp, /    \/\/ inferred standards: NEP-17/);
  assert.match(result.csharp, /    \/\/ inferred patterns: NEP-17/);
  assert.match(result.csharp, /    \/\/ inferred language: C#/);
  assert.match(result.csharp, /    \/\/ pattern confidence: high/);
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

test("C# rendering widens parameters checked with the VM null helper", () => {
  const rendered = renderCSharpContract([
    "contract NullableVmCheck {",
    "    fn valueOrDefault(value: int) -> int {",
    "        if is_null(value) {",
    "            return 0;",
    "        }",
    "        return value;",
    "    }",
    "}",
  ].join("\n"));
  assert.match(rendered, /BigInteger valueOrDefault\(dynamic @value\)/);
});

test("C# rendering boxes VM null-check operands", () => {
  const rendered = renderCSharpContract([
    "contract NullLiteral {",
    "    fn check() -> bool {",
    "        return is_null(1);",
    "    }",
    "}",
  ].join("\n"));
  assert.match(rendered, /return \(\(\(object\)\(1\)\) is null\);/);
});

test("C# rendering uses conservative typed declarations by default", () => {
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
  const typedRendered = renderCSharpContract(source);
  const legacyRendered = renderCSharpContract(source, null, { typedDeclarations: false });
  assert.match(typedRendered, /BigInteger count = 0;/);
  assert.match(typedRendered, /bool comparison = count \* 2 > 1;/);
  assert.match(typedRendered, /BigInteger\[\] values = new BigInteger\[/);
  assert.match(typedRendered, /dynamic unknown = call\(\);/);
  assert.match(legacyRendered, /var count = 0;/);
});

test("C# typed declarations infer known helper return annotations", () => {
  const rendered = renderCSharpContract([
    "contract KnownCalls {",
    "fn helper() -> int {",
    "    return 1;",
    "}",
    "fn unknownHelper() -> any {",
    "    return 1;",
    "}",
    "fn main() -> void {",
    "    let value = helper();",
    "    let unknown = unknownHelper();",
    "    let missing = missingHelper();",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /BigInteger @value = helper\(\);/);
  assert.match(rendered, /dynamic unknown = unknownHelper\(\);/);
  assert.match(rendered, /dynamic missing = missingHelper\(\);/);
});

test("C# typed declarations stay scoped to each method", () => {
  const source = [
    "contract TypedScopes {",
    "fn integerValue() -> void {",
    "    let value = 1;",
    "}",
    "fn booleanValue() -> void {",
    "    let value = true;",
    "}",
    "}",
  ].join("\n");
  const rendered = renderCSharpContract(source, null, { typedDeclarations: true });
  assert.match(rendered, /BigInteger @value = 1;/);
  assert.match(rendered, /bool @value = true;/);
  assert.doesNotMatch(rendered, /dynamic @value/);
});

test("C# typed declarations keep VM shifts numeric and cast their counts", () => {
  const rendered = renderCSharpContract([
    "contract ShiftTypes {",
    "fn rotate(value: int, shift: int) -> any {",
    "    let shifted = value << shift;",
    "    let restored = shifted >> (shift + 1);",
    "    return restored;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /BigInteger shifted = @value << \(int\)\(shift\);/);
  assert.match(rendered, /BigInteger restored = shifted >> \(int\)\(\(shift \+ 1\)\);/);
});

test("C# rendering casts VM indices and byte-string numeric conversions", () => {
  const rendered = renderCSharpContract([
    "contract IndexTypes {",
    "fn inspect(value: string, index: int) -> any {",
    "    let item = value[index];",
    "    let bytes = convert_to_bytestring(32);",
    "    let slice = convert_to_bytestring(substr(value, index, 1));",
    "    return slice;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /dynamic item = @value\[\(int\)\(index\)\];/);
  assert.match(rendered, /ByteString bytes = \(ByteString\)\(BigInteger\)\(32\);/);
  assert.match(rendered, /ByteString slice = \(ByteString\)\(Helper\.Range\(\(byte\[\]\)\(ByteString\)\(@value\),/);
});

test("C# rendering keeps mixed VM operators dynamically bindable", () => {
  const rendered = renderCSharpContract([
    "contract MixedOperators {",
    "fn compare(value: any) -> bool {",
    "    return value == 1;",
    "}",
    "fn subtract(left: bool, right: bool) -> any {",
    "    return left - right;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /return \(\(dynamic\)\(@value\)\) == 1;/);
  assert.match(rendered, /return \(\(dynamic\)\(left\)\) - right;/);
});

test("C# rendering hoists VM slots that cross source block scopes", () => {
  const rendered = renderCSharpContract([
    "contract ScopeSlots {",
    "fn sample() -> any {",
    "    let loc0 = 0;",
    "    do {",
    "        let loc1 = 1;",
    "    } while (loc1 != 2);",
    "    loc1 = 3;",
    "    return loc1;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /BigInteger loc1 = default;/);
  assert.doesNotMatch(rendered, /let loc1/);
  assert.doesNotMatch(rendered, /BigInteger loc1 = 1;/);
});

test("C# rendering keeps for-loop declarations in their lexical scope", () => {
  const rendered = renderCSharpContract([
    "contract LoopScope {",
    "fn sample() -> void {",
    "    for (let loc0 = 0; loc0 < 3; loc0 += 1) {",
    "    }",
    "    return;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /for \(BigInteger loc0 = 0; loc0 < 3; loc0 \+= 1\)/);
  assert.doesNotMatch(rendered, /default;\s*for \(BigInteger loc0/);
});

test("C# rendering removes an unused generated local copy", () => {
  const rendered = renderCSharpContract([
    "contract DeadCopies {",
    "fn sample(value: int) -> int {",
    "    let loc0 = value;",
    "    return value;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /return @value;/);
  assert.doesNotMatch(rendered, /loc0/);
});

test("C# rendering keeps a generated copy when it is read", () => {
  const rendered = renderCSharpContract([
    "contract LiveCopies {",
    "fn sample(value: int) -> int {",
    "    let loc0 = value;",
    "    return loc0;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /BigInteger loc0 = @value;/);
  assert.match(rendered, /return loc0;/);
});

test("C# rendering keeps a type-conflicting generated copy conservative", () => {
  const rendered = renderCSharpContract([
    "contract ConflictingCopies {",
    "fn sample(value: int, text: string) -> any {",
    "    let t0 = value;",
    "    let t0 = text;",
    "    return null;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /dynamic t0 = default;/);
  assert.match(rendered, /t0 = @value;/);
  assert.match(rendered, /t0 = text;/);
});

test("C# rendering hoists a for-loop variable only when it escapes", () => {
  const rendered = renderCSharpContract([
    "contract LoopScopeEscape {",
    "fn sample() -> int {",
    "    for (let loc0 = 0; loc0 < 3; loc0 += 1) {",
    "    }",
    "    return loc0;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /BigInteger loc0 = default;/);
  assert.match(rendered, /for \(loc0 = 0; loc0 < 3; loc0 \+= 1\)/);
  assert.doesNotMatch(rendered, /for \(var loc0/);
});

test("C# rendering dynamically binds compound VM call and comparison operands", () => {
  const rendered = renderCSharpContract([
    "contract CompoundOperators {",
    "fn check() -> bool {",
    "    return syscall(\"System.Storage.Local.Get\", 1) == 1;",
    "}",
    "fn compare(a: int, b: int) -> any {",
    "    return (a > b) - (b > a);",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /return \(\(dynamic\)\(Storage.Get\(/);
  assert.match(rendered, /return \(\(dynamic\)\(\(a > b\)\)\) - \(b > a\);/);
});

test("C# rendering keeps dynamic compound assignments assignable", () => {
  const rendered = renderCSharpContract([
    "contract CompoundAssignments {",
    "fn update(value: int) -> any {",
    "    value += 1;",
    "    return value;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /@value = \(\(dynamic\)\(@value\)\) \+ 1;/);
  assert.doesNotMatch(rendered, /\(\(dynamic\)\(@value\)\) \+=/);
});

test("C# rendering folds complete literal expressions conservatively", () => {
  const csharp = renderCSharpContract([
    "contract Constants {",
    "fn add() -> int {",
    "    return 1 + 1;",
    "}",
    "fn nested() -> int {",
    "    let value = (1 + 2) * 3;",
    "    return value;",
    "}",
    "fn compare() -> bool {",
    "    return 2 < 3;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return 2;/);
  assert.match(csharp, /BigInteger @value = 9;/);
  assert.match(csharp, /return true;/);
  assert.doesNotMatch(csharp, /dynamic\)\(1 \+ 1/);
  assert.doesNotMatch(csharp, /dynamic\)\(\(1 \+ 2\)\)/);
});

test("C# constant folding refuses fault-prone and mixed expressions", () => {
  const csharp = renderCSharpContract([
    "contract Constants {",
    "fn divide() -> int {",
    "    return 1 / 0;",
    "}",
    "fn mixed(value) -> int {",
    "    return value + 1 + 2;",
    "}",
    "fn text() -> string {",
    '    return "1 + 2";',
    "}",
    "}",
  ].join("\n"));
  assert.doesNotMatch(csharp, /return 0;/);
  assert.match(csharp, /return \(BigInteger\)\(dynamic\)\(1 \/ 0\);/);
  assert.match(csharp, /return \(BigInteger\)\(dynamic\)\(\(\(dynamic\)\(@value\)\) \+ 1 \+ 2\);/);
  assert.match(csharp, /return \"1 \+ 2\";/);
});

test("C# rendering lowers VM function-pointer markers", () => {
  const rendered = renderCSharpContract([
    "contract FunctionPointers {",
    "fn invoke() -> any {",
    "    let callback = &sub_0x0010;",
    "    return call(callback, &sub_0x0020);",
    "}",
    "}",
  ].join("\n"));
  assert.doesNotMatch(rendered, /(?:=|,)\s*&sub_0x/);
  assert.match(rendered, /default\(dynamic\) \/\* unresolved VM function pointer &sub_0x0010 \*\//);
  assert.match(rendered, /default\(dynamic\) \/\* unresolved VM function pointer &sub_0x0020 \*\//);
});

test("C# rendering lowers method-token calls through Contract.Call", () => {
  const rendered = renderCSharpContract([
    "contract MethodTokens {",
    "fn invoke() -> any {",
    "    return testArgs1(4);",
    "}",
    "}",
  ].join("\n"), null, {
    methodTokens: [{
      hash: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]),
      method: "testArgs1",
      callFlags: 15,
      parametersCount: 1,
      hasReturnValue: true,
    }],
  });
  assert.match(rendered, /return Contract\.Call\(\(UInt160\)new byte\[\] \{ 0x01, 0x02, 0x03/);
  assert.match(rendered, /"testArgs1", \(CallFlags\)\(15\), new object\[\] \{ 4 \}\)/);
  assert.doesNotMatch(rendered, /return testArgs1\(4\);/);
});

test("C# method-token rewriting leaves qualified native calls intact", () => {
  const rendered = renderCSharpContract([
    "contract NativeCall {",
    "fn invoke() -> any {",
    "    return NeoToken.Transfer(from, to, amount);",
    "}",
    "}",
  ].join("\n"), null, {
    methodTokens: [{
      hash: new Uint8Array(20),
      method: "Transfer",
      callFlags: 15,
      parametersCount: 3,
      hasReturnValue: true,
    }],
  });
  assert.match(rendered, /return NeoToken\.Transfer\(@from, to, amount\);/);
  assert.doesNotMatch(rendered, /Contract\.Call\(/);
});

test("C# native method tokens keep framework casing and metadata comments intact", () => {
  const stdLibHash = Uint8Array.from([
    0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2,
    0xA0, 0x6A, 0x79, 0xE1, 0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
  ]);
  const cryptoHash = Uint8Array.from([
    0x1B, 0xF5, 0x75, 0xAB, 0x11, 0x89, 0x68, 0x84, 0x13, 0x61,
    0x0A, 0x35, 0xA1, 0x28, 0x86, 0xCD, 0xE0, 0xB6, 0x6C, 0x72,
  ]);
  const rendered = renderCSharpContract([
    "contract NativeSpelling {",
    "    // itoa (StdLib::Itoa) hash=C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC",
    "fn invoke(value, data, sig) -> any {",
    "    let text = StdLib::Itoa(value);",
    "    let bytes = StdLib::Base64UrlDecode(text);",
    "    return CryptoLib::recoverSecp256K1(data, sig);",
    "}",
    "}",
  ].join("\n"), null, {
    methodTokens: [
      { hash: stdLibHash, method: "itoa", callFlags: 15, parametersCount: 1, hasReturnValue: true },
      { hash: stdLibHash, method: "base64UrlDecode", callFlags: 15, parametersCount: 1, hasReturnValue: true },
      { hash: cryptoHash, method: "recoverSecp256K1", callFlags: 15, parametersCount: 2, hasReturnValue: true },
    ],
  });

  assert.match(rendered, /return CryptoLib\.RecoverSecp256K1\(data, sig\);/);
  assert.match(rendered, /StdLib\.Base64UrlDecode\(text\)/);
  assert.match(rendered, /\/\/ itoa \(StdLib::Itoa\) hash=/);
  assert.doesNotMatch(rendered, /Contract\.Call\(.*StdLib::Itoa/);
});

test("C# typed declarations propagate concrete collection aliases", () => {
  const rendered = renderCSharpContract([
    "contract AliasTypes {",
    "fn inspect() -> any {",
    "    let source = [1, 2];",
    "    let copy = source;",
    "    let count = len(copy);",
    "    return count;",
    "}",
    "}",
  ].join("\n"));
  assert.match(rendered, /object\[\] source = new object\[\] \{ 1, 2 \};/);
  assert.match(rendered, /object\[\] copy = source;/);
  assert.match(rendered, /BigInteger count = copy\.Length;/);
  assert.doesNotMatch(rendered, /dynamic copy/);
  assert.doesNotMatch(rendered, /\(\(dynamic\)copy\)\.Count/);
});

test("C# declaration aliases keep unresolved definitions conservative", () => {
  const types = inferDeclarationTypes([
    "let source = unknown_call();",
    "let copy = source;",
  ]);
  assert.equal(types.get("source"), "dynamic");
  assert.equal(types.get("copy"), "dynamic");
});

test("C# declaration aliases only join neutral nulls with generic arrays", () => {
  const generic = inferDeclarationTypes([
    "let source = [1];",
    "let source = null;",
    "let copy = source;",
  ]);
  assert.equal(generic.get("source"), "object[]");
  assert.equal(generic.get("copy"), "object[]");

  const typed = inferDeclarationTypes([
    'let source = new_array_t(size, "int");',
    "let source = null;",
    "let copy = source;",
  ]);
  assert.equal(typed.get("source"), "dynamic");
  assert.equal(typed.get("copy"), "dynamic");
});

test("C# typed declarations use catalog syscall return types", () => {
  const source = [
    "contract Syscalls {",
    "fn inspect() -> any {",
    '    let time = syscall("System.Runtime.GetTime");',
    '    let context = syscall("System.Storage.GetContext");',
    '    let value = syscall("System.Storage.Get", context, key);',
    '    let iterator = syscall("System.Storage.Local.Find", key, 0);',
    '    let next = syscall("System.Iterator.Next", iterator);',
    '    let unknown = syscall("System.Custom.Unknown", key);',
    "}",
    "}",
  ].join("\n");
  const typedRendered = renderCSharpContract(source, null, { typedDeclarations: true });
  assert.match(typedRendered, /BigInteger time = Runtime\.Time;/);
  assert.match(typedRendered, /StorageContext context = Storage\.CurrentContext;/);
  assert.match(typedRendered, /ByteString @value = Storage\.Get\(context, key\);/);
  assert.match(typedRendered, /Iterator iterator = Storage\.Find\(key, \(FindOptions\)\(0\)\);/);
  assert.match(typedRendered, /bool next = iterator\.Next\(\);/);
  assert.match(
    typedRendered,
    /dynamic unknown = default\(dynamic\) \/\* unresolved VM syscall "System\.Custom\.Unknown"\(key\) \*\//,
  );
});

test("C# typed declarations use catalog native method return types", () => {
  const rendered = renderCSharpContract([
    "contract Natives {",
    "fn main(value, text, data, sig) -> any {",
    "    let s = StdLib::Itoa(value);",
    "    let bytes = StdLib::Base64UrlDecode(text);",
    "    let n = StdLib::StrLen(s);",
    "    let parts = StdLib::StringSplit(s, \",\");",
    "    let digest = CryptoLib::Sha256(bytes);",
    "    let recovered = CryptoLib::recoverSecp256K1(data, sig);",
    "    let unknown = CustomContract::DoThing(value);",
    "    return n;",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: true });

  assert.match(rendered, /string s = StdLib\.Itoa\(@value\);/);
  assert.match(rendered, /ByteString bytes = StdLib\.Base64UrlDecode\(text\);/);
  assert.match(rendered, /BigInteger n = StdLib\.StrLen\(s\);/);
  assert.match(rendered, /object\[\] parts = StdLib\.StringSplit\(s, ","\);/);
  assert.match(rendered, /ByteString digest = CryptoLib\.Sha256\(bytes\);/);
  assert.match(rendered, /ByteString recovered = CryptoLib\.RecoverSecp256K1\(data, sig\);/);
  assert.match(rendered, /dynamic unknown = CustomContract\.DoThing\(@value\);/);
});

test("C# rendering keeps known void natives and syscalls statement-only", () => {
  const rendered = renderCSharpContract([
    "contract VoidCalls {",
    "fn main(nef, manifest, data, url, filter, cb, user, gas) -> void {",
    "    let destroyed = ContractManagement::Destroy();",
    "    let updated = ContractManagement::Update(nef, manifest, data);",
    "    let requested = OracleContract::Request(url, filter, cb, user, gas);",
    '    let logged = syscall("System.Runtime.Log", "hi");',
    '    syscall("System.Storage.Put", ctx, key, val);',
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: true });

  assert.match(rendered, /^\s*ContractManagement\.Destroy\(\);$/m);
  assert.match(rendered, /^\s*ContractManagement\.Update\(nef, manifest, data\);$/m);
  assert.match(rendered, /^\s*OracleContract\.Request\(url, filter, cb, user, gas\);$/m);
  assert.match(rendered, /^\s*Runtime\.Log\("hi"\);$/m);
  assert.match(rendered, /^\s*Storage\.Put\(ctx, key, val\);$/m);
  assert.doesNotMatch(rendered, /dynamic destroyed|void destroyed|var destroyed/);
  assert.doesNotMatch(rendered, /dynamic logged|void logged|var logged/);
  assert.doesNotMatch(rendered, /=\s*ContractManagement\.Destroy/);
  assert.doesNotMatch(rendered, /=\s*Runtime\.Log/);
});

test("C# rendering lowers known syscalls and keeps unknown ones compile-safe", () => {
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
  assert.match(
    csharp,
    /default\(dynamic\) \/\* unresolved VM syscall "System\.Custom\.Unknown"\(key\) \*\//,
  );
});

test("C# rendering replays unknown numeric syscall hashes through Runtime.LoadScript", () => {
  const csharp = renderCSharpContract([
    "contract UnknownSyscall {",
    "fn get(key) -> any {",
    "    return syscall(0xDEADBEEF, key);",
    "}",
    "}",
  ].join("\n"));
  assert.match(
    csharp,
    /Runtime\.LoadScript\(\(ByteString\)new byte\[\] \{ 0x41, 0xEF, 0xBE, 0xAD, 0xDE \}, CallFlags\.All, new object\[\] \{ key \}\)/,
  );
  assert.doesNotMatch(csharp, /syscall\(0xDEADBEEF/);
});

test("C# storage calls normalize numeric VM keys and find options", () => {
  const csharp = renderCSharpContract([
    "contract StorageTypes {",
    "fn write() {",
    '    syscall("System.Storage.Local.Put", 0xFF00, 0);',
    '    syscall("System.Storage.Local.Find", 0xFF, 0);',
    '    syscall("System.Storage.Put", syscall("System.Storage.GetContext"), 0xFF00, 0);',
    '    syscall("System.Storage.Find", syscall("System.Storage.GetContext"), 0xFF, 0);',
    "    return;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /Storage\.Put\(\(ByteString\)\(BigInteger\)\(0xFF00\), 0\);/);
  assert.match(csharp, /Storage\.Find\(\(ByteString\)\(BigInteger\)\(0xFF\), \(FindOptions\)\(0\)\);/);
  assert.match(csharp, /Storage\.Put\(Storage\.CurrentContext, \(ByteString\)\(BigInteger\)\(0xFF00\), 0\);/);
  assert.match(csharp, /Storage\.Find\(Storage\.CurrentContext, \(ByteString\)\(BigInteger\)\(0xFF\), \(FindOptions\)\(0\)\);/);
});

test("C# contract calls normalize numeric call flags", () => {
  const csharp = renderCSharpContract([
    "contract ContractCallFlags {",
    "fn call(account, args) -> any {",
    '    return syscall("System.Contract.Call", account, "method", 15, args);',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return Contract\.Call\(account, "method", \(CallFlags\)\(15\), args\);/);
});

test("C# rendering coerces compound VM truthiness for unary NOT", () => {
  const csharp = renderCSharpContract([
    "contract Truthiness {",
    "fn odd(value: int) -> bool {",
    "    return !(value % 2);",
    "}",
    "fn flag(value: bool) -> bool {",
    "    return !value;",
    "}",
    "fn comparison(value: int) -> bool {",
    "    return !(value > 0);",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return !\(\(bool\)\(dynamic\)\(@value % 2\)\);/);
  assert.match(csharp, /public static bool flag\(bool @value\) \{\n\s+return !@value;/);
  assert.match(csharp, /return !\(@value > 0\);/);
});

test("C# rendering rewrites nested syscall arguments", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn get(key) -> any {",
    '    return syscall("System.Storage.Get", syscall("System.Storage.GetReadOnlyContext"), key);',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /Storage\.Get\(Storage\.CurrentReadOnlyContext, key\)/);
  assert.doesNotMatch(csharp, /syscall\("System\.Storage\.GetReadOnlyContext"/);
});

test("C# rendering keeps unknown stack placeholders valid and literal", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn unknown() -> any {",
    "    let value = ???;",
    '    return "???";',
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: false });
  assert.match(csharp, /var @value = default\(dynamic\);/);
  assert.match(csharp, /return \"\?\?\?\";/);
  assert.doesNotMatch(csharp, /\?\?\?(?!\")/);
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

test("C# rendering lowers iterator and local storage syscalls", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn read(iterator, context, key) -> any {",
    '    let next = syscall("System.Iterator.Next", iterator);',
    '    let value = syscall("System.Iterator.Value", iterator);',
    '    let read = syscall("System.Storage.Local.Get", key);',
    '    let readonly = syscall("System.Storage.AsReadOnly", context);',
    '    return syscall("System.Storage.Local.Put", key, value);',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /iterator\.Next\(\)/);
  assert.match(csharp, /iterator\.Value/);
  assert.match(csharp, /Storage\.Get\(key\)/);
  assert.match(csharp, /context\.AsReadOnly/);
  assert.match(csharp, /Storage\.Put\(key, @value\)/);
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
    [
      "contract Token {",
      "fn record(await: int) -> int {",
      "    let value = 1;",
      "    return value + await;",
      "}",
      "}",
    ].join("\n"),
  );
  assert.match(csharp, /public static BigInteger @record\(BigInteger @await\)/);
  assert.match(csharp, /BigInteger @value = 1;/);
  assert.match(csharp, /return @value \+ @await;/);
});

test("C# identifier rewriting preserves literals, comments, and loop syntax", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn loop(await: int) -> int {",
    "    for (let index = 0; index < await; index = index + 1) {",
    '        let value = "await value"; // await value',
    "        return await; // await value",
    "    }",
    "    return 0;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /for \(BigInteger index = 0; index < @await; index = index \+ 1\) \{/);
  assert.match(csharp, /dynamic @value = "await value"; \/\/ await value/);
  assert.match(csharp, /return @await; \/\/ await value/);
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
  ].join("\n"), null, { typedDeclarations: false });
  assert.match(csharp, /new object\[\(int\)\(2\)\]/);
  assert.match(csharp, /\(\(dynamic\)items\)\.Add\(@value\)/);
  assert.match(csharp, /\(\(dynamic\)map\)\.HasKey\(key\)/);
});

test("C# rendering lowers VM array, type-test, and memory helpers", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn helpers(size, value) -> any {",
    "    let items = [];",
    "    let count = len(items);",
    "    let isArray = is_type_array(items);",
    "    memcpy(items, 0, items, 0, count);",
    "    return convert(value);",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: true });
  assert.match(csharp, /object\[\] items = new object\[\] \{ \};/);
  assert.match(csharp, /BigInteger count = items\.Length;/);
  assert.match(csharp, /bool isArray = \(\(object\)\(items\)\) is object\[\];/);
  assert.match(csharp, /Array\.Copy\(items, \(int\)\(0\), items, \(int\)\(0\), \(int\)\(count\)\);/);
  assert.match(csharp, /return \(object\)\(@value\);/);
  assert.doesNotMatch(csharp, /\b(?:len|is_type_array|memcpy|convert)\(/);
});

test("C# rendering lowers dynamic unpack helpers to indexable values", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn unpackValue(value) -> any {",
    "    let values = unpack(value);",
    "    let first = unpack_item(values, 0);",
    "    return first;",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: false });
  assert.match(csharp, /var values = \(\(dynamic\)@value\);/);
  assert.match(csharp, /var first = \(\(dynamic\)values\)\[\(int\)\(0\)\];/);
  assert.doesNotMatch(csharp, /\b(?:unpack|unpack_item)\(/);
});

test("C# rendering keeps dynamic pack sizes as C# arrays", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn packValue(count) -> any {",
    "    return pack_dynamic(count);",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return new object\[\(int\)\(count\)\];/);
  assert.doesNotMatch(csharp, /pack_dynamic\(/);
});

test("C# rendering lowers unresolved VM stack helpers and discarded values", () => {
  const csharp = renderCSharpContract([
    "contract StackFallback {",
    "fn test(value: any) -> any {",
    "    let picked = pick(value);",
    "    let rolled = roll(3); // dynamic roll",
    "    null;",
    "    picked;",
    "    return rolled;",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: false });

  assert.match(csharp, /var picked = default\(dynamic\) \/\* unresolved VM PICK\(value\) \*\//);
  assert.match(csharp, /var rolled = default\(dynamic\) \/\* unresolved VM ROLL\(3\) \*\/; \/\/ dynamic roll/);
  assert.match(csharp, /global::System\.Convert\.ToString\(\(object\)\(null\)\);/);
  assert.match(csharp, /global::System\.Convert\.ToString\(\(object\)\(picked\)\);/);
  assert.doesNotMatch(csharp, /\blet\b|\b(?:pick|roll)\(/);
});

test("C# rendering comments metadata block continuations", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "    permissions {",
    '        contract=hash:0x0123456789abcdef0123456789abcdef01234567 methods=["transfer"]',
    "    }",
    "    fn verify() -> bool {",
    "        return true;",
    "    }",
    "}",
  ].join("\n"));
  assert.match(csharp, /\/\/ permissions \{/);
  assert.match(csharp, /\/\/ contract=hash:0x0123456789abcdef0123456789abcdef01234567 methods=\["transfer"\]/);
  assert.match(csharp, /\/\/ \}/);
  assert.doesNotMatch(csharp, /^\s+contract=hash:/m);
});

test("C# rendering lowers buffer conversions and oversized byte literals", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn buffer() -> any {",
    "    return convert_to_buffer(0x010203);",
    "}",
    "fn bytes() -> any {",
    "    return 0x024700DB2E90D9F02C4F9FC862ABACA92725F95B4FDDCC8D7FFA538693ECF463A9;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return \(\(BigInteger\)\(0x010203\)\)\.ToByteArray\(\);/);
  assert.match(csharp, /return \(ByteString\)new byte\[\] \{ 0x02, 0x47, 0x00, 0xDB/);
  assert.doesNotMatch(csharp, /0x024700DB2E90D9F02C4F9FC862ABACA92725F95B4FDDCC8D7FFA538693ECF463A9/);
});

test("C# rendering preserves oversized decimal integers with BigInteger.Parse", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn value() -> int {",
    "    return 18446744073709551616;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return BigInteger\.Parse\("18446744073709551616"\);/);
});

test("C# rendering lowers collection literals and pseudo loops", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn flow(flag: bool) -> any {",
    "    if flag {",
    "        return [[3]];",
    "    }",
    "    loop {",
    "        break;",
    "    }",
    "    return [1, 2];",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /return new object\[\] \{ new object\[\] \{ 3 \} \};/);
  assert.match(csharp, /while \(true\) \{/);
  assert.match(csharp, /return new object\[\] \{ 1, 2 \};/);
  assert.doesNotMatch(csharp, /\bloop\s*\{/);
});

test("C# rendering adapts high-level control syntax", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn flow(value) -> void {",
    "    if value { goto label_0x000A; }",
    "    while 1 {",
    "        break;",
    "    }",
    "    label_0x000A:",
    "    do {",
    "        continue;",
    "    } while (value);",
    "    leave label_0x000A;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /if \(\(bool\)\(dynamic\)\(@value\)\) \{ goto label_0x000A; \}/);
  assert.match(csharp, /while \(1 != 0\) \{/);
  assert.match(csharp, /\} while \(\(bool\)\(dynamic\)\(@value\)\);/);
  assert.match(csharp, /goto label_0x000A;/);
  assert.doesNotMatch(csharp, /\b(?:if value|while 1|leave label_)/);
});

test("C# rendering comments unresolved method-scoped gotos", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn flow(value) -> void {",
    "    if value { goto label_0x000A; }",
    "    goto label_0x000B;",
    "    label_0x000A:",
    "    return;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /goto label_0x000A;/);
  assert.match(csharp, /\/\/ unresolved control transfer: goto label_0x000B;/);
  assert.doesNotMatch(csharp, /^\s+goto label_0x000B;$/m);
});

test("C# rendering scopes switch syntax and deduplicates method labels", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn flow(value) {",
    "    switch value {",
    "        case 0 {",
    "            goto label_0x000A;",
    "        }",
    "        default {",
    "            goto label_0x000A;",
    "        }",
    "    }",
    "    label_0x000A:",
    "    label_0x000A:",
    "    return;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /switch \(@value\) \{/);
  assert.match(csharp, /case 0: \{/);
  assert.match(csharp, /default: \{/);
  assert.equal((csharp.match(/label_0x000A:/g) ?? []).length, 1);
  assert.doesNotMatch(csharp, /switch @value \{|case 0 \{|default \{/);
});

test("C# rendering gives nested catch clauses distinct exception bindings", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn nested() {",
    "    try {",
    "        let outer = 1;",
    "    } catch {",
    "        let outer = exception;",
    "        try {",
    "            throw(\"exception\");",
    "        } catch {",
    "            let inner = exception;",
    "        }",
    "    }",
    "    return;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /catch \(Exception exception\) \{/);
  assert.match(csharp, /catch \(Exception exception_1\) \{/);
  assert.match(csharp, /dynamic inner = exception_1;/);
  assert.match(csharp, /throw new Exception\(Convert\.ToString\("exception"\)\);/);
});

test("C# rendering replays framework-internal syscalls through Runtime.LoadScript", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn native(version: int) -> any {",
    '    return syscall("System.Contract.CallNative", version);',
    "}",
    "fn lifecycle() {",
    '    syscall("System.Contract.NativeOnPersist");',
    '    syscall("System.Contract.NativePostPersist");',
    '    syscall("System.Runtime.Notify", "event", state);',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /Runtime\.LoadScript\(\(ByteString\)new byte\[\] \{ 0x41, 0x1A, 0xF7, 0x7B, 0x67 \}, CallFlags\.All, new object\[\] \{ version \}\)/);
  assert.match(csharp, /Runtime\.LoadScript\(\(ByteString\)new byte\[\] \{ 0x41, 0x2E, 0xDB, 0xBC, 0x93 \}/);
  assert.match(csharp, /Runtime\.LoadScript\(\(ByteString\)new byte\[\] \{ 0x41, 0x44, 0xA1, 0x5D, 0x16 \}/);
  assert.match(csharp, /Runtime\.LoadScript\(\(ByteString\)new byte\[\] \{ 0x41, 0x95, 0x01, 0x6F, 0x61 \}, CallFlags\.All, new object\[\] \{ "event", state \}\)/);
  assert.doesNotMatch(csharp, /Contract\.CallNative|Runtime\.Notify|syscall\("System\.Contract\.Native/);
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

test("C# rendering preserves typed array element types for mutations", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn mutate(size, value) {",
    '    let items = new_array_t(size, "integer");',
    "    append(items, value);",
    "    remove_item(items, 0);",
    "    pop_item(items);",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: true });
  assert.match(csharp, /List<BigInteger>\)items\)\.Add\(@value\)/);
  assert.match(csharp, /List<BigInteger>\)items\)\.RemoveAt/);
  assert.match(csharp, /List<BigInteger>\)items\)\.PopItem/);
  assert.doesNotMatch(csharp, /List<object>\)items\)/);
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

test("C# rendering lowers the NEWSTRUCT high-level spelling", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn build(value) -> any {",
    "    let structure = new_struct(value);",
    "    return structure;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /new object\[\] \{ @value \}/);
  assert.doesNotMatch(csharp, /new_struct\(/);
});

test("C# rendering lowers reverse item operations for array-like values", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn reverse(items) -> any {",
    "    reverse_items(items);",
    "    return items;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /Helper\.Reverse\(items\)/);
  assert.doesNotMatch(csharp, /reverse_items\(/);
});

test("C# rendering lowers an empty VM struct to a framework-compatible array", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn build() -> any {",
    "    let value = Struct();",
    "    return value;",
    "}",
    "}",
  ].join("\n"), null, { typedDeclarations: false });
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
  ].join("\n"), null, { typedDeclarations: false });
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
  assert.match(csharp, /ExecutionEngine\.Assert\(\(bool\)\(object\)\(\(\(dynamic\)\(@value\)\) > 0\)\);/);
  assert.match(csharp, /if \(!\(bool\)\(object\)\(@value\)\) throw new InvalidOperationException/);
  assert.match(csharp, /throw new Exception\(Convert\.ToString\(@value\)\);/);
  assert.doesNotMatch(csharp, /assert\(/);
  assert.doesNotMatch(csharp, /throw\(value\)/);
});

test("C# rendering guards non-void VM fallthrough after structured blocks", () => {
  const csharp = renderCSharpContract([
    "contract Fallthrough {",
    "    fn abortInFinally(flag: bool) -> int {",
    "        try {",
    "            let result = 1;",
    "        } catch {",
    "            let result = 2;",
    "        }",
    "    }",
    "    fn run() {",
    "        let result = 1;",
    "    }",
    "}",
  ].join("\n"));
  assert.match(csharp, /public static BigInteger abortInFinally\(bool flag\)/);
  assert.match(csharp, /\/\/ unreachable VM fallthrough\n\s+throw new InvalidOperationException\("Unreachable Neo VM fallthrough\."\);/);
  assert.doesNotMatch(csharp, /public static void run\(\)[\s\S]*Unreachable Neo VM fallthrough/);
});

test("C# rendering converts value-less returns only in non-void methods", () => {
  const csharp = renderCSharpContract([
    "contract ValueLessReturn {",
    "    fn value() -> bool {",
    "        return;",
    "    }",
    "    fn run() {",
    "        return;",
    "    }",
    "}",
  ].join("\n"));
  assert.match(csharp, /public static bool @value\(\)[\s\S]*throw new InvalidOperationException\("Unreachable Neo VM fallthrough\."\);/);
  assert.match(csharp, /public static void run\(\)[\s\S]*return;/);
});

test("C# rendering lowers native qualified calls outside literals", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn call() -> any {",
    "    let value = GasToken::Transfer(from, to, amount);",
    "    let gasSymbol = GasToken::Symbol();",
    "    let neoDecimals = NeoToken::Decimals();",
    "    let blockHash = LedgerContract::CurrentHash();",
    "    let blockIndex = LedgerContract::CurrentIndex();",
    "    let price = OracleContract::GetPrice(url);",
    "    let blocked = PolicyContract::IsBlocked(account);",
    '    return "GasToken::Transfer(x)";',
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /GasToken\.Transfer\(@from, to, amount\)/);
  assert.match(csharp, /GasToken\.Symbol;/);
  assert.match(csharp, /NeoToken\.Decimals;/);
  assert.match(csharp, /LedgerContract\.CurrentHash;/);
  assert.match(csharp, /LedgerContract\.CurrentIndex;/);
  assert.match(csharp, /OracleContract\.GetPrice\(url\)/);
  assert.match(csharp, /PolicyContract\.IsBlocked\(account\)/);
  assert.doesNotMatch(
    csharp,
    /(?:GasToken\.Symbol|NeoToken\.Decimals|LedgerContract\.CurrentHash|LedgerContract\.CurrentIndex)\(\)/,
  );
  assert.match(csharp, /GasToken::Transfer\(x\)/);
});

test("C# rendering keeps inferred VM helpers private", () => {
  const csharp = renderCSharpContract([
    "contract Token {",
    "fn main() -> any {",
    "    return sub_0x0010();",
    "}",
    "fn sub_0x0010(arg0) -> any {",
    "    return arg0 + 1;",
    "}",
    "}",
  ].join("\n"));
  assert.match(csharp, /public static object main\(\)/);
  assert.match(csharp, /private static dynamic sub_0x0010\(dynamic arg0\)/);
  assert.doesNotMatch(csharp, /public static (?:object|dynamic) sub_0x0010\(dynamic arg0\)/);
});

test("C# rendering declares referenced static VM slots", () => {
  const csharp = renderCSharpContract([
    "contract StaticSlots {",
    "    fn read() -> any {",
    "        return static0;",
    "    }",
    "    fn write() {",
    "        let static1 = 1;",
    "        static0 = static1;",
    "        return;",
    "    }",
    "}",
  ].join("\n"));

  assert.match(csharp, /private static BigInteger __neoStatic0;/);
  assert.match(csharp, /private static BigInteger __neoStatic1;/);
  assert.match(csharp, /__neoStatic0 = __neoStatic1;/);
  assert.match(csharp, /return __neoStatic0;/);
  assert.doesNotMatch(csharp, /\bstatic[01]\b/);
});

test("C# static slot inference stays dynamic for conflicting writes", () => {
  const csharp = renderCSharpContract([
    "contract ConflictingStaticSlots {",
    "    fn write() {",
    "        let static0 = 1;",
    "        static0 = \"later\";",
    "        return;",
    "    }",
    "}",
  ].join("\n"));

  assert.match(csharp, /private static dynamic __neoStatic0;/);
});
