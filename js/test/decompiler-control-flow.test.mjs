import assert from "node:assert/strict";
import test from "node:test";

import {
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
} from "../src/index.js";
import {
  GAS_TOKEN_HASH,
  SAMPLE_MANIFEST,
  buildLocalMathNef,
  buildNefFromScript,
  buildNefWithSingleToken,
  buildSampleNef,
} from "./decompiler-fixtures.mjs";

test("lifts straight-line arithmetic into a high-level return", () => {
  const result = decompileHighLevelBytes(buildSampleNef());
  assert.match(result.highLevel, /fn script_entry\(\) -> any \{/);
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

test("keeps a manifest-known void helper CALL as a statement", () => {
  const script = new Uint8Array([
    0x34, 0x03, // CALL +3 -> helper at 0x0003
    0x40, // RET
    0x40, // helper: RET
  ]);
  const manifest = {
    name: "VoidCalls",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0 },
        { name: "helper", parameters: [], returntype: "Void", offset: 3 },
      ],
      events: [],
    },
  };

  const result = decompileHighLevelBytesWithManifest(
    buildNefFromScript(script),
    manifest,
  );

  assert.match(
    result.highLevel,
    /fn main\(\) \{\s+helper\(\);\s+return;/,
    "a void helper call must not disappear at the caller's RET",
  );
});

test("keeps a manifest-known void helper CALLA as a statement", () => {
  const script = new Uint8Array([
    0x0a, 0x07, 0x00, 0x00, 0x00, // PUSHA +7 -> helper at 0x0007
    0x36, // CALLA
    0x40, // RET
    0x40, // helper: RET
  ]);
  const manifest = {
    name: "VoidCalls",
    abi: {
      methods: [
        { name: "main", parameters: [], returntype: "Void", offset: 0 },
        { name: "helper", parameters: [], returntype: "Void", offset: 7 },
      ],
      events: [],
    },
  };

  const result = decompileHighLevelBytesWithManifest(
    buildNefFromScript(script),
    manifest,
  );

  assert.match(
    result.highLevel,
    /fn main\(\) \{\s+helper\(\);\s+return;/,
    "a resolved void helper call must not disappear at the caller's RET",
  );
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
