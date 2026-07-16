import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  analyzeBytes,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
} from "../src/index.js";
import {
  buildNefFromScript,
  buildNefWithSingleToken,
} from "./decompiler-fixtures.mjs";

test("DUP on a call result materialises a temp instead of double-evaluating", () => {
  // SYSCALL System.Runtime.GetTime; DUP; DROP; DROP; RET. The second
  // DUP'd reference used to render as a second call, which is
  // observably wrong because syscalls have side effects (or simply
  // expensive to re-evaluate). Both DUP'd consumers now resolve to
  // the same `let tN = syscall(...);` materialisation.
  const script = new Uint8Array([
    0x41, 0xB7, 0xC3, 0x88, 0x03, // SYSCALL System.Runtime.GetTime (0x0388C3B7)
    0x4A,                          // DUP
    0x75, 0x75,                    // DROP DROP
    0x40,                          // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // Exactly one syscall emission in the high-level lift.
  const occurrences = (result.highLevel.match(/syscall\("System\.Runtime\.GetTime"\)/g) ?? []).length;
  assert.equal(
    occurrences,
    1,
    `DUP must not duplicate the syscall expression: ${result.highLevel}`,
  );
});

test("unknown syscall hash surfaces a `// warning: unknown syscall` annotation", () => {
  // SYSCALL with a hash that isn't in the bundled table. JS used to
  // render a bare `syscall(0xHASH)` with no hint that the hash was
  // unrecognised — Rust emits `// unknown syscall` so the user can
  // tell the call won't have a name. Both ports now agree.
  const script = new Uint8Array([
    0x41, 0xEF, 0xBE, 0xAD, 0xDE, // SYSCALL 0xDEADBEEF (unknown)
    0x40,                          // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /\/\/ warning: unknown syscall 0xDEADBEEF/,
    "unknown-syscall warning should appear before the call: ${result.highLevel}",
  );
  assert.match(
    result.csharp,
    /Runtime\.LoadScript\(\(ByteString\)new byte\[\] \{ 0x41, 0xEF, 0xBE, 0xAD, 0xDE \}, CallFlags\.All, new object\[\] \{  \}\)/,
    `unknown syscall hash should use a C# compatibility call: ${result.csharp}`,
  );
  assert.doesNotMatch(result.csharp, /syscall\(0xDEADBEEF\)/);
});

test("NEWARRAY / NEWBUFFER / NEWSTRUCT materialise a temp before mutation", () => {
  // Same DUP-duplicates-the-expression hazard as NEWMAP/NEWARRAY0:
  // pushing the call expression onto the operand stack means
  // `NEWARRAY DUP "k" 8 SETITEM RET` rendered as
  // `new_array(3)[0] = 8; return new_array(3);` (two independent
  // allocations). Now both DUP'd references resolve to the same temp.
  const script = new Uint8Array([
    0x13,       // PUSH3
    0xC3,       // NEWARRAY
    0x4A,       // DUP
    0x10,       // PUSH0
    0x18,       // PUSH8
    0xD0,       // SETITEM
    0x40,       // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /let t0 = new_array\(3\);\s*t0\[0\] = 8;\s*return t0;/,
    "NEWARRAY result should be materialised once and reused: ${result.highLevel}",
  );
  assert.doesNotMatch(
    result.highLevel,
    /new_array\(3\)\[0\] = 8;\s*return new_array\(3\);/,
    "no double-allocation pattern should appear",
  );
});

test("INITSLOT method does not double-stack the inferred arg labels", () => {
  // Without a manifest, INITSLOT 0/1 declares one inferred arg. The
  // bytecode pops the arg off the stack into the arg slot at INITSLOT
  // time, so the lifter must NOT pre-populate the operand stack with
  // `arg0` — otherwise a method that compares `arg0 <= N` and returns
  // bubbles a phantom `arg0` past the RET into the surrounding scope
  // as a bare-expression statement (`return 1; arg0;`).
  const script = new Uint8Array([
    0x57, 0x00, 0x01, // INITSLOT 0 locals, 1 arg
    0x78,             // LDARG0
    0x1A,             // PUSH10
    0xB6,             // LE
    0x26, 0x04,       // JMPIFNOT +4
    0x11, 0x40,       // PUSH1; RET
    0x12, 0x40,       // PUSH2; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /if arg0 <= 10 \{\s*return 1;\s*\}\s*else \{\s*return 2;\s*\}/,
    "INITSLOT lift should not leak phantom arg0 onto the stack: ${result.highLevel}",
  );
  assert.doesNotMatch(
    result.highLevel,
    /^\s*arg0;\s*$/m,
    "no bare-expression `arg0;` statement should leak into the body",
  );
});

test("PUSHINT128 / PUSHINT256 decode as decimal literals", () => {
  // 16 bytes little-endian for 0x123 — used to render as
  // `// 0000: PUSHINT128 0x...(not yet translated)`.
  const positiveScript = new Uint8Array(18);
  positiveScript[0] = 0x04; // PUSHINT128
  // Encode 0x123 LE in 16 bytes.
  positiveScript[1] = 0x23;
  positiveScript[2] = 0x01;
  positiveScript[17] = 0x40; // RET
  const positive = decompileHighLevelBytes(buildNefFromScript(positiveScript));
  assert.match(positive.highLevel, /return 291;/);

  // 32-byte all-0xff payload encodes -1 in two's complement.
  const negScript = new Uint8Array(34);
  negScript[0] = 0x05; // PUSHINT256
  for (let i = 1; i < 33; i++) negScript[i] = 0xff;
  negScript[33] = 0x40; // RET
  const negative = decompileHighLevelBytes(buildNefFromScript(negScript));
  assert.match(negative.highLevel, /return -1;/);
});

test("PUSHDATA bytes decode as quoted string when printable ASCII", () => {
  // 0x0c 0x03 'k' 'e' 'y' = PUSHDATA1 [0x6b 0x65 0x79]; followed by RET.
  // Rust decodes the bytes as "key"; JS used to print the raw hex form.
  // Now both ports render `return "key";` for printable payloads.
  const script = new Uint8Array([0x0c, 0x03, 0x6b, 0x65, 0x79, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /return "key";/,
    "printable PUSHDATA payload should render as quoted string",
  );
  assert.doesNotMatch(
    result.highLevel,
    /0x6B6579/,
    "raw hex form should not appear when ASCII decode succeeded",
  );
});

test("PUSHDATA bytes stay as hex when payload is non-printable", () => {
  // 0x0c 0x02 0x00 0xff = PUSHDATA1 with NUL + 0xff (non-printable); RET.
  const script = new Uint8Array([0x0c, 0x02, 0x00, 0xff, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /return 0x00FF;/,
    "binary PUSHDATA should keep the unambiguous hex form",
  );
});

test("emits method-token header with native-contract description", () => {
  // Build a NEF whose method tokens table contains a single entry pointing
  // at StdLib::Serialize. The header should label it `(StdLib::Serialize)`,
  // include the hash / param count / return / call-flag breakdown, and use
  // `|`-joined flag labels — matching the Rust contract header writer.
  const stdlibHash = new Uint8Array([
    0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2, 0xA0, 0x6A,
    0x79, 0xE1, 0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
  ]);
  // Compose the NEF by hand: PUSH1; RET script with a single token row.
  const tokenRow = [];
  for (const b of stdlibHash) tokenRow.push(b);
  // method name (varint length 9 + ASCII "Serialize")
  tokenRow.push(9);
  for (const c of "Serialize") tokenRow.push(c.charCodeAt(0));
  tokenRow.push(0x01, 0x00); // parameters_count = 1 (LE u16)
  tokenRow.push(0x01); // has_return_value = true
  tokenRow.push(0x0F); // call_flags = ReadStates|WriteStates|AllowCall|AllowNotify
  const data = [];
  for (const c of "NEF3") data.push(c.charCodeAt(0));
  for (let c = 0; c < 64; c++) data.push(0); // compiler
  data.push(0); // source varint
  data.push(0); // reserved
  data.push(1); // method tokens count varint = 1
  for (const b of tokenRow) data.push(b);
  data.push(0, 0); // reserved word
  const script = [0x11, 0x40]; // PUSH1; RET
  data.push(script.length);
  for (const b of script) data.push(b);
  // checksum
  const arr = new Uint8Array(data);
  const h1 = createHash("sha256").update(arr).digest();
  const h2 = createHash("sha256").update(h1).digest();
  const out = Buffer.concat([Buffer.from(arr), h2.subarray(0, 4)]);

  const result = decompileHighLevelBytes(out);
  assert.match(
    result.highLevel,
    /\/\/ method tokens declared in NEF/,
    "header section label should be present",
  );
  assert.match(
    result.highLevel,
    /\/\/ Serialize \(StdLib::Serialize\) hash=C0EF39CEE0E4E925C6C2A06A79E1440DD86FCEAC params=1 returns=true flags=0x0F \(ReadStates\|WriteStates\|AllowCall\|AllowNotify\)/,
    "method-token line must include native-contract label and full metadata",
  );
});

test("ABI summary shows `-> void` for void methods (Rust parity)", () => {
  // The lifted body of a void method idiomatically omits `-> void`, but
  // the ABI surface header should be fully explicit so the manifest
  // contract is unambiguous. Matches the Rust `manifest_summary` writer.
  const script = new Uint8Array([0x40]); // RET
  const manifest = JSON.stringify({
    name: "OnlyVoid",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0 },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(
    buildNefFromScript(script),
    manifest,
  );
  assert.match(
    result.highLevel,
    /fn main\(\) -> void; \/\/ offset 0/,
    "ABI declaration should show `-> void`",
  );
  // Body signature stays idiomatic — no `-> void`.
  assert.match(
    result.highLevel,
    /fn main\(\) \{/,
    "lifted body keeps the conventional bare `fn main() {{`",
  );
});

test("drops inferred helpers whose body decodes to nothing (NOP padding)", () => {
  // Two manifest-aligned methods separated by a run of NOPs the compiler
  // emits as padding. Previously the JS port treated the post-RET NOPs as
  // a third "method" and rendered `fn sub_0x0002() { // no instructions
  // decoded }`. The Rust port already skips these; this test pins the JS
  // port to the same behaviour.
  const script = new Uint8Array([
    0x11, 0x40, // 0x0000: PUSH1; RET (main)
    0x21, 0x21, 0x21, 0x21, // 0x0002-0x0005: NOP padding
    0x11, 0x11, 0x9e, 0x40, // 0x0006: PUSH1; PUSH1; ADD; RET (helper)
  ]);
  const manifest = JSON.stringify({
    name: "PaddedMulti",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
        { name: "helper", parameters: [], returntype: "Integer", offset: 6 },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  const result = decompileHighLevelBytesWithManifest(
    buildNefFromScript(script),
    manifest,
  );
  assert.doesNotMatch(
    result.highLevel,
    /fn sub_0x0002\(\)/,
    "NOP padding between methods should not surface as a helper",
  );
  assert.doesNotMatch(
    result.highLevel,
    /\/\/ no instructions decoded/,
    "no method should render the empty-body placeholder",
  );
  assert.match(result.highLevel, /fn main\(\) -> int \{/);
  assert.match(result.highLevel, /fn helper\(\) -> int \{/);
});

test("emits fallthrough call statements instead of empty method bodies", () => {
  const script = new Uint8Array([0x34, 0x02, 0x34, 0x00, 0x40, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.doesNotMatch(
    result.highLevel,
    /fn script_entry\(\) \{\n\s*\/\/ no instructions decoded/,
  );
  assert.match(
    result.highLevel,
    /fn script_entry\(\) -> any \{\n\s*sub_0x0002\(\);/,
  );
});

test("uses method token metadata to lift CALLT arguments and void returns", () => {
  const nef = buildNefWithSingleToken(
    new Uint8Array([0x11, 0x37, 0x00, 0x00, 0x40]),
    new Uint8Array(20),
    "foo",
    1,
    false,
    0x0f,
  );
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /foo\(1\);/);
  assert.doesNotMatch(result.highLevel, /let t\d+ = foo/);
  assert.match(result.highLevel, /return;/);
});

test("preserves a value through a suffix branch chain before CALLT", () => {
  // Each matching branch returns immediately; the fallthrough chain keeps the
  // original argument for the final one-argument token call. Linear stack
  // replay used to consume that value while simulating the nested branches,
  // leaving `foo(???)` in the generated contract.
  const script = new Uint8Array([
    0x57, 0x00, 0x01, // INITSLOT 0 locals, 1 arg
    0x78, // LDARG0
    0x4a, 0x11, 0x97, // DUP; PUSH1; EQUAL
    0x26, 0x05, // JMPIFNOT -> second comparison
    0x75, 0x11, 0x40, // DROP; PUSH1; RET
    0x4a, 0x12, 0x97, // DUP; PUSH2; EQUAL
    0x26, 0x05, // JMPIFNOT -> token call
    0x75, 0x12, 0x40, // DROP; PUSH2; RET
    0x37, 0x00, 0x00, // CALLT token 0 (one argument)
    0x40, // RET
  ]);
  const nef = buildNefWithSingleToken(
    script,
    new Uint8Array(20),
    "foo",
    1,
    true,
    0x0f,
  );
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /foo\(arg0\)/);
  assert.doesNotMatch(result.highLevel, /\?\?\?/);
  assert.equal(result.warnings.length, 0);
});

test("keeps restricted native CALLT labels unqualified", () => {
  const nef = buildNefWithSingleToken(
    new Uint8Array([0x11, 0x37, 0x00, 0x00, 0x40]),
    Uint8Array.from([
      0xC0, 0xEF, 0x39, 0xCE, 0xE0, 0xE4, 0xE9, 0x25, 0xC6, 0xC2,
      0xA0, 0x6A, 0x79, 0xE1, 0x44, 0x0D, 0xD8, 0x6F, 0xCE, 0xAC,
    ]),
    "Serialize",
    1,
    true,
    0x01,
  );
  const result = decompileHighLevelBytes(nef);
  assert.doesNotMatch(result.highLevel, /return StdLib::Serialize/);
  assert.match(result.highLevel, /Serialize\(1\)/);
});

test("uses call-graph-resolved CALLA targets across method arguments", () => {
  // main passes &helper into invoke; invoke loads that argument and CALLA uses
  // it. Method-local pointer maps cannot recover this provenance, but the call
  // graph's interprocedural argument pass resolves CALLA@13 to helper@15.
  const script = new Uint8Array([
    0x0a, 0x0f, 0x00, 0x00, 0x00, // 0x00 PUSHA helper@15
    0x34, 0x04, // 0x05 CALL invoke@9
    0x40, // 0x07 RET
    0x21, // 0x08 NOP padding
    0x57, 0x00, 0x01, // 0x09 INITSLOT 0 locals, 1 arg
    0x78, // 0x0C LDARG0
    0x36, // 0x0D CALLA
    0x40, // 0x0E RET
    0x40, // 0x0F helper: RET
  ]);
  const manifest = JSON.stringify({
    name: "IndirectVoid",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0 },
        {
          name: "invoke",
          parameters: [{ name: "target", type: "Any" }],
          returntype: "Void",
          offset: 9,
        },
        { name: "helper", parameters: [], returntype: "Void", offset: 15 },
      ],
      events: [],
    },
  });
  const nef = buildNefFromScript(script);

  const analysis = analyzeBytes(nef, manifest);
  const calla = analysis.callGraph.edges.find((edge) => edge.callOffset === 13);
  assert.equal(calla?.target.kind, "Internal");
  assert.equal(calla?.target.method.offset, 15);

  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  assert.match(
    highLevel,
    /fn invoke\(target: any\) \{\s+helper\(\);\s+return;/,
    `resolved void CALLA should remain visible in invoke: ${highLevel}`,
  );
  assert.doesNotMatch(highLevel, /= helper\(\)/);
});

test("does not resolve CALLA through a value-returning internal call", () => {
  // The helper pointer remains below value()'s result. CALLA consumes the
  // result, not the stale pointer, so analysis must keep the target indirect.
  const script = new Uint8Array([
    0x0a, 0x0c, 0x00, 0x00, 0x00, // 0x00 PUSHA helper@12
    0x34, 0x04, // 0x05 CALL value@9
    0x36, // 0x07 CALLA (consumes value() result)
    0x40, // 0x08 RET
    0x11, 0x40, // 0x09 value: PUSH1; RET
    0x21, // 0x0B NOP padding
    0x40, // 0x0C helper: RET
  ]);
  const manifest = JSON.stringify({
    name: "IndirectValue",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0 },
        { name: "value", parameters: [], returntype: "Integer", offset: 9 },
        { name: "helper", parameters: [], returntype: "Void", offset: 12 },
      ],
      events: [],
    },
  });
  const nef = buildNefFromScript(script);

  const analysis = analyzeBytes(nef, manifest);
  const calla = analysis.callGraph.edges.find((edge) => edge.callOffset === 7);
  assert.equal(calla?.target.kind, "Indirect");

  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  assert.doesNotMatch(
    highLevel,
    /^\s*helper\(\);$/m,
    `CALLA must not consume the stale pointer below value()'s result: ${highLevel}`,
  );
});

test("does not resolve CALLA through unmodeled value producers", () => {
  const cases = [
    {
      name: "unknown CALLT",
      script: new Uint8Array([
        0x0a, 0x0a, 0x00, 0x00, 0x00, // PUSHA helper@10
        0x37, 0x00, 0x00, // CALLT token 0 (token table is empty)
        0x36, 0x40, // CALLA; RET
        0x40, // helper@10: RET
      ]),
      helperOffset: 10,
      callaOffset: 8,
    },
    {
      name: "NEWMAP",
      script: new Uint8Array([
        0x0a, 0x08, 0x00, 0x00, 0x00, // PUSHA helper@8
        0xc8, // NEWMAP
        0x36, 0x40, // CALLA; RET
        0x40, // helper@8: RET
      ]),
      helperOffset: 8,
      callaOffset: 6,
    },
  ];

  for (const { name, script, helperOffset, callaOffset } of cases) {
    const manifest = JSON.stringify({
      name: "IndirectUnknown",
      abi: {
        methods: [
          { name: "main", parameters: [], returntype: "Void", offset: 0 },
          {
            name: "helper",
            parameters: [],
            returntype: "Void",
            offset: helperOffset,
          },
        ],
        events: [],
      },
    });
    const nef = buildNefFromScript(script);
    const analysis = analyzeBytes(nef, manifest);
    const calla = analysis.callGraph.edges.find((edge) => edge.callOffset === callaOffset);
    assert.equal(calla?.target.kind, "Indirect", `${name} must hide the stale pointer`);

    const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
    assert.doesNotMatch(
      highLevel,
      /^\s*helper\(\);$/m,
      `${name} must not fabricate a helper call: ${highLevel}`,
    );
  }
});

test("infers entry-stack arguments for syscall-only helpers", () => {
  const script = new Uint8Array([
    0x0c, 0x01, 0x78, // PUSHDATA1 "x"
    0x34, 0x03, // CALL +3
    0x40, // RET
    0x41, 0xcf, 0xe7, 0x47, 0x96, // SYSCALL System.Runtime.Log
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /fn sub_0x0006\(arg0\) \{/);
  assert.match(result.highLevel, /syscall\("System\.Runtime\.Log", arg0\);/);
});

test("method contracts classify a private void helper and preserve the caller return", () => {
  const script = new Uint8Array([
    0x19, 0x11, 0x34, 0x05, 0x40, 0x21, 0x21,
    0x57, 0x00, 0x01, 0x78, 0x45, 0x40,
  ]);
  const manifest = JSON.stringify({
    name: "InferredVoidHelper",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
      ],
      events: [],
    },
  });
  const nef = buildNefFromScript(script);
  const analysis = analyzeBytes(nef, manifest);

  assert.deepEqual(analysis.methodContracts.methods, [
    {
      method: { offset: 0, name: "main" },
      argumentCount: 0,
      returnBehavior: "value",
    },
    {
      method: { offset: 7, name: "sub_0x0007" },
      argumentCount: 1,
      returnBehavior: "void",
    },
  ]);

  const result = decompileHighLevelBytesWithManifest(nef, manifest, {
    inlineSingleUseTemps: true,
  });
  assert.deepEqual(result.methodContracts, analysis.methodContracts);
  assert.match(result.highLevel, /sub_0x0007\(1\);/);
  assert.match(result.highLevel, /return 9;/);
  assert.doesNotMatch(result.highLevel, /return sub_0x0007\(1\)/);
  assert.match(result.highLevel, /fn sub_0x0007\(arg0\) \{/);
});

test("method contracts keep exception-bearing helper returns conservative", () => {
  const script = new Uint8Array([
    0x34, 0x03, // CALL helper at offset 3
    0x40, // RET
    0x3b, 0x00, 0x05, // TRY with finally at offset 8
    0x11, // PUSH1
    0x3d, 0x02, // ENDTRY -> RET after ENDFINALLY
    0x3f, // ENDFINALLY
    0x40, // RET
  ]);
  const manifest = JSON.stringify({
    name: "TryValueHelper",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
      ],
      events: [],
    },
  });
  const result = decompileHighLevelBytesWithManifest(buildNefFromScript(script), manifest);
  assert.equal(
    result.methodContracts.methods.find(({ method }) => method.offset === 3)
      ?.returnBehavior,
    "unknown",
  );
  assert.match(result.highLevel, /return sub_0x0003\(\);/);
  assert.doesNotMatch(result.highLevel, /return \?\?\?;/);
});

test("method contracts converge through a private void wrapper chain", () => {
  const nef = buildNefFromScript(
    new Uint8Array([0x19, 0x34, 0x03, 0x40, 0x34, 0x03, 0x40, 0x40]),
  );
  const manifest = JSON.stringify({
    name: "VoidWrapperChain",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
      ],
      events: [],
    },
  });

  const result = decompileHighLevelBytesWithManifest(nef, manifest, {
    inlineSingleUseTemps: true,
  });

  assert.deepEqual(
    result.methodContracts.methods.map(({ method, returnBehavior }) => [
      method.offset,
      returnBehavior,
    ]),
    [[0, "value"], [4, "void"], [7, "void"]],
  );
  assert.match(result.highLevel, /sub_0x0004\(\);/);
  assert.match(result.highLevel, /sub_0x0007\(\);/);
  assert.match(result.highLevel, /return 9;/);
});

test("method contracts keep recursive, mixed, and missing private returns unknown", () => {
  const manifest = JSON.stringify({
    name: "AmbiguousHelpers",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
      ],
      events: [],
    },
  });
  const recursive = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([0x19, 0x34, 0x03, 0x40, 0x34, 0x00, 0x40]),
    ),
    manifest,
  );
  const mixed = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x34, 0x06, 0x40, 0x21, 0x21, 0x21,
        0x11, 0x26, 0x04, 0x11, 0x40, 0x40,
      ]),
    ),
    manifest,
  );
  const missing = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x34, 0x04, 0x40, 0x21, 0x38])),
    manifest,
  );

  assert.equal(
    recursive.methodContracts.methods.find(({ method }) => method.offset === 4)
      ?.returnBehavior,
    "unknown",
  );
  assert.equal(
    mixed.methodContracts.methods.find(({ method }) => method.offset === 6)
      ?.returnBehavior,
    "unknown",
  );
  assert.equal(
    missing.methodContracts.methods.find(({ method }) => method.offset === 4)
      ?.returnBehavior,
    "unknown",
  );
});

test("method contracts keep manifest return declarations authoritative", () => {
  const script = new Uint8Array([0x34, 0x04, 0x40, 0x21, 0x11, 0x40]);
  const nef = buildNefFromScript(script);
  const withHelperReturn = (returntype) =>
    JSON.stringify({
      name: "DeclaredHelper",
      abi: {
        methods: [
          { name: "main", parameters: [], returntype: "Void", offset: 0 },
          { name: "helper", parameters: [], returntype, offset: 4 },
        ],
        events: [],
      },
    });

  const value = analyzeBytes(nef, withHelperReturn("Integer"));
  const voidResult = analyzeBytes(nef, withHelperReturn("Void"));

  assert.equal(
    value.methodContracts.methods.find(({ method }) => method.offset === 4)
      ?.returnBehavior,
    "value",
  );
  assert.equal(
    voidResult.methodContracts.methods.find(({ method }) => method.offset === 4)
      ?.returnBehavior,
    "void",
  );
});

test("unknown private method contracts remain conservatively value-producing", () => {
  const nef = buildNefFromScript(
    new Uint8Array([0x34, 0x04, 0x40, 0x21, 0x11, 0x40]),
  );
  const manifest = JSON.stringify({
    name: "UnknownValueHelper",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Integer", offset: 0 },
      ],
      events: [],
    },
  });

  const result = decompileHighLevelBytesWithManifest(nef, manifest, {
    inlineSingleUseTemps: true,
  });

  assert.equal(
    result.methodContracts.methods.find(({ method }) => method.offset === 4)
      ?.returnBehavior,
    "unknown",
  );
  assert.match(result.highLevel, /fn sub_0x0004\(\) -> any \{/);
  assert.match(result.highLevel, /return sub_0x0004\(\);/);
});
