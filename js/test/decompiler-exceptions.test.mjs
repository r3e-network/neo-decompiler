import assert from "node:assert/strict";
import test from "node:test";

import {
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
} from "../src/index.js";
import { buildNefFromScript } from "./decompiler-fixtures.mjs";

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

test("matches an outer TRY_L body past a nested ENDTRY", () => {
  // The outer handler starts after the ENDTRY that closes the outer body;
  // the inner ENDTRY must not become the outer slice boundary.
  const script = new Uint8Array([
    0x3c, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // TRY_L, catch at 0x14
    0x3b, 0x06, 0x00, // nested TRY, catch at 0x0F
    0x11, // nested try body
    0x3d, 0x04, // nested ENDTRY -> nested resume
    0x12, // nested catch body
    0x3d, 0x02, // nested catch ENDTRY -> outer ENDTRY
    0x3d, 0x02, // outer ENDTRY -> outer catch at 0x14
    0x13, // outer catch body
    0x3d, 0x02, // outer catch ENDTRY -> RET
    0x40,
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /try \{/);
  assert.match(result.highLevel, /catch \{/);
  assert.doesNotMatch(result.highLevel, /TRY_L .*not yet translated/);
  assert.doesNotMatch(result.highLevel, /TRY .*not yet translated/);
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

test("seeds manifest parameters for methods without INITSLOT", () => {
  const script = new Uint8Array([0x78, 0x40]); // LDARG0; RET
  const manifest = {
    name: "PropertyContract",
    groups: [],
    features: {},
    supportedstandards: [],
    permissions: [],
    trusts: [],
    abi: {
      methods: [{
        name: "setValue",
        parameters: [{ name: "value", type: "String" }],
        returntype: "String",
        offset: 0,
        safe: false,
      }],
      events: [],
    },
  };
  const result = decompileHighLevelBytesWithManifest(buildNefFromScript(script), manifest);
  assert.match(result.highLevel, /fn setValue\(value: string\) -> string \{/);
  assert.match(result.highLevel, /return value;/);
  assert.doesNotMatch(result.highLevel, /\?\?\?/);
});

test("lifts compiler-generated catch-only regions using the normal ENDTRY target", () => {
  const script = new Uint8Array([
    0x3b, 0x08, 0x00, // TRY: catch handler at 0x08
    0x11, // normal body value
    0x3d, 0x08, // ENDTRY: normal path resumes at 0x0C
    0x21, 0x21, // padding between the body transfer and handler
    0x70, // catch stores the exception
    0x12, // catch payload
    0x3a, // catch throws the payload
    0x21, // padding before the shared resume block
    0x40, // RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /try \{/);
  assert.match(result.highLevel, /catch \{/);
  assert.match(result.highLevel, /let loc0 = exception;/);
  assert.match(result.highLevel, /throw\(2\);/);
  assert.doesNotMatch(result.highLevel, /TRY .*not yet translated/);
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

test("DROP followed by THROW preserves a payloadless VM throw", () => {
  const result = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x45, 0x3A, 0x40])),
  );
  assert.match(result.highLevel, /throw\(\);/);
  assert.doesNotMatch(result.highLevel, /\?\?\?/);
  assert.match(result.csharp, /throw new Exception\(\);/);
  assert.doesNotMatch(result.csharp, /default\(dynamic\)/);
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
