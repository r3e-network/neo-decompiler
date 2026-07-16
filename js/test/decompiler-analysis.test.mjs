import assert from "node:assert/strict";
import test from "node:test";

import { analyzeBytes, decompileHighLevelBytes } from "../src/index.js";
import {
  buildNefFromScript,
  buildNefWithSingleToken,
} from "./decompiler-fixtures.mjs";

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

test("call graph: an out-of-range CALL target is unresolved, not a fabricated method", () => {
  // CALL +127 (target 0x7F) in a 3-byte script — the target is far past the
  // script end, so it must be UnresolvedInternal, not a fabricated sub_0x007F
  // Internal edge/method. Mirrors the Rust port (and the negative-target case).
  const a = analyzeBytes(buildNefFromScript(new Uint8Array([0x34, 0x7f, 0x40])));
  assert.equal(a.callGraph.edges[0].target.kind, "UnresolvedInternal");
  assert.equal(a.callGraph.edges[0].target.target, 127);
  assert.ok(
    a.callGraph.methods.every((m) => m.offset !== 127),
    "out-of-range CALL must not fabricate a method",
  );
});

test("call graph: an out-of-range PUSHA+CALLA target is Indirect, not a fabricated method", () => {
  // PUSHA +127 (target 0x7F) ; CALLA ; RET — the pointer lands past the script
  // end, so the CALLA must be Indirect, not a fabricated sub_0x007F Internal
  // edge/method. Mirrors the Rust port.
  const a = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x0a, 0x7f, 0x00, 0x00, 0x00, 0x36, 0x40])),
  );
  const edge = a.callGraph.edges.find((e) => e.opcode === "CALLA");
  assert.equal(edge.target.kind, "Indirect");
  assert.ok(
    a.callGraph.methods.every((m) => m.offset !== 127),
    "out-of-range CALLA must not fabricate a method",
  );
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
        0x0a, 0x0d, 0x00, 0x00, 0x00, // PUSHA +13 (target 0x0012, a valid in-range helper)
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
  assert.equal(aliasedEdge.target.method.offset, 0x0012);
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

test("infers integer and boolean local types via stack simulation", () => {
  // The type inferencer is a full stack simulation (mirroring the Rust core),
  // so integer/boolean locals are recovered, not just collection kinds.

  // PUSH5; STLOC0 — an integer literal stored.
  const intLiteral = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x15, 0x70, 0x40])),
  );
  assert.equal(intLiteral.types.methods[0].locals[0], "integer");

  // PUSH1; PUSH1; ADD; STLOC0 — an arithmetic result is integer.
  const arithmetic = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x11, 0x11, 0x9e, 0x70, 0x40])),
  );
  assert.equal(arithmetic.types.methods[0].locals[0], "integer");

  // PUSH1; PUSH3; LT; STLOC0 — a comparison result is boolean.
  const comparison = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0x11, 0x13, 0xb5, 0x70, 0x40])),
  );
  assert.equal(comparison.types.methods[0].locals[0], "bool");

  // NEWARRAY0; SIZE; STLOC0 — SIZE yields an integer.
  const size = analyzeBytes(
    buildNefFromScript(new Uint8Array([0x57, 0x01, 0x00, 0xc2, 0xca, 0x70, 0x40])),
  );
  assert.equal(size.types.methods[0].locals[0], "integer");
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
