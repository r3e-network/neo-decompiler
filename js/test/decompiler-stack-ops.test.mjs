import assert from "node:assert/strict";
import test from "node:test";

import { decompileHighLevelBytes } from "../src/index.js";
import { buildNefFromScript } from "./decompiler-fixtures.mjs";

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
