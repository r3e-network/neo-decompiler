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

test("stack-semantics: PACK with a computed (non-literal) count renders pack_dynamic", () => {
  // PUSH7 PUSH8 PUSH1 PUSH1 ADD PACK RET — count = 1 + 1 (composite). A loose
  // parseInt("1 + 1") folds it to 1 and packs only [8]; the honest lift is the
  // dynamic form (the VM packs 2 elements), matching the Rust port.
  const out = highLevel([0x17, 0x18, 0x11, 0x11, 0x9e, 0xc0, 0x40]);
  assert.match(out, /pack_dynamic\(1 \+ 1\)/);
  assert.doesNotMatch(out, /return \[8\];/);
  // A bare literal count still packs statically.
  assert.match(highLevel([0x17, 0x18, 0x12, 0xc0, 0x40]), /\[8, 7\]/);
});

test("stack-semantics: unary NEGATE/INVERT/NOT preserve precedence over a compound operand", () => {
  // PUSH2 ; PUSH3 ; ADD ; NEGATE ; RET — the negation applies to the whole
  // sum. A bare `-2 + 3` would parse as `(-2) + 3` (= 1), not `-(2 + 3)`
  // (= -5), so the operand must be parenthesised to match the Rust port.
  assert.match(highLevel([0x12, 0x13, 0x9e, 0x9b, 0x40]), /return -\(2 \+ 3\);/);
  // INVERT (0x90) and NOT (0xaa) bind the same way.
  assert.match(highLevel([0x12, 0x13, 0x9e, 0x90, 0x40]), /return ~\(2 \+ 3\);/);
  assert.match(highLevel([0x12, 0x13, 0x9e, 0xaa, 0x40]), /return !\(2 \+ 3\);/);
  // A simple operand stays bare (no spurious parentheses).
  assert.match(highLevel([0x12, 0x9b, 0x40]), /return -2;/);
});

test("stack-semantics: ROLL/PICK with a computed (non-literal) index fall to the dynamic form", () => {
  // PUSH10 PUSH11 PUSH12 PUSH1 PUSH1 ADD ROLL RET — the index is `1 + 1`, a
  // composite expression. A loose parseInt("1 + 1") folds it to slot 1 and
  // emits a confidently-wrong `return 11;`; the honest lift is a dynamic roll
  // (the VM rolls slot 2 = 10), matching the Rust port.
  const roll = highLevel([0x1a, 0x1b, 0x1c, 0x11, 0x11, 0x9e, 0x52, 0x40]);
  assert.match(roll, /roll\(1 \+ 1\)/);
  assert.doesNotMatch(roll, /return 11;/);
  // PICK with the same computed index → dynamic pick, not a fabricated slot.
  const pick = highLevel([0x1a, 0x1b, 0x1c, 0x11, 0x11, 0x9e, 0x4d, 0x40]);
  assert.match(pick, /pick\(1 \+ 1\)/);
  assert.doesNotMatch(pick, /return 11;/);
  // A bare literal index still resolves statically (PUSH2 ROLL → slot 2 = 10).
  assert.match(highLevel([0x1a, 0x1b, 0x1c, 0x12, 0x52, 0x40]), /return 10;/);
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

test("stack-semantics: PACK with count > inline cap still drains every consumed element", () => {
  // INITSLOT(1 local) ; 70x PUSH1 ; PUSHINT8 70 ; PACK ; STLOC0 ; RET
  // The inline render is capped at 64 entries, but the VM pops all 70. The
  // simulated stack must be fully drained, otherwise STLOC0 stores the array
  // while stale operands remain and RET returns one of them (`return 1;`).
  const script = [0x57, 0x01, 0x00];
  for (let i = 0; i < 70; i += 1) script.push(0x11);
  script.push(0x00, 70, 0xc0, 0x70, 0x40);
  const output = highLevel(script);
  assert.match(output, /\/\* 6 more elements \*\//);
  assert.match(output, /return;/);
  assert.ok(
    !/return 1;/.test(output),
    `stale packed operands must be drained, not returned: ${output}`,
  );
});

test("postprocess: empty-if/else inversion keeps braces balanced", async () => {
  const { postprocess } = await import("../src/postprocess.js");
  const statements = [
    "if cond {",
    "}",
    "else {",
    "loc0 = 5;",
    "}",
    "return loc0;",
  ];
  postprocess(statements, {});
  const balance = statements.reduce((acc, line) => {
    const t = line.trim();
    return acc + (t.match(/{/g) || []).length - (t.match(/}/g) || []).length;
  }, 0);
  assert.equal(balance, 0, `unbalanced braces: ${JSON.stringify(statements)}`);
  assert.ok(
    statements.join("\n").includes("if !(cond) {\nloc0 = 5;\n}"),
    `inverted if must retain the else body and its closer: ${JSON.stringify(statements)}`,
  );
});

test("postprocess: ' get '/' has_key ' inside a string literal is not rewritten as an index", async () => {
  const { postprocess } = await import("../src/postprocess.js");
  const getCase = ['return "a get b";'];
  postprocess(getCase, {});
  assert.equal(getCase[0], 'return "a get b";');
  const hasKeyCase = ['return "x has_key y";'];
  postprocess(hasKeyCase, {});
  assert.equal(hasKeyCase[0], 'return "x has_key y";');
  // A real ` get ` operator outside strings is still rewritten to indexing.
  const real = ["let t0 = loc0 get t1;"];
  postprocess(real, {});
  assert.equal(real[0], "let t0 = loc0[t1];");
});

test("postprocess: switch fold preserves non-temp inter-case statements", async () => {
  const { postprocess } = await import("../src/postprocess.js");
  const statements = [
    "if loc0 == 0 {",
    "    do0();",
    "}",
    "loc5 = side_effect();",
    "if loc0 == 1 {",
    "    do1();",
    "}",
    "if loc0 == 2 {",
    "    do2();",
    "}",
  ];
  postprocess(statements, {});
  assert.ok(
    statements.some((line) => line.trim() === "loc5 = side_effect();"),
    `non-temp inter-case statement must survive: ${JSON.stringify(statements)}`,
  );
});

test("postprocess: switch fold preserves a side-effecting temp between cases", async () => {
  const { postprocess } = await import("../src/postprocess.js");
  const statements = [
    "if loc0 == 0 {",
    "    do0();",
    "    return;",
    "}",
    "let t7 = Foo(arg);",
    "if loc0 == 1 {",
    "    do1();",
    "    return;",
    "}",
    "if loc0 == 2 {",
    "    do2();",
    "    return;",
    "}",
  ];
  postprocess(statements, {});
  assert.ok(
    statements.some((line) => line.trim() === "let t7 = Foo(arg);"),
    `side-effecting temp must survive: ${JSON.stringify(statements)}`,
  );
});
