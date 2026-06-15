// Regression tests for stack-semantics fixes that must stay byte-identical
// to the Rust port: syscall argument order, PACKMAP 2n pop arity, invalid
// StackItemType fallbacks, and NEWMAP/NEWSTRUCT0 rendering.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";

import { decompileHighLevelBytes } from "../src/index.js";

function computeChecksum(payload) {
  const bytes = Uint8Array.from(payload);
  const first = createHash("sha256").update(bytes).digest();
  const second = createHash("sha256").update(first).digest();
  return second.subarray(0, 4);
}

function buildNef(scriptBytes) {
  const script = Array.from(scriptBytes);
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("test"), 0);
  data.push(...compiler);
  data.push(0); // source varint (empty)
  data.push(0); // reserved byte
  data.push(0); // token count
  data.push(0x00, 0x00); // reserved word
  data.push(script.length, ...script); // script varbytes (1-byte prefix)
  data.push(...computeChecksum(data));
  return new Uint8Array(data);
}

function syscallHash(name) {
  return Array.from(createHash("sha256").update(name).digest().subarray(0, 4));
}

function highLevel(scriptBytes) {
  return decompileHighLevelBytes(buildNef(scriptBytes), { clean: true }).highLevel;
}

test("stack-semantics: multi-arg syscall renders arguments in pop (declaration) order", () => {
  // NEWARRAY0 (state) ; PUSHDATA1 "evt" (eventName, on top) ; SYSCALL Notify ; RET
  // Notify(eventName, state): eventName is on top, popped first, so it
  // renders first. A reversal bug rendered ([], "evt").
  const script = [0xc2, 0x0c, 0x03, 0x65, 0x76, 0x74, 0x41, ...syscallHash("System.Runtime.Notify"), 0x40];
  const output = highLevel(script);
  assert.match(output, /syscall\("System\.Runtime\.Notify", "evt", \[\]\)/);
});

test("stack-semantics: three-arg syscall preserves pop order", () => {
  // PUSH1 ; PUSH2 ; PUSH3 ; SYSCALL Storage.Put ; RET
  // Pop order 3, 2, 1 equals declaration order (context, key, value).
  const script = [0x11, 0x12, 0x13, 0x41, ...syscallHash("System.Storage.Put"), 0x40];
  const output = highLevel(script);
  assert.match(output, /syscall\("System\.Storage\.Put", 3, 2, 1\)/);
});

test("stack-semantics: PACKMAP pops key/value pairs (2n) and renders entries", () => {
  // PUSH4 ; PUSH3 ; PUSH2 ; PUSH1 ; PUSH2 (count) ; PACKMAP ; RET
  // count=2 then key=1,value=2,key=3,value=4 — 2n+1 pops total.
  const output = highLevel([0x14, 0x13, 0x12, 0x11, 0x12, 0xbe, 0x40]);
  assert.match(output, /Map\(1: 2, 3: 4\)/);
});

test("stack-semantics: invalid StackItemType byte surfaces as raw hex in both fallbacks", () => {
  // PUSH1 ; NEWARRAY_T 0x99 ; DROP ; PUSH1 ; ISTYPE 0x99 ; RET
  const output = highLevel([0x11, 0xc4, 0x99, 0x45, 0x11, 0xd9, 0x99, 0x40]);
  assert.match(output, /new_array_t\(1, 0x99\)/);
  assert.match(output, /is_type\(1, 0x99\)/);
});

test("stack-semantics: NEWMAP renders Map() and NEWSTRUCT0 renders Struct()", () => {
  // NEWMAP ; NEWSTRUCT0 ; RET
  const output = highLevel([0xc8, 0xc5, 0x40]);
  assert.match(output, /= Map\(\);/);
  assert.match(output, /Struct\(\)/);
  assert.doesNotMatch(output, /= \{\};/);
});

test("stack-semantics: pathological PACK count terminates and does not balloon output", () => {
  // PUSHINT32 0x7fffffff ; PACK ; RET — the count is attacker-controlled
  // but the emitter caps inline rendering and drains only the real stack.
  const output = highLevel([0x02, 0xff, 0xff, 0xff, 0x7f, 0xc0, 0x40]);
  assert.ok(output.length < 4096, "output must stay bounded for a huge PACK count");
});

test("switch guard: scrutinee-mutating standalone if-chain is not folded into a switch", async () => {
  const { postprocess } = await import("../src/postprocess.js");
  const statements = [
    "if loc0 == 0 {",
    "    loc0 = 1;",
    "    do0;",
    "}",
    "if loc0 == 1 {",
    "    do1;",
    "}",
    "if loc0 == 2 {",
    "    do2;",
    "}",
  ];
  postprocess(statements, {});
  assert.ok(
    !statements.some((line) => line.trim() === "switch loc0 {"),
    "a case body reassigning the scrutinee must block the switch rewrite",
  );
});

test("switch guard: terminator-ending standalone if-chain still folds into a switch", async () => {
  const { postprocess } = await import("../src/postprocess.js");
  const statements = [
    "if loc0 == 0 {",
    "    return 0;",
    "}",
    "if loc0 == 1 {",
    "    return 1;",
    "}",
    "if loc0 == 2 {",
    "    return 2;",
    "}",
  ];
  postprocess(statements, {});
  assert.ok(
    statements.some((line) => line.trim() === "switch loc0 {"),
    "terminator-ending case bodies should fold into a switch",
  );
});
