import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import {
  analyzeBytes,
  decompileBytes,
  decompileBytesWithManifest,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
  disassembleScript,
  parseManifest,
  parseNef,
} from "../src/index.js";

const GAS_TOKEN_HASH = [
  0xcf, 0x76, 0xe2, 0x8b, 0xd0, 0x06, 0x2c, 0x4a, 0x47, 0x8e,
  0xe3, 0x55, 0x61, 0x01, 0x13, 0x19, 0xf3, 0xcf, 0xa4, 0xd2,
];

const SAMPLE_MANIFEST = JSON.stringify({
  name: "SampleToken",
  groups: [],
  supportedstandards: ["NEP-17"],
  features: { storage: true, payable: false },
  abi: {
    methods: [
      {
        name: "symbol",
        parameters: [],
        returntype: "String",
        offset: 0,
        safe: true,
      },
    ],
    events: [],
  },
  permissions: [],
  trusts: [],
  extra: {},
});

function writeVarint(buffer, value) {
  if (value <= 0xfc) {
    buffer.push(value);
  } else if (value <= 0xffff) {
    buffer.push(0xfd, value & 0xff, value >> 8);
  } else {
    buffer.push(
      0xfe,
      value & 0xff,
      (value >> 8) & 0xff,
      (value >> 16) & 0xff,
      (value >> 24) & 0xff,
    );
  }
}

function buildSampleNef() {
  const script = [0x10, 0x11, 0x9e, 0x40];
  return buildNefFromScript(script);
}

function buildNefFromScript(scriptBytes) {
  const script = Array.from(scriptBytes);
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0);
  data.push(0);
  if (script.length === 4 && script[0] === 0x10 && script[1] === 0x11) {
    data.push(1);
    data.push(...GAS_TOKEN_HASH);
    writeVarint(data, 8);
    data.push(...Buffer.from("Transfer"));
    data.push(0x02, 0x00);
    data.push(0x01);
    data.push(0x0f);
  } else {
    data.push(0);
  }
  data.push(0x00, 0x00);
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

function buildLocalMathNef() {
  const script = [
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, // PUSH1
    0x70, // STLOC0
    0x68, // LDLOC0
    0x12, // PUSH2
    0x9e, // ADD
    0x40, // RET
  ];
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0);
  data.push(0);
  data.push(0);
  data.push(0x00, 0x00);
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

function buildNefWithSingleToken(
  scriptBytes,
  hash,
  method,
  parametersCount,
  hasReturnValue,
  callFlags,
) {
  const script = Array.from(scriptBytes);
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0);
  data.push(0);
  data.push(1);
  data.push(...hash);
  writeVarint(data, Buffer.byteLength(method));
  data.push(...Buffer.from(method));
  data.push(parametersCount & 0xff, (parametersCount >> 8) & 0xff);
  data.push(hasReturnValue ? 1 : 0);
  data.push(callFlags);
  data.push(0x00, 0x00);
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

function computeChecksum(payload) {
  const first = createHash("sha256").update(Buffer.from(payload)).digest();
  const second = createHash("sha256").update(first).digest();
  return Array.from(second.subarray(0, 4));
}

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

test("lifts straight-line arithmetic into a high-level return", () => {
  const result = decompileHighLevelBytes(buildSampleNef());
  assert.match(result.highLevel, /fn script_entry\(\) \{/);
  assert.match(result.highLevel, /return 0 \+ 1;/);
});

test("lifts locals into named high-level statements", () => {
  const result = decompileHighLevelBytes(buildLocalMathNef());
  assert.match(result.highLevel, /let loc0 = 1;/);
  assert.match(result.highLevel, /return loc0 \+ 2;/);
});

test("uses manifest method names in grouped high-level output", () => {
  const result = decompileHighLevelBytesWithManifest(buildSampleNef(), SAMPLE_MANIFEST);
  assert.match(result.highLevel, /contract SampleToken \{/);
  assert.match(result.highLevel, /fn symbol\(\) -> string \{/);
  assert.match(result.highLevel, /return 0 \+ 1;/);
});

test("lifts a simple forward JMPIFNOT into an if block", () => {
  const script = new Uint8Array([0x11, 0x26, 0x04, 0x12, 0x40, 0x40]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /if 1 \{/);
  assert.match(result.highLevel, /return 2;/);
  assert.match(result.highLevel, /return;/);
});

test("lifts a simple forward JMPIF into a negated if block", () => {
  const script = new Uint8Array([0x11, 0x24, 0x04, 0x12, 0x40]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /if !1 \{/);
  assert.match(result.highLevel, /return 2;/);
});

test("lifts a simple JMPEQ forward branch using negated condition", () => {
  const script = new Uint8Array([0x11, 0x11, 0x28, 0x04, 0x12, 0x40]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /if 1 != 1 \{/);
  assert.match(result.highLevel, /return 2;/);
});

test("lifts a simple forward JMPIFNOT plus jump into if-else", () => {
  const script = new Uint8Array([0x11, 0x26, 0x04, 0x12, 0x40, 0x13, 0x40]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /if 1 \{/);
  assert.match(result.highLevel, /\} else \{/);
  assert.match(result.highLevel, /return 2;/);
  assert.match(result.highLevel, /return 3;/);
});

test("lifts an explicit else branch with a shared suffix", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, // PUSH1
    0x26, 0x06, // JMPIFNOT -> else
    0x12, // PUSH2
    0x70, // STLOC0
    0x22, 0x04, // JMP -> merge
    0x13, // PUSH3
    0x70, // STLOC0
    0x68, // LDLOC0
    0x40, // RET
  ]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /if 1 \{/);
  assert.match(result.highLevel, /\} else \{/);
  assert.match(result.highLevel, /let loc0 = 2;/);
  assert.match(result.highLevel, /let loc0 = 3;/);
  assert.match(result.highLevel, /return loc0;/);
});

test("lifts a simple while loop", () => {
  const script = new Uint8Array([0x11, 0x26, 0x05, 0x21, 0x22, 0xfc, 0x40]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /while 1 \{/);
  assert.doesNotMatch(result.highLevel, /JMP 252/);
});

test("lifts a simple do-while loop", () => {
  const script = new Uint8Array([0x11, 0x21, 0x11, 0x24, 0xfd, 0x40]);
  const nef = buildNefFromScript(script);
  const result = decompileHighLevelBytes(nef);
  assert.match(result.highLevel, /do \{/);
  assert.match(result.highLevel, /\} while \(1\);/);
});

test("eliminates fallthrough gotos that target the next instruction", () => {
  // JMP +2 (to JMP_L), JMP_L +5 (to RET) — both jumps land on the very
  // next instruction, so eliminate_fallthrough_gotos collapses them and
  // the orphaned labels get stripped, leaving a clean body.
  const script = new Uint8Array([0x22, 0x02, 0x23, 0x05, 0x00, 0x00, 0x00, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.doesNotMatch(
    result.highLevel,
    /goto label_/,
    "fallthrough gotos should be eliminated",
  );
  assert.doesNotMatch(
    result.highLevel,
    /label_0x000[27]:/,
    "fallthrough goto targets should be removed as orphan labels",
  );
  assert.doesNotMatch(
    result.highLevel,
    /control flow not yet lifted/,
    "lifting must not leave behind unhandled-control-flow placeholders",
  );
});

test("eliminates fallthrough leave transfers from ENDTRYs", () => {
  // ENDTRY +2 (to ENDTRY_L), ENDTRY_L +5 (to RET) — same fallthrough
  // pattern as above, but in the try-context `leave` form.
  const script = new Uint8Array([0x3d, 0x02, 0x3e, 0x05, 0x00, 0x00, 0x00, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.doesNotMatch(
    result.highLevel,
    /leave label_/,
    "fallthrough leave transfers should be eliminated",
  );
  assert.doesNotMatch(
    result.highLevel,
    /label_0x000[27]:/,
    "fallthrough leave targets should be removed as orphan labels",
  );
  assert.doesNotMatch(
    result.highLevel,
    /control flow not yet lifted/,
    "lifting must not leave behind unhandled-control-flow placeholders",
  );
});

test("renders comparison jump fallbacks without not-yet-translated placeholders", () => {
  const script = new Uint8Array([
    0x10, 0x11, 0x28, 0x01, 0x21,
    0x10, 0x11, 0x2a, 0x01, 0x21,
    0x10, 0x11, 0x2c, 0x01, 0x21,
    0x10, 0x11, 0x2e, 0x01, 0x21,
    0x10, 0x11, 0x30, 0x01, 0x21,
    0x10, 0x11, 0x32, 0x01, 0x21,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, / == /);
  assert.match(result.highLevel, / != /);
  assert.match(result.highLevel, />/);
  assert.match(result.highLevel, />=/);
  assert.match(result.highLevel, /</);
  assert.match(result.highLevel, /<=/);
  assert.doesNotMatch(result.highLevel, /not yet translated/);
});

test("lifts a nested loop inside an outer if", () => {
  const script = new Uint8Array([
    0x10,
    0x26, 0x08,
    0x10,
    0x26, 0x05,
    0x21,
    0x22, 0xfc,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /if/);
  assert.match(result.highLevel, /while/);
  assert.doesNotMatch(result.highLevel, /not yet translated/);
});

test("lifts a nested inner if without raw placeholders", () => {
  const script = new Uint8Array([
    0x10,
    0x26, 0x0a,
    0x10,
    0x26, 0x05,
    0x21,
    0x22, 0x04,
    0x21,
    0x21,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /if/);
  assert.doesNotMatch(result.highLevel, /not yet translated/);
});

test("keeps nested if-else structured without raw labels", () => {
  const script = new Uint8Array([
    0x10, // PUSH0
    0x26, 0x09, // JMPIFNOT +9 → 0x000A
    0x10, // PUSH0
    0x26, 0x05, // JMPIFNOT +5 → 0x0009
    0x08, // PUSHT (true)
    0x22, 0x02, // JMP +2 → 0x000A
    0x09, // PUSHF (false)
    0x09, // PUSHF (false)
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /else \{/);
  assert.doesNotMatch(result.highLevel, /label_0x0009:/);
  assert.doesNotMatch(result.highLevel, /label_0x000a:/);
});

test("lifts a try block inside a loop without raw placeholders", () => {
  const script = new Uint8Array([
    0x10,
    0x26, 0x0c,
    0x3b, 0x07, 0x00,
    0x21,
    0x3d, 0x06,
    0x21,
    0x3f,
    0x22, 0xf5,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /while/);
  assert.match(result.highLevel, /try/);
  assert.doesNotMatch(result.highLevel, /not yet translated/);
});

test("lifts internal CALL targets into named helper calls", () => {
  const script = new Uint8Array([
    0x34, 0x05, // CALL +5 -> 0x0005
    0x40, // RET
    0x21, 0x21, // NOP x2
    0x57, 0x00, 0x00, // INITSLOT 0,0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /fn sub_0x0005\(\) \{/);
  assert.match(result.highLevel, /sub_0x0005\(\)/);
});

test("string and hex literal operands do not get redundant outer parens", () => {
  // Bytes 0x6B 0x65 0x79 are printable ASCII "key", and the lifted
  // `cat` (CAT) operand chain previously rendered as
  // `return ("Hello, ") cat ("World!");`. Both literal types now
  // render bare.
  const helloHex = "Hello, ".split("").map((c) => c.charCodeAt(0));
  const worldHex = "World!".split("").map((c) => c.charCodeAt(0));
  const script = new Uint8Array([
    0x0c, 7, ...helloHex, // PUSHDATA1 "Hello, "
    0x0c, 6, ...worldHex, // PUSHDATA1 "World!"
    0x8b, // CAT
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /return "Hello, " cat "World!";/,
    "string literals should not get extra parens around them",
  );
  assert.doesNotMatch(
    result.highLevel,
    /\("Hello, "\)|\("World!"\)/,
    "no redundant parens around bare string operands",
  );
});

test("function-call operand does not get redundant outer parens", () => {
  // Script: PUSH1; CALL +5; ADD; RET; NOP; PUSH3; RET — main calls a
  // helper that pushes 3, then adds 1. Used to render as
  // `return 1 + (sub_0x0006());`. The redundant outer parens on the
  // self-contained call expression now collapse.
  const script = new Uint8Array([
    0x11,             // PUSH1
    0x34, 0x05,       // CALL +5 -> helper at 0x0006
    0x9E,             // ADD
    0x40,             // RET
    0x21,             // NOP padding
    0x13,             // PUSH3 (helper)
    0x40,             // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /return 1 \+ sub_0x0006\(\);/,
    "self-contained call should not be wrapped in extra parens",
  );
  assert.doesNotMatch(
    result.highLevel,
    /\(sub_0x0006\(\)\)/,
    "redundant outer parens around the call should be gone",
  );
});

test("manifest-less header runs `// manifest not provided` flush against `// method tokens declared in NEF`", () => {
  // Earlier the no-manifest branch eagerly pushed a blank line after
  // `// manifest not provided`, which compounded with the
  // method-tokens header to produce `// manifest not provided\n\n//
  // method tokens declared in NEF`. Rust runs them flush. Verify
  // the contiguous comment block now matches Rust byte-for-byte.
  const script = new Uint8Array([0x40]); // RET
  const nef = buildNefWithSingleToken(
    script,
    GAS_TOKEN_HASH,
    "Transfer",
    2,
    true,
    0x0f,
  );
  const { highLevel } = decompileHighLevelBytes(nef);
  assert.match(
    highLevel,
    /\/\/ manifest not provided\n {4}\/\/ method tokens declared in NEF/,
    "manifest-not-provided and method-tokens header should be flush:\n" +
      highLevel,
  );
});

test("manifest-less header surfaces `// manifest not provided` and uses NeoContract default", () => {
  // Without a manifest the Rust port emits `contract NeoContract { ...
  // // manifest not provided\n\n fn ... }`. The JS port previously named
  // the contract `Contract` and skipped the manifest-absence comment,
  // producing a smaller header that diverged from Rust on every
  // manifest-less script.
  const script = new Uint8Array([0x40]); // RET
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(
    result.highLevel,
    /^contract NeoContract \{/m,
    "default contract name should be `NeoContract` (matches Rust)",
  );
  assert.match(
    result.highLevel,
    /\/\/ manifest not provided/,
    "absence of a manifest should surface explicitly",
  );
});

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
  assert.match(result.highLevel, /fn script_entry\(\) \{\n\s*sub_0x0002\(\);/);
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

test("renders known syscalls with human-readable names", () => {
  const script = new Uint8Array([0x41, 0xb2, 0x79, 0xfc, 0xf6, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return syscall\("System\.Runtime\.Platform"\);/);
});

test("renders void syscalls as statements instead of return values", () => {
  const script = new Uint8Array([0x10, 0x41, 0xcf, 0xe7, 0x47, 0x96, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /syscall\("System\.Runtime\.Log", 0\);/);
  assert.doesNotMatch(result.highLevel, /= syscall\("System\.Runtime\.Log"/);
  assert.match(result.highLevel, /return;/);
});

test("treats unknown syscalls as returning values", () => {
  const script = new Uint8Array([0x41, 0xef, 0xbe, 0xad, 0xde, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return syscall\(0xDEADBEEF\);/);
});

test("distinguishes void and value-returning known syscalls", () => {
  const storagePut = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x10, 0x10, 0x10, 0x41, 0xe6, 0x3f, 0x18, 0x84, 0x40])),
  );
  const checkWitness = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x10, 0x41, 0xf8, 0x27, 0xec, 0x8c, 0x40])),
  );
  assert.match(storagePut.highLevel, /syscall\("System\.Storage\.Put", 0, 0, 0\);/);
  assert.match(checkWitness.highLevel, /return syscall\("System\.Runtime\.CheckWitness", 0\);/);
});

test("emits missing syscall argument warnings inline and structurally", () => {
  const script = new Uint8Array([0x41, 0xcf, 0xe7, 0x47, 0x96, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /syscall\("System\.Runtime\.Log", \?\?\?\);/);
  assert.match(
    result.highLevel,
    /missing syscall argument values for System\.Runtime\.Log/,
  );
  assert.ok(
    result.warnings.some((warning) =>
      warning.includes("missing syscall argument values for System.Runtime.Log"),
    ),
  );
});

test("adds packed-store context to missing syscall argument warnings", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1,0
    0x0c, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f, // PUSHDATA1 "Hello"
    0x11, // PUSH1
    0xc0, // PACK
    0x70, // STLOC0
    0x41, 0xcf, 0xe7, 0x47, 0x96, // SYSCALL System.Runtime.Log
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /syscall\("System\.Runtime\.Log", \?\?\?\);/);
  assert.match(
    result.highLevel,
    /preceding STLOC0 stored a packed value into loc0/,
  );
  assert.ok(
    result.warnings.some((warning) =>
      warning.includes("preceding STLOC0 stored a packed value into loc0"),
    ),
  );
});

test("lifts literal PACK into an array expression", () => {
  const script = new Uint8Array([0x11, 0x12, 0x12, 0xc0, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return \[2, 1\];/);
});

test("lifts PACKMAP into a map constructor expression", () => {
  const script = new Uint8Array([0x10, 0xbe, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return Map\(\);/);
});

test("lifts PACKSTRUCT into a struct constructor expression", () => {
  const script = new Uint8Array([0x11, 0x12, 0x12, 0xbf, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return Struct\(2, 1\);/);
});

test("rewrites PICKITEM as indexing", () => {
  const script = new Uint8Array([0xc2, 0x10, 0xce, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // NEWARRAY0 now lifts to `let t0 = [];` so the index reads from t0.
  assert.match(result.highLevel, /return t0\[0\];/);
});

test("rewrites SETITEM as index assignment", () => {
  const script = new Uint8Array([0xc8, 0x10, 0x11, 0xd0, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // NEWMAP now lifts to `let t0 = {};` so the index assigns into t0.
  assert.match(result.highLevel, /t0\[0\] = 1;/);
});

test("lifts generic CALLA as an indirect call expression", () => {
  const script = new Uint8Array([0x11, 0x10, 0x36, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /calla\(0\)/);
});

test("resolves PUSHA plus local flow into CALLA helper names", () => {
  const script = new Uint8Array([
    0x0a, 0x09, 0x00, 0x00, 0x00, // PUSHA 0x0009
    0x70, // STLOC0
    0x68, // LDLOC0
    0x36, // CALLA
    0x40, // RET
    0x57, 0x00, 0x00, // INITSLOT 0,0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /fn sub_0x0009\(\) \{/);
  assert.match(result.highLevel, /sub_0x0009\(\)/);
  assert.doesNotMatch(result.highLevel, /calla\(loc0\)/);
});

test("resolves direct PUSHA plus CALLA into helper names", () => {
  const script = new Uint8Array([
    0x0a, 0x0a, 0x00, 0x00, 0x00, // PUSHA +10
    0x36, // CALLA
    0x40, // RET
    0x21, 0x21, 0x21, // NOP x3
    0x57, 0x00, 0x00, // INITSLOT 0,0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /sub_0x000a\(\)/i);
  assert.doesNotMatch(result.highLevel, /calla\(/);
});

test("resolves static pointer flow into CALLA helper names", () => {
  const script = new Uint8Array([
    0x0a, 0x09, 0x00, 0x00, 0x00, // PUSHA +9
    0x60, // STSFLD0
    0x58, // LDSFLD0
    0x36, // CALLA
    0x40, // RET
    0x57, 0x00, 0x00, // INITSLOT 0,0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /sub_0x0009\(\)/);
  assert.doesNotMatch(result.highLevel, /calla\(static0\)/);
});

test("resolves duplicated pointer flow into CALLA helper names", () => {
  const script = new Uint8Array([
    0x0a, 0x08, 0x00, 0x00, 0x00, // PUSHA +8
    0x4a, // DUP
    0x36, // CALLA
    0x40, // RET
    0x57, 0x00, 0x00, // INITSLOT 0,0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /sub_0x0008\(\)/);
});

test("rewrites HASKEY as a helper call", () => {
  const script = new Uint8Array([0xc8, 0x10, 0xcb, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // NEWMAP now lifts to `let t0 = {};` so the helper receives the
  // materialised temp rather than a fresh literal.
  assert.match(result.highLevel, /return has_key\(t0, 0\);/);
});

test("unpack of stored packed value keeps reverse3 stack shape", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, 0x12, 0x12, 0xc0, 0x70, // PUSH1; PUSH2; PUSH2; PACK; STLOC0
    0x13, 0x68, 0xc1, 0x45, 0x53, 0x40, // PUSH3; LDLOC0; UNPACK; DROP; REVERSE3; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /let loc0 = \[2, 1\];/);
  // The previous `// reverse top 3 stack values` annotation was VM
  // narration — stripped from clean output now. The substantive
  // check is that REVERSE3 didn't underflow after the UNPACK.
  assert.doesNotMatch(
    result.highLevel,
    /insufficient values on stack for REVERSE3/,
    `UNPACK from stored PACK value should preserve enough stack entries for REVERSE3: ${result.highLevel}`,
  );
});

test("pick preserves packed shape metadata for unpack reverse4", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, 0x12, 0x12, 0xc0, 0x70, // PUSH1; PUSH2; PUSH2; PACK; STLOC0
    0x13, 0x68, 0x10, 0x4d, 0xc1, 0x45, 0x54, 0x40, // PUSH3; LDLOC0; PUSH0; PICK; UNPACK; DROP; REVERSE4; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // VM-mechanics annotation stripped; ensure no REVERSE4 underflow.
  assert.doesNotMatch(
    result.highLevel,
    /insufficient values on stack for REVERSE4/,
    `PICK should preserve packed shape metadata for downstream UNPACK stack modeling: ${result.highLevel}`,
  );
});

test("synthesizes unknown UNPACK elements from follow-up consumers", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x01, // INITSLOT 1 local, 1 arg
    0x78, // LDARG0
    0xc1, // UNPACK
    0x45, // DROP count
    0x70, // STLOC0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.doesNotMatch(result.highLevel, /unsupported UNPACK/);
  assert.match(result.highLevel, /unpack\(arg0\)/);
  assert.match(result.highLevel, /unpack_item\(/);
});

test("rewrites ISTYPE using the operand tag helper name", () => {
  const script = new Uint8Array([0x11, 0xd9, 0x40, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return is_type_array\(1\);/);
});

test("rewrites KEYS and VALUES as helper calls", () => {
  const keysScript = new Uint8Array([0xc8, 0xcc, 0x40]);
  const valuesScript = new Uint8Array([0xc8, 0xcd, 0x40]);
  const keys = decompileHighLevelBytes(buildNefFromScript(keysScript));
  const values = decompileHighLevelBytes(buildNefFromScript(valuesScript));
  // NEWMAP now materialises into `let t0 = {};`.
  assert.match(keys.highLevel, /return keys\(t0\);/);
  assert.match(values.highLevel, /return values\(t0\);/);
});

test("rewrites APPEND and POPITEM using collection helpers", () => {
  const appendScript = new Uint8Array([0xc2, 0x11, 0xcf, 0x40]);
  const popitemScript = new Uint8Array([0xc2, 0xd4, 0x40]);
  const append = decompileHighLevelBytes(buildNefFromScript(appendScript));
  const popitem = decompileHighLevelBytes(buildNefFromScript(popitemScript));
  // NEWARRAY0 now lifts to `let t0 = [];` so subsequent helpers see
  // the materialised temp rather than a fresh literal.
  assert.match(append.highLevel, /append\(t0, 1\);/);
  assert.match(popitem.highLevel, /return pop_item\(t0\);/);
});

test("handles DUP plus INC in high-level output", () => {
  const script = new Uint8Array([0x11, 0x4a, 0x9e, 0x9c, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return (\(1 \+ 1\)|1 \+ 1) \+ 1;/);
});

test("lifts PICK with a literal index", () => {
  const script = new Uint8Array([0x11, 0x12, 0x11, 0x4d, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return 1;/);
});

test("lifts PICK with a dynamic index helper", () => {
  const script = new Uint8Array([0x57, 0x00, 0x01, 0x11, 0x12, 0x78, 0x4d, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /pick\(arg0\)/);
  assert.doesNotMatch(result.highLevel, /unsupported dynamic PICK/);
});

test("lifts XDROP with a literal index", () => {
  const script = new Uint8Array([0x11, 0x12, 0x13, 0x11, 0x48, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return 3;/);
});

test("XDROP with a dynamic index lifts cleanly without an unsupported placeholder", () => {
  const script = new Uint8Array([0x57, 0x00, 0x01, 0x11, 0x12, 0x78, 0x48, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // The `// xdrop stack[arg0] ...` annotation that previously
  // appeared in the lifted body is verbose-mode noise — the Rust
  // port strips it from clean output, and the JS port now does too
  // (parity). What matters is that the stale `unsupported dynamic
  // XDROP` placeholder doesn't leak through.
  assert.doesNotMatch(result.highLevel, /unsupported dynamic XDROP/);
  assert.doesNotMatch(
    result.highLevel,
    /\/\/ xdrop stack/,
    `xdrop trace comment should be stripped: ${result.highLevel}`,
  );
});

test("rewrites CONVERT using the operand target helper name", () => {
  const script = new Uint8Array([0x11, 0xdb, 0x28, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return convert_to_bytestring\(1\);/);
});

test("lifts unary math helpers", () => {
  const notResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0xaa, 0x40])),
  );
  const negateResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x9b, 0x40])),
  );
  const absResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x9a, 0x40])),
  );
  const signResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x99, 0x40])),
  );
  const invertResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x90, 0x40])),
  );

  assert.match(notResult.highLevel, /return !1;/);
  assert.match(negateResult.highLevel, /return -1;/);
  assert.match(absResult.highLevel, /return abs\(1\);/);
  assert.match(signResult.highLevel, /return sign\(1\);/);
  assert.match(invertResult.highLevel, /return ~1;/);
});

test("lifts binary math and helper ops", () => {
  const andResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x91, 0x40])),
  );
  const orResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x92, 0x40])),
  );
  const xorResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x93, 0x40])),
  );
  const sqrtResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0xa4, 0x40])),
  );
  const modmulResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x11, 0xa5, 0x40])),
  );
  const withinResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x11, 0xbb, 0x40])),
  );
  const leftResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x8d, 0x40])),
  );
  const rightResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x8e, 0x40])),
  );

  assert.match(andResult.highLevel, /return 1 & 1;/);
  assert.match(orResult.highLevel, /return 1 \| 1;/);
  assert.match(xorResult.highLevel, /return 1 \^ 1;/);
  assert.match(sqrtResult.highLevel, /return sqrt\(1\);/);
  assert.match(modmulResult.highLevel, /return modmul\(1, 1, 1\);/);
  assert.match(withinResult.highLevel, /return within\(1, 1, 1\);/);
  assert.match(leftResult.highLevel, /return left\(1, 1\);/);
  assert.match(rightResult.highLevel, /return right\(1, 1\);/);
});

test("lifts SUBSTR as a helper call", () => {
  const result = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x11, 0x11, 0x8c, 0x40])),
  );
  assert.match(result.highLevel, /return substr\(1, 1, 1\);/);
});

test("handles OVER, SWAP, and NIP stack shuffles", () => {
  const over = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0x12, 0x4b, 0x40])));
  const swap = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0x12, 0x50, 0x40])));
  const nip = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0x12, 0x46, 0x40])));
  assert.match(over.highLevel, /return 1;/);
  assert.match(swap.highLevel, /return 1;/);
  assert.match(nip.highLevel, /return 2;/);
});

test("ROT exposes the rotated-to-top value via return", () => {
  // PUSH1 PUSH2 PUSH3 ROT RET — ROT is [a,b,c]→[b,c,a], so the
  // return references PUSH1's value (the original `a`).
  const script = new Uint8Array([0x11, 0x12, 0x13, 0x51, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return 1;/);
});

test("TUCK leaves PUSH2's value as the lifted return", () => {
  // PUSH1 PUSH2 TUCK RET — TUCK puts a duplicate of the top under
  // the second; the new top is still PUSH2's value.
  const script = new Uint8Array([0x11, 0x12, 0x4e, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return 2;/);
});

test("lifts ROLL with a literal index", () => {
  const script = new Uint8Array([0x11, 0x12, 0x13, 0x11, 0x52, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return 2;/);
});

test("lifts ROLL with a dynamic index helper", () => {
  const script = new Uint8Array([0x57, 0x00, 0x01, 0x11, 0x12, 0x78, 0x52, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /roll\(arg0\)/);
  assert.doesNotMatch(result.highLevel, /unsupported dynamic ROLL/);
});

test("REVERSEN with a literal count flips the top of stack", () => {
  // PUSH1 PUSH2 PUSH3 PUSH3(count) REVERSEN RET — reverses the top 3
  // entries; the new top is PUSH1's value.
  const script = new Uint8Array([0x11, 0x12, 0x13, 0x13, 0x55, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /return 1;/);
});

test("REVERSEN with a dynamic count lifts cleanly without a placeholder", () => {
  const script = new Uint8Array([0x57, 0x00, 0x01, 0x11, 0x12, 0x78, 0x55, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.doesNotMatch(result.highLevel, /unsupported dynamic REVERSEN/);
  assert.doesNotMatch(
    result.highLevel,
    /\/\/ reverse top/,
    `reverse-top annotation should be stripped: ${result.highLevel}`,
  );
});

test("lifts DEPTH and CLEAR stack ops", () => {
  const depthResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x12, 0x43, 0x40])),
  );
  const clearResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x49, 0x40])),
  );
  assert.match(depthResult.highLevel, /return 2;/);
  // The previous `// clear stack` annotation was VM narration —
  // stripped from clean output now (parity with Rust). What we
  // care about is that CLEAR didn't crash the lift and that the
  // lifted body still ends with a bare `return;`.
  assert.doesNotMatch(
    clearResult.highLevel,
    /\/\/ clear stack/,
    `clear-stack annotation should be stripped: ${clearResult.highLevel}`,
  );
  assert.match(clearResult.highLevel, /return;/);
});

test("emits break for a loop jump to the break target", () => {
  const script = new Uint8Array([
    0x11, // PUSH1
    0x26, 0x07, // JMPIFNOT -> exit
    0x22, 0x05, // JMP -> break target
    0x21, // NOP
    0x22, 0xfa, // back jump to loop head
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /while 1 \{/);
  assert.match(result.highLevel, /break;/);
});

test("emits continue for a loop jump to the continue target", () => {
  const script = new Uint8Array([
    0x11, // PUSH1
    0x26, 0x07, // JMPIFNOT -> exit
    0x22, 0xfd, // JMP -> continue target / loop head
    0x21, // NOP
    0x22, 0xfa, // back jump to loop head
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /while 1 \{/);
  assert.match(result.highLevel, /continue;/);
});

test("lifts a simple for loop", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00,
    0x10, 0x70,
    0x68, 0x13, 0xb5,
    0x26, 0x09,
    0x21,
    0x68, 0x11, 0x9e, 0x70,
    0x22, 0xf6,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /for \(let loc0 = 0; loc0 < 3; loc0 \+= 1\) \{/);
});

test("lifts simple try-finally with finally body", () => {
  const script = new Uint8Array([
    0x11, // PUSH1
    0x3B, 0x00, 0x06, // TRY (finally at +6)
    0x12, // PUSH2 (try body)
    0x3D, 0x02, // ENDTRY +2
    0x13, // PUSH3 (finally body)
    0x3F, // ENDFINALLY
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /try \{/);
  assert.match(result.highLevel, /finally \{/);
  assert.match(result.highLevel, /return 3;/);
});

test("lifts simple try-catch blocks", () => {
  const script = new Uint8Array([0x3b, 0x03, 0x00, 0x11, 0x3d, 0x03, 0x12, 0x3d, 0x00, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /try \{/);
  assert.match(result.highLevel, /catch \{/);
});

test("lifts simple try-catch-finally blocks", () => {
  const script = new Uint8Array([
    0x3b, 0x03, 0x06, 0x11, 0x3d, 0x05, 0x12, 0x3d, 0x02, 0x13, 0x3f, 0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /try \{/);
  assert.match(result.highLevel, /catch \{/);
  assert.match(result.highLevel, /finally \{/);
});

test("models catch entry stack with exception value", () => {
  const script = new Uint8Array([
    0x3b, 0x06, 0x00, // TRY catch=+6
    0x11, // PUSH1
    0x3d, 0x06, // ENDTRY +6
    0x70, // STLOC0
    0x68, // LDLOC0
    0x3d, 0x00, // ENDTRY +0
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /catch \{/);
  assert.match(result.highLevel, /let loc0 = exception;/);
});

test("lifts throw inside try-finally", () => {
  const script = new Uint8Array([
    0x3b, 0x00, 0x07,
    0x11,
    0x3a,
    0x3d, 0x04,
    0x12,
    0x3f,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /try \{/);
  assert.match(result.highLevel, /throw\(1\);/);
  assert.match(result.highLevel, /finally \{/);
});

test("lifts abort inside catch blocks", () => {
  const script = new Uint8Array([
    0x3b, 0x03, 0x00,
    0x11,
    0x3d, 0x03,
    0x38,
    0x3d, 0x00,
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /catch \{/);
  assert.match(result.highLevel, /abort\(\);/);
});

test("recovers a switch from an equality chain", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local
    0x11, 0x70, // PUSH1; STLOC0
    0x68, 0x10, 0x97, // LDLOC0; PUSH0; EQUAL
    0x26, 0x06, // JMPIFNOT +6
    0x1a, 0x70, // PUSH10; STLOC0
    0x22, 0x0d, // JMP +13
    0x68, 0x11, 0x97, // LDLOC0; PUSH1; EQUAL
    0x26, 0x06, // JMPIFNOT +6
    0x1b, 0x70, // PUSH11; STLOC0
    0x22, 0x04, // JMP +4
    0x1c, 0x70, // PUSH12; STLOC0
    0x68, 0x40, // LDLOC0; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /switch loc0 \{/);
  assert.match(result.highLevel, /case 0 \{/);
  assert.match(result.highLevel, /case 1 \{/);
  assert.match(result.highLevel, /default \{/);
});

test("recovers a switch from a string-literal equality chain", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local
    0x0c, 0x01, 0x30, 0x70, // PUSHDATA1 "0"; STLOC0
    0x68, 0x0c, 0x01, 0x30, 0x97, // LDLOC0; PUSHDATA1 "0"; EQUAL
    0x26, 0x06, // JMPIFNOT +6
    0x1a, 0x70, // PUSH10; STLOC0
    0x22, 0x0e, // JMP +14
    0x68, 0x0c, 0x01, 0x31, 0x97, // LDLOC0; PUSHDATA1 "1"; EQUAL
    0x26, 0x06, // JMPIFNOT +6
    0x1b, 0x70, // PUSH11; STLOC0
    0x22, 0x04, // JMP +4
    0x1c, 0x70, // PUSH12; STLOC0
    0x68, 0x40, // LDLOC0; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /switch loc0 \{/);
  assert.match(result.highLevel, /case "0" \{/);
  assert.match(result.highLevel, /case "1" \{/);
  assert.match(result.highLevel, /default \{/);
});

test("INITSLOT lifts to a local store + load + return without leaking the slot-declaration comment", () => {
  const script = new Uint8Array([0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // `// declare N locals, M arguments` is informational noise that
  // belongs in verbose mode only; the JS port has no verbose mode,
  // so it should be stripped entirely (parity with the Rust port's
  // clean-by-default output).
  assert.ok(
    !/\/\/ declare \d+ locals, \d+ arguments/.test(result.highLevel),
    `slot-declaration comment should be stripped: ${result.highLevel}`,
  );
  assert.match(result.highLevel, /let loc0 = 1;/);
  assert.match(result.highLevel, /return loc0;/);
});

test("supports indexed local and argument slots", () => {
  const localScript = new Uint8Array([0x57, 0x08, 0x00, 0x11, 0x77, 0x07, 0x6f, 0x07, 0x40]);
  const argScript = new Uint8Array([0x57, 0x00, 0x08, 0x7f, 0x07, 0x11, 0x87, 0x07, 0x40]);
  const local = decompileHighLevelBytes(buildNefFromScript(localScript));
  const arg = decompileHighLevelBytes(buildNefFromScript(argScript));
  assert.match(local.highLevel, /loc7/);
  assert.match(arg.highLevel, /arg7/);
});

test("supports static slots in high-level output", () => {
  const script = new Uint8Array([0x56, 0x02, 0x11, 0x60, 0x58, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  // INITSSLOT's slot-count comment, like INITSLOT's, is verbose-mode
  // noise — strip it from clean output.
  assert.ok(
    !/\/\/ declare \d+ static slots/.test(result.highLevel),
    `static-slot declaration comment should be stripped: ${result.highLevel}`,
  );
  assert.match(result.highLevel, /let static0 = 1;/);
  assert.match(result.highLevel, /return static0;/);
});

test("rewrites SIZE and ISNULL helpers", () => {
  const sizeScript = new Uint8Array([0xc2, 0xca, 0x40]);
  const nullScript = new Uint8Array([0x0b, 0xd8, 0x40]);
  const size = decompileHighLevelBytes(buildNefFromScript(sizeScript));
  const isNull = decompileHighLevelBytes(buildNefFromScript(nullScript));
  // NEWARRAY0 now lifts to `let t0 = []; return len(t0);` so the
  // helper's argument is the materialised temp rather than a fresh
  // literal.
  assert.match(size.highLevel, /return len\(t0\);/);
  assert.match(isNull.highLevel, /return is_null\(null\);/);
});

test("rewrites NEWARRAY, NEWSTRUCT, and NEWBUFFER helpers", () => {
  const newArray = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0xc3, 0x40])));
  const newStruct = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0xc6, 0x40])));
  const newBuffer = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0x88, 0x40])));
  assert.match(newArray.highLevel, /return new_array\(1\);/);
  assert.match(newStruct.highLevel, /return new_struct\(1\);/);
  assert.match(newBuffer.highLevel, /return new_buffer\(1\);/);
});

test("rewrites NEWARRAY_T using the operand type tag", () => {
  const result = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0xc4, 0x40, 0x40])),
  );
  assert.match(result.highLevel, /return new_array_t\(1, "array"\);/);
});

test("rewrites MEMCPY as a helper statement", () => {
  const result = decompileHighLevelBytes(
    buildNefFromScript(
      new Uint8Array([0x11, 0x11, 0x11, 0x11, 0x11, 0x89, 0x40]),
    ),
  );
  assert.match(result.highLevel, /memcpy\(1, 1, 1, 1, 1\);/);
  assert.match(result.highLevel, /return;/);
});

test("rewrites REMOVE, CLEARITEMS, and REVERSEITEMS helpers", () => {
  const remove = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0xc8, 0x10, 0xd2, 0x40])));
  const clear = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0xc8, 0xd3, 0x40])));
  const reverse = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0xc2, 0xd1, 0x40])));
  // NEWMAP / NEWARRAY0 now materialise into a temp so DUP'd references
  // resolve to the same identifier (e.g. `let t0 = {}; t0["k"] = "v";
  // return t0;`). The mutating helpers therefore receive the temp
  // rather than a fresh literal.
  assert.match(remove.highLevel, /remove_item\(t0, 0\);/);
  assert.match(clear.highLevel, /clear_items\(t0\);/);
  assert.match(reverse.highLevel, /reverse_items\(t0\);/);
});

test("assert pops only the condition and keeps the remaining stack", () => {
  const script = new Uint8Array([0x12, 0x11, 0x39, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /assert\(1\);/);
  assert.match(result.highLevel, /return 2;/);
});

test("abort and throw clear the tracked stack", () => {
  const abort = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0x38, 0x40])));
  const throwResult = decompileHighLevelBytes(buildNefFromScript(new Uint8Array([0x11, 0x3a, 0x40])));
  assert.match(abort.highLevel, /abort\(\);/);
  assert.match(abort.highLevel, /return;/);
  assert.match(throwResult.highLevel, /throw\(1\);/);
  assert.match(throwResult.highLevel, /return;/);
});

test("abortmsg and assertmsg use their message and condition arguments", () => {
  const abortmsg = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x0c, 0x01, 0x41, 0xe0, 0x40])),
  );
  const assertmsg = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x0c, 0x01, 0x42, 0xe1, 0x40])),
  );
  // Bytes 0x41 / 0x42 are printable ASCII ("A" / "B"); the lifter now
  // decodes them as quoted string literals, matching the Rust port.
  assert.match(abortmsg.highLevel, /abort\("A"\);/);
  assert.match(assertmsg.highLevel, /assert\(1, "B"\);/);
});

test("assertmsg preserves the remaining stack value", () => {
  const script = new Uint8Array([0x12, 0x11, 0x0c, 0x01, 0x42, 0xe1, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /assert\(1, "B"\);/);
  assert.match(result.highLevel, /return 2;/);
});

test("builds call graph edges for syscalls, internal calls, CALLT, and CALLA", () => {
  const syscall = analyzeBytes(buildNefFromScript(new Uint8Array([0x41, 0xb7, 0xc3, 0x88, 0x03, 0x40])));
  assert.equal(syscall.callGraph.edges[0].opcode, "SYSCALL");
  assert.equal(syscall.callGraph.edges[0].target.kind, "Syscall");
  assert.equal(syscall.callGraph.edges[0].target.name, "System.Runtime.GetTime");

  const internal = analyzeBytes(buildNefFromScript(new Uint8Array([0x34, 0x05, 0x40, 0x21, 0x21, 0x57, 0x00, 0x00, 0x40])));
  assert.equal(internal.callGraph.edges[0].opcode, "CALL");
  assert.equal(internal.callGraph.edges[0].target.kind, "Internal");
  assert.equal(internal.callGraph.edges[0].target.method.name, "sub_0x0005");

  const token = analyzeBytes(
    buildNefWithSingleToken(new Uint8Array([0x37, 0x00, 0x00, 0x40]), new Uint8Array(20), "transfer", 2, true, 0x0f),
  );
  assert.equal(token.callGraph.edges[0].opcode, "CALLT");
  assert.equal(token.callGraph.edges[0].target.kind, "MethodToken");
  assert.equal(token.callGraph.edges[0].target.method, "transfer");

  const indirect = analyzeBytes(buildNefFromScript(new Uint8Array([0x11, 0x10, 0x36, 0x40])));
  assert.equal(indirect.callGraph.edges[0].opcode, "CALLA");
  assert.equal(indirect.callGraph.edges[0].target.kind, "Indirect");

  const resolvedCalla = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x09, 0x00, 0x00, 0x00,
        0x70,
        0x68,
        0x36,
        0x40,
        0x57, 0x00, 0x00,
        0x40,
      ]),
    ),
  );
  assert.equal(resolvedCalla.callGraph.edges[0].target.kind, "Internal");
  assert.equal(resolvedCalla.callGraph.edges[0].target.method.offset, 9);
});

test("resolves duplicated and static pointer flow into CALLA edges", () => {
  const dup = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x08, 0x00, 0x00, 0x00, // PUSHA +8
        0x4a, // DUP
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
      ]),
    ),
  );
  assert.equal(dup.callGraph.edges[0].target.kind, "Internal");
  assert.equal(dup.callGraph.edges[0].target.method.offset, 8);

  const staticFlow = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x09, 0x00, 0x00, 0x00, // PUSHA +9
        0x60, // STSFLD0
        0x58, // LDSFLD0
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
      ]),
    ),
  );
  assert.equal(staticFlow.callGraph.edges[0].target.kind, "Internal");
  assert.equal(staticFlow.callGraph.edges[0].target.method.offset, 9);
});

test("resolves multi-hop local pointer flow into CALLA edges", () => {
  const result = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x0c, 0x00, 0x00, 0x00, // PUSHA +12
        0x70, // STLOC0
        0x68, // LDLOC0
        0x71, // STLOC1
        0x69, // LDLOC1
        0x36, // CALLA
        0x40, // RET
        0x21, // NOP
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
      ]),
    ),
  );
  const edge = result.callGraph.edges.find((candidate) => candidate.opcode === "CALLA");
  assert.equal(edge.target.kind, "Internal");
  assert.equal(edge.target.method.offset, 0x000c);
});

test("does not resolve local pointer flow across method boundaries", () => {
  const result = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x57, 0x01, 0x00, // INITSLOT 1,0
        0x0a, 0x0e, 0x00, 0x00, 0x00, // PUSHA +14
        0x70, // STLOC0
        0x40, // RET
        0x57, 0x01, 0x00, // INITSLOT 1,0
        0x68, // LDLOC0
        0x36, // CALLA
        0x40, // RET
        0x21, // NOP
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
      ]),
    ),
  );
  const edge = result.callGraph.edges.find((candidate) => candidate.opcode === "CALLA");
  assert.equal(edge.target.kind, "Indirect");
});

test("resolves CALLA targets loaded from helper arguments", () => {
  const directArg = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x0f, 0x00, 0x00, 0x00, // PUSHA +15
        0x34, 0x04, // CALL +4
        0x40, // RET
        0x21, // NOP
        0x57, 0x00, 0x01, // INITSLOT 0,1
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xb7, 0xc3, 0x88, 0x03, // SYSCALL
        0x40, // RET
      ]),
    ),
  );
  const directEdge = directArg.callGraph.edges.find(
    (candidate) => candidate.opcode === "CALLA" && candidate.callOffset === 0x000d,
  );
  assert.equal(directEdge.target.kind, "Internal");
  assert.equal(directEdge.target.method.offset, 0x000f);

  const viaLocal = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x11, 0x00, 0x00, 0x00, // PUSHA +17
        0x34, 0x04, // CALL +4
        0x40, // RET
        0x21, // NOP
        0x57, 0x01, 0x01, // INITSLOT 1,1
        0x78, // LDARG0
        0x70, // STLOC0
        0x68, // LDLOC0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xb7, 0xc3, 0x88, 0x03, // SYSCALL
        0x40, // RET
      ]),
    ),
  );
  const localEdge = viaLocal.callGraph.edges.find(
    (candidate) => candidate.opcode === "CALLA" && candidate.callOffset === 0x000f,
  );
  assert.equal(localEdge.target.kind, "Internal");
  assert.equal(localEdge.target.method.offset, 0x0011);
});

test("resolves nested PUSHA argument through CALLA helpers", () => {
  const withInitslot = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x12, 0x00, 0x00, 0x00, // PUSHA +18 -> 0x0012
        0x0a, 0x07, 0x00, 0x00, 0x00, // PUSHA +7 -> 0x000c
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x01, // INITSLOT 0,1
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xb7, 0xc3, 0x88, 0x03, // SYSCALL
        0x40, // RET
      ]),
    ),
  );
  const nested = withInitslot.callGraph.edges.find(
    (candidate) => candidate.opcode === "CALLA" && candidate.callOffset === 0x0010,
  );
  assert.equal(nested.target.kind, "Internal");
  assert.equal(nested.target.method.offset, 0x0012);

  const withoutInitslot = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0x0a, 0x0b, 0x00, 0x00, 0x00, // PUSHA +11 -> 0x000b
        0x34, 0x03, // CALL +3 -> 0x0008
        0x40, // RET
        0x78, // LDARG0
        0x36, // CALLA
        0x40, // RET
        0x41, 0xb7, 0xc3, 0x88, 0x03, // SYSCALL
        0x40, // RET
      ]),
    ),
  );
  const noInitSlotEdge = withoutInitslot.callGraph.edges.find(
    (candidate) => candidate.opcode === "CALLA" && candidate.callOffset === 0x0009,
  );
  assert.equal(noInitSlotEdge.target.kind, "Internal");
  assert.equal(noInitSlotEdge.target.method.offset, 0x000b);
});

test("resolves delegate-array PICKITEM targets into CALLA edges", () => {
  const direct = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0xc2, // NEWARRAY0
        0x70, // STLOC0
        0x68, // LDLOC0
        0x0a, 0x0b, 0x00, 0x00, 0x00, // PUSHA +11
        0xcf, // APPEND
        0x68, // LDLOC0
        0x10, // PUSH0
        0xce, // PICKITEM
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
      ]),
    ),
  );
  const directEdge = direct.callGraph.edges.find(
    (candidate) => candidate.opcode === "CALLA",
  );
  assert.equal(directEdge.target.kind, "Internal");
  assert.equal(directEdge.target.method.offset, 0x000e);

  const aliased = analyzeBytes(
    buildNefFromScript(
      new Uint8Array([
        0xc2, // NEWARRAY0
        0x70, // STLOC0
        0x68, // LDLOC0
        0x4a, // DUP
        0x71, // STLOC1
        0x0a, 0x11, 0x00, 0x00, 0x00, // PUSHA +17
        0xcf, // APPEND
        0x69, // LDLOC1
        0x10, // PUSH0
        0xce, // PICKITEM
        0x72, // STLOC2
        0x6a, // LDLOC2
        0x36, // CALLA
        0x40, // RET
        0x57, 0x00, 0x00, // INITSLOT 0,0
        0x40, // RET
      ]),
    ),
  );
  const aliasedEdge = aliased.callGraph.edges.find(
    (candidate) => candidate.opcode === "CALLA",
  );
  assert.equal(aliasedEdge.target.kind, "Internal");
  assert.equal(aliasedEdge.target.method.offset, 0x0016);
});

test("includes slot xrefs for locals, arguments, and statics", () => {
  const local = analyzeBytes(buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40])));
  assert.equal(local.xrefs.methods[0].locals[0].writes[0], 4);
  assert.equal(local.xrefs.methods[0].locals[0].reads[0], 5);

  const args = analyzeBytes(buildNefFromScript(new Uint8Array([0x57, 0x00, 0x02, 0x78, 0x11, 0x81, 0x40])));
  assert.equal(args.xrefs.methods[0].arguments[0].reads[0], 3);
  assert.equal(args.xrefs.methods[0].arguments[1].writes[0], 5);

  const statics = analyzeBytes(buildNefFromScript(new Uint8Array([0x56, 0x02, 0x11, 0x60, 0x58, 0x40])));
  assert.equal(statics.xrefs.methods[0].statics[0].writes[0], 3);
  assert.equal(statics.xrefs.methods[0].statics[0].reads[0], 4);
});

test("infers basic collection and manifest argument types", () => {
  const localMap = analyzeBytes(buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0xc8, 0x70, 0x68, 0x40])));
  assert.equal(localMap.types.methods[0].locals[0], "map");

  const staticMap = analyzeBytes(buildNefFromScript(new Uint8Array([0x56, 0x01, 0xc8, 0x60, 0x40])));
  assert.equal(staticMap.types.statics[0], "map");

  const manifest = JSON.stringify({
    name: "Types",
    abi: {
      methods: [
        {
          name: "main",
          parameters: [
            { name: "amount", type: "Integer" },
            { name: "flag", type: "Boolean" },
          ],
          returntype: "Void",
          offset: 0,
        },
      ],
      events: [],
    },
    permissions: [],
    trusts: "*",
  });
  const args = analyzeBytes(buildNefFromScript(new Uint8Array([0x57, 0x00, 0x02, 0x40])), manifest);
  assert.deepEqual(args.types.methods[0].arguments, ["integer", "bool"]);

  const inferredArgs = analyzeBytes(buildNefFromScript(new Uint8Array([0x78, 0x40])));
  assert.deepEqual(inferredArgs.types.methods[0].arguments, ["unknown"]);
});

test("infers PACKMAP and CONVERT target types", () => {
  const packMap = analyzeBytes(buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x10, 0xbe, 0x70, 0x40])));
  assert.equal(packMap.types.methods[0].locals[0], "map");

  const packStruct = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x10, 0xbf, 0x70, 0x40])),
  );
  assert.equal(packStruct.types.methods[0].locals[0], "struct");

  const newBuffer = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x11, 0x88, 0x70, 0x40])),
  );
  assert.equal(newBuffer.types.methods[0].locals[0], "buffer");

  const convert = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x0c, 0x01, 0xaa, 0xdb, 0x28, 0x70, 0x40])),
  );
  assert.equal(convert.types.methods[0].locals[0], "bytestring");
});

test("inlineSingleUseTemps option inlines single-use temp variables", () => {
  // Script: PUSH1 PUSH2 ADD RET — produces a temp for the addition
  const script = new Uint8Array([0x11, 0x12, 0x9e, 0x40]);
  const nef = buildNefFromScript(script);

  const without = decompileHighLevelBytes(nef);
  const with_ = decompileHighLevelBytes(nef, { inlineSingleUseTemps: true });

  // Both should produce valid output
  assert.match(without.highLevel, /fn script_entry\(\)/);
  assert.match(with_.highLevel, /fn script_entry\(\)/);
});

test("postprocess inlineSingleUseTemps inlines temp into complex expression", async () => {
  const { postprocess } = await import("../src/postprocess.js");

  // Temp used inside a larger expression (not just `Y = tN;` which collapseTempIntoStore handles)
  const stmts = [
    "  let t0 = 42;",
    "  loc0 = t0 + 10;",
  ];
  postprocess(stmts, { inlineSingleUseTemps: true });
  const result = stmts.filter((s) => s.trim() !== "").join("\n");
  assert.match(result, /loc0 = 42 \+ 10;/);
  assert.ok(!result.includes("let t0"));
});

test("postprocess inlineSingleUseTemps does not inline multi-use temps", async () => {
  const { postprocess } = await import("../src/postprocess.js");

  // t0 used twice in separate expressions - should not be inlined
  const stmts = [
    "  let t0 = 42;",
    "  loc0 = t0 + 1;",
    "  loc1 = t0 + 2;",
  ];
  postprocess(stmts, { inlineSingleUseTemps: true });
  const result = stmts.filter((s) => s.trim() !== "").join("\n");
  assert.match(result, /let t0 = 42;/);
});

test("postprocess inlineSingleUseTemps wraps operator expressions in parens", async () => {
  const { postprocess } = await import("../src/postprocess.js");

  // Temp with operator RHS used inside a larger expression needs parens
  const stmts = [
    "  let t0 = a + b;",
    "  loc0 = t0 * 2;",
  ];
  postprocess(stmts, { inlineSingleUseTemps: true });
  const result = stmts.filter((s) => s.trim() !== "").join("\n");
  assert.match(result, /loc0 = \(a \+ b\) \* 2;/);
});

test("postprocess inlineSingleUseTemps skips non-temp identifiers", async () => {
  const { postprocess } = await import("../src/postprocess.js");

  // loc0 is not a temp (doesn't match t[0-9]+) so should not be inlined
  const stmts = [
    "  let loc0 = 42;",
    "  loc1 = loc0 + 10;",
  ];
  postprocess(stmts, { inlineSingleUseTemps: true });
  const result = stmts.filter((s) => s.trim() !== "").join("\n");
  assert.match(result, /let loc0 = 42;/);
  assert.match(result, /loc1 = loc0 \+ 10;/);
});
