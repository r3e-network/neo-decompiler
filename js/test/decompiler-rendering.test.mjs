import assert from "node:assert/strict";
import test from "node:test";

import {
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
} from "../src/index.js";
import { buildNefFromScript } from "./decompiler-fixtures.mjs";

test("structured trusts {hashes:[...], groups:[...]} renders as a typed list", async () => {
  const { decompileHighLevelBytesWithManifest } = await import("../src/index.js");
  const script = new Uint8Array([0x40]); // RET
  const nef = buildNefFromScript(script);
  const manifest = JSON.stringify({
    name: "TrustsObject",
    groups: [],
    features: { storage: false, payable: false },
    supportedstandards: [],
    abi: { methods: [{ name: "main", parameters: [], returntype: "Void", offset: 0, safe: false }], events: [] },
    permissions: [],
    trusts: { hashes: ["0xabc"], groups: ["02def"] },
    extra: {},
  });
  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  assert.match(highLevel, /trusts = \[hash:0xabc, group:02def\];/);
  assert.ok(
    !highLevel.includes('{"hashes"'),
    "structured trusts must not fall back to JSON.stringify when both keys are well-formed",
  );
});

test("structured trusts with unexpected key falls back to JSON output", async () => {
  const { decompileHighLevelBytesWithManifest } = await import("../src/index.js");
  const script = new Uint8Array([0x40]); // RET
  const nef = buildNefFromScript(script);
  const manifest = JSON.stringify({
    name: "TrustsUnknown",
    groups: [],
    features: { storage: false, payable: false },
    supportedstandards: [],
    abi: { methods: [{ name: "main", parameters: [], returntype: "Void", offset: 0, safe: false }], events: [] },
    permissions: [],
    trusts: { groups: ["02def"], unexpected: 1 },
    extra: {},
  });
  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  // Unknown key means we don't trust the shape; surface raw JSON so
  // the user sees the anomaly instead of silently dropping fields.
  assert.match(highLevel, /trusts = \{.*unexpected.*\};/);
});

test("formatManifestType normalises known kinds regardless of case and preserves unknown kinds verbatim", async () => {
  const { formatManifestType } = await import("../src/manifest.js");
  // Known kinds normalise to the high-level vocabulary, irrespective
  // of the manifest's source casing.
  assert.equal(formatManifestType("Integer"), "int");
  assert.equal(formatManifestType("integer"), "int");
  assert.equal(formatManifestType("INTEGER"), "int");
  assert.equal(formatManifestType("Boolean"), "bool");
  assert.equal(formatManifestType("Hash160"), "hash160");
  assert.equal(formatManifestType("PublicKey"), "publickey");
  assert.equal(formatManifestType("InteropInterface"), "interop");
  // Unknown kinds round-trip the original input — preserving the
  // user's chosen casing/spelling. Mirrors the Rust port's
  // `format_manifest_type` behaviour.
  assert.equal(formatManifestType("MyCustomType"), "MyCustomType");
  assert.equal(formatManifestType("Foo_Bar"), "Foo_Bar");
  assert.equal(formatManifestType(""), "");
});

test("PUSHA pushes function-pointer expression `&fn_0xNNNN` (Rust parity)", async () => {
  const { decompileHighLevelBytes } = await import("../src/index.js");
  // PUSHA +0x7B pushes a function pointer to absolute offset 0x007C.
  // Rust's `resolve_pusha_display` formats it as `&fn_0x007C` (or
  // `&{label}` when the target is a known method); JS used to push
  // the bare integer (`123`), losing the function-pointer semantics
  // and rendering as if it were a PUSHINT.
  const script = new Uint8Array([0x0a, 0x7b, 0x00, 0x00, 0x00, 0x40]);
  const nef = buildNefFromScript(script);
  const { highLevel } = decompileHighLevelBytes(nef, { clean: true });
  // Target is 0x0001 (after the PUSHA opcode byte) + 0x7B = 0x007C.
  // Wait — Rust uses `instruction.offset + delta`; PUSHA is at offset
  // 0x0000, delta is 0x7B, so target is 0x007B. Match Rust output.
  assert.match(highLevel, /return &fn_0x007B;/);
});

test("inferred helper labels use uppercase hex (Rust `sub_0x{:04X}` parity)", async () => {
  const { decompileHighLevelBytes } = await import("../src/index.js");
  // Build a NEF with seven RETs after the 4-byte entry, putting a sub
  // helper at offset 0x000B — an offset that contains the hex letter
  // `B`. Earlier `methods.js` built the label with
  // `.toString(16).padStart(4, "0")`, lowercasing the letter and
  // producing `sub_0x000b`, while Rust's `format!("sub_0x{:04X}")`
  // emits `sub_0x000B`. The differential test never tripped on this
  // because none of the curated artifacts have helpers at offsets
  // ≥0x000A.
  const script = new Uint8Array([
    0x10, 0x11, 0x9e, 0x40,                         // entry: PUSH0 PUSH1 ADD RET
    0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40,       // padding RETs at 0x04..0x0A
    0x57, 0x00, 0x00, 0x12, 0x40,                    // sub at 0x0B: INITSLOT 0,0; PUSH2; RET
  ]);
  const nef = buildNefFromScript(script);
  const { highLevel } = decompileHighLevelBytes(nef, { clean: true });
  assert.match(highLevel, /fn sub_0x000A\(/);
  assert.match(highLevel, /fn sub_0x000B\(/);
  assert.ok(!/sub_0x000a/.test(highLevel), "lowercase hex should not appear");
});

test("formatOperand renders syscall as `Name (0xHASH)` for known syscalls (Rust parity)", async () => {
  const { formatOperand } = await import("../src/disassembler.js");
  // 0xCE67F69B is System.Storage.GetContext — well-known syscall.
  // Earlier JS rendered every syscall as bare `0xHASH`, while Rust
  // prefixes the resolved name when available. Verify JS now matches.
  const known = formatOperand({ kind: "Syscall", value: 0xce67f69b });
  assert.match(known, /^System\.Storage\.GetContext \(0xCE67F69B\)$/);
  // Unknown / reserved hashes still fall back to bare hex.
  const unknown = formatOperand({ kind: "Syscall", value: 0xdeadbeef });
  assert.equal(unknown, "0xDEADBEEF");
});

test("extractContractName mirrors Rust extract_contract_name fallback to NeoContract", async () => {
  const { extractContractName } = await import("../src/manifest.js");
  // Live manifest with a usable name → sanitised verbatim.
  assert.equal(extractContractName({ name: "Foo" }), "Foo");
  // Manifest with whitespace-only name → fallback.
  assert.equal(extractContractName({ name: "   " }), "NeoContract");
  // Names that sanitise to a non-empty placeholder follow Rust's
  // sanitiser behaviour: characters outside `[A-Za-z0-9_-\s]` are
  // dropped, and a fully-empty result becomes `"param"` (not
  // `"NeoContract"` — that fallback only kicks in when the trimmed
  // input is itself empty).
  assert.equal(extractContractName({ name: "@@@" }), "param");
  // Null / undefined / missing manifest → fallback.
  assert.equal(extractContractName(null), "NeoContract");
  assert.equal(extractContractName(undefined), "NeoContract");
  assert.equal(extractContractName({}), "NeoContract");
  // Hyphenated/spaced names are sanitised to underscores.
  assert.equal(extractContractName({ name: "my-token-v2" }), "my_token_v2");
  assert.equal(extractContractName({ name: "Sample Token" }), "Sample_Token");
});

test("sanitizeIdentifier preserves consecutive underscores and collapses whitespace/dashes (Rust parity)", async () => {
  const { sanitizeIdentifier } = await import("../src/manifest.js");
  // Explicit `_` is always preserved, so leading double/triple
  // underscores stay verbatim — earlier the JS port silently
  // collapsed them to a single `_`, diverging from Rust's
  // `decompiler::helpers::sanitize_identifier`.
  assert.equal(sanitizeIdentifier("__foo"), "__foo");
  assert.equal(sanitizeIdentifier("___bar"), "___bar");
  // Trailing underscores still strip.
  assert.equal(sanitizeIdentifier("_foo_"), "_foo");
  // Whitespace and `-` collapse into a single `_` separator.
  assert.equal(sanitizeIdentifier("foo bar"), "foo_bar");
  assert.equal(sanitizeIdentifier("foo  bar"), "foo_bar");
  assert.equal(sanitizeIdentifier("foo--bar"), "foo_bar");
  // Empty / digit-leading inputs.
  assert.equal(sanitizeIdentifier(""), "param");
  assert.equal(sanitizeIdentifier("9live"), "_9live");
});

test("manifest.groups renders as scannable block with pubkey only (signature elided)", async () => {
  const { decompileHighLevelBytesWithManifest } = await import("../src/index.js");
  const script = new Uint8Array([0x40]);
  const nef = buildNefFromScript(script);
  const manifest = JSON.stringify({
    name: "Signed",
    groups: [
      { pubkey: "02f49ce0c33aabbccdd", signature: "BAt..." },
      { pubkey: "02b00b1eaaaabbbbcccc", signature: "BAd..." },
    ],
    features: { storage: false, payable: false },
    supportedstandards: [],
    abi: { methods: [], events: [] },
    permissions: [],
    trusts: "*",
    extra: {},
  });
  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  assert.match(highLevel, /groups \{/);
  assert.match(highLevel, /pubkey=02f49ce0c33aabbccdd/);
  assert.match(highLevel, /pubkey=02b00b1eaaaabbbbcccc/);
  // Signature is intentionally elided — opaque base64, no human value.
  assert.ok(!highLevel.includes("BAt..."));
  assert.ok(!highLevel.includes("signature="));
});

test("manifest.extra renders string, boolean, and number scalars (drops null/objects/arrays)", async () => {
  const { decompileHighLevelBytesWithManifest } = await import("../src/index.js");
  const script = new Uint8Array([0x40]); // RET
  const nef = buildNefFromScript(script);
  const manifest = JSON.stringify({
    name: "ExtraScalars",
    groups: [],
    features: { storage: false, payable: false },
    supportedstandards: [],
    abi: { methods: [], events: [] },
    permissions: [],
    trusts: "*",
    // Mix of scalar and non-scalar types. Strings, numbers, and
    // booleans should appear in the rendered output verbatim;
    // null/objects/arrays have no canonical short form so they
    // drop out (rather than serialising as `[object Object]` or
    // `null` and confusing readers).
    extra: {
      Author: "Anon",
      Version: 2,
      Verified: true,
      Notes: null,
      Nested: { deep: 1 },
      Tags: ["a", "b"],
    },
  });
  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  assert.match(highLevel, /\/\/ Author: Anon/);
  assert.match(highLevel, /\/\/ Version: 2/);
  assert.match(highLevel, /\/\/ Verified: true/);
  assert.ok(!/\/\/ Notes:/.test(highLevel), "null entry should be dropped");
  assert.ok(!/\/\/ Nested:/.test(highLevel), "nested-object entry should be dropped");
  assert.ok(!/\/\/ Tags:/.test(highLevel), "array entry should be dropped");
});

test("ABI method declaration surfaces `safe` annotation alongside offset", async () => {
  const { decompileHighLevelBytesWithManifest } = await import("../src/index.js");
  const script = new Uint8Array([0x40]); // RET
  const nef = buildNefFromScript(script);
  const manifest = JSON.stringify({
    name: "Safe",
    groups: [],
    features: { storage: false, payable: false },
    supportedstandards: [],
    abi: {
      methods: [
        { name: "balanceOf", parameters: [], returntype: "Integer", offset: 0, safe: true },
        { name: "transfer", parameters: [], returntype: "Void", offset: 1, safe: false },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
    extra: {},
  });
  const { highLevel } = decompileHighLevelBytesWithManifest(nef, manifest);
  // Safe method declaration should expose the `safe` annotation
  // alongside the offset (parity with Rust's manifest summary).
  assert.match(
    highLevel,
    /fn balanceOf\(\) -> int; \/\/ safe, offset 0/,
    `safe annotation should appear before offset: ${highLevel}`,
  );
  // Non-safe methods should NOT include the annotation — it would
  // be redundant noise (the absence of `safe` in the manifest
  // already implies it).
  assert.match(
    highLevel,
    /fn transfer\(\) -> void; \/\/ offset 1$/m,
    `non-safe methods should not include the safe annotation: ${highLevel}`,
  );
});

test("unstructured ENDFINALLY is silently consumed (parity with Rust clean mode)", () => {
  // ENDFINALLY (0x3F) without a wrapping try-block lift — the JS
  // port used to fall through to `renderUntranslatedInstruction`
  // and emit `// XXXX: ENDFINALLY (not yet translated)` plus a
  // structured warning. Rust handles it explicitly (silent in
  // clean mode); JS now matches.
  const script = new Uint8Array([0x3f, 0x40]);
  const { highLevel, warnings } = decompileHighLevelBytes(buildNefFromScript(script));
  assert.doesNotMatch(highLevel, /ENDFINALLY \(not yet translated\)/);
  assert.ok(
    !warnings.some((w) => /ENDFINALLY/i.test(w)),
    `unstructured ENDFINALLY should not surface as an untranslated-opcode warning: ${JSON.stringify(warnings)}`,
  );
});

test("untranslated opcode surfaces a structured warning (not just an inline comment)", () => {
  // 0xFF is reserved/unknown in NEO. The disassembler emits
  // UNKNOWN_0xFF; the high-level lift should flag it both inline
  // and via the `warnings` array so a CI tool iterating warnings
  // catches the hazard.
  const script = new Uint8Array([0xff, 0x40]); // UNKNOWN_0xFF; RET
  const { highLevel, warnings } = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(highLevel, /UNKNOWN_0xFF.*not yet translated/);
  assert.ok(
    warnings.some((w) => /UNKNOWN_0xFF \(not yet translated\)/.test(w)),
    `warnings array should carry the untranslated-opcode hazard: ${JSON.stringify(warnings)}`,
  );
});

test("internal call with insufficient stack values emits `???` placeholder + warning (not `/* stack_underflow */`)", () => {
  // INITSLOT 0,0 ; CALL +3 -> 0x05 ; RET ; INITSLOT 0,2 ; LDARG0 ; LDARG1 ; ADD ; RET
  // The CALL targets a 2-arg helper but the caller has nothing on
  // the stack — both arguments underflow at the lift site.
  // Opcodes: 0x57=INITSLOT, 0x34=CALL (1-byte rel offset),
  // 0x40=RET, 0x78=LDARG0, 0x79=LDARG1, 0x9E=ADD.
  const script = new Uint8Array([
    0x57, 0x00, 0x00,                  // 0x00 INITSLOT 0 locals 0 args
    0x34, 0x03,                        // 0x03 CALL +3 -> 0x06
    0x40,                              // 0x05 RET (pre-pad before the helper)
    0x57, 0x00, 0x02,                  // 0x06 INITSLOT 0 locals 2 args
    0x78, 0x79, 0x9E,                  // 0x09 LDARG0 LDARG1 ADD
    0x40,                              // 0x0C RET
  ]);
  const { highLevel, warnings } = decompileHighLevelBytes(buildNefFromScript(script));
  // The lifted internal call substitutes `???` for every missing
  // argument — never the legacy `/* stack_underflow */` comment.
  assert.match(highLevel, /sub_0x0006\(\?\?\?, \?\?\?\)/);
  // And the structured warnings array carries the hazard so a
  // caller iterating warnings can surface it.
  assert.ok(
    warnings.some((w) => /missing call argument values for sub_0x0006/.test(w)),
    `warnings should include a missing-call-arg entry: ${JSON.stringify(warnings)}`,
  );
});

test("detached PACKSTRUCT helpers infer entry arity and keep C# underflow explicit", () => {
  // The helper has no INITSLOT. Its three literal PACK operations require
  // four values from the caller's entry stack before the final RET.
  const script = new Uint8Array([
    0x57, 0x00, 0x00, // caller: INITSLOT 0 locals, 0 args
    0x34, 0x03,       // CALL +3 -> helper at 0x06
    0x40,
    0x12, 0xBF,      // PUSH2; PACKSTRUCT
    0x12, 0xBF,      // PUSH2; PACKSTRUCT
    0x50,             // SWAP
    0x12, 0xC0,      // PUSH2; PACK
    0x40,
  ]);
  const { highLevel, csharp, warnings } = decompileHighLevelBytes(buildNefFromScript(script), {
    typedDeclarations: true,
  });

  assert.match(highLevel, /sub_0x0006\(\?\?\?, \?\?\?, \?\?\?, \?\?\?\)/);
  assert.ok(
    warnings.some((warning) => /missing call argument values for sub_0x0006/.test(warning)),
    `warnings should include the PACKSTRUCT helper underflow: ${JSON.stringify(warnings)}`,
  );
  assert.match(
    csharp,
    /sub_0x0006\(\(dynamic\)\(\(\(object\)null\) \?\? throw new InvalidOperationException\("VM argument underflow/,
  );
  assert.match(csharp, /throwing compatibility expression/);
});

test("proven non-returning internal calls terminate both C# branch paths", () => {
  // main(bool) calls helper() on either branch. The helper ends in ABORT and
  // has no RET, so the call is a proven non-returning edge even though its
  // manifest return type is Integer.
  const script = new Uint8Array([
    0x57, 0x00, 0x01, // 0x00 INITSLOT 0 locals, 1 arg
    0x78,             // 0x03 LDARG0
    0x26, 0x04,       // 0x04 JMPIFNOT +4 -> 0x08
    0x34, 0x04,       // 0x06 CALL +4 -> helper at 0x0A
    0x34, 0x02,       // 0x08 CALL +2 -> helper at 0x0A
    0x57, 0x00, 0x00, // 0x0A helper INITSLOT 0 locals, 0 args
    0x38,             // 0x0D ABORT
  ]);
  const manifest = {
    name: "NoReturnCall",
    abi: {
      methods: [
        {
          name: "main",
          parameters: [{ name: "abortMsg", type: "Boolean" }],
          returntype: "Integer",
          offset: 0,
        },
        { name: "helper", parameters: [], returntype: "Integer", offset: 10 },
      ],
      events: [],
    },
  };
  const result = decompileHighLevelBytesWithManifest(buildNefFromScript(script), manifest);

  assert.match(result.highLevel, /if abortMsg \{/);
  assert.match(result.highLevel, /helper\(\);\n\s*throw\(\);/);
  assert.match(result.highLevel, /\} else \{/);
  assert.match(result.csharp, /helper\(\);\n\s*throw new Exception\(\);/);
  assert.equal(
    result.methodContracts.methods.find(({ method }) => method.offset === 10)?.returnBehavior,
    "value",
    "manifest return behavior remains authoritative",
  );
});

test("proven non-returning calls stop source-order lifting at dead opcodes", () => {
  // The helper starts at 0x08 and throws. The PUSH/JMP bytes after the call
  // are still inside the caller's method slice, but cannot execute on that
  // path and must not surface as untranslated control flow.
  const script = new Uint8Array([
    0x57, 0x00, 0x00, // 0x00 INITSLOT 0 locals, 0 args
    0x34, 0x05,       // 0x03 CALL +5 -> helper at 0x08
    0x11,             // 0x05 unreachable PUSH1
    0x26, 0x04,       // 0x06 unreachable JMPIF
    0x3A,             // 0x08 helper: THROW
  ]);
  const manifest = {
    name: "DeadCallTail",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0 },
        { name: "helper", parameters: [], returntype: "Integer", offset: 8 },
      ],
      events: [],
    },
  };
  const result = decompileHighLevelBytesWithManifest(buildNefFromScript(script), manifest);
  assert.match(result.highLevel, /helper\(\);\n\s*throw\(\);/);
  assert.doesNotMatch(result.highLevel, /JMPIF.*not yet translated/);
  assert.doesNotMatch(result.warnings.join("\n"), /JMPIF.*not yet translated/);
});
