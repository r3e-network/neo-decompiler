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
  assert.match(result.highLevel, /if 1 !== 1 \{/);
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

test("uses label-style gotos for generic jump fallbacks", () => {
  const script = new Uint8Array([0x22, 0x02, 0x23, 0x05, 0x00, 0x00, 0x00, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /goto label_0x0002;/);
  assert.match(result.highLevel, /goto label_0x0007;/);
  assert.match(result.highLevel, /label_0x0002:/);
  assert.match(result.highLevel, /label_0x0007:/);
});

test("uses label-style leave fallbacks for ENDTRY transfers", () => {
  const script = new Uint8Array([0x3d, 0x02, 0x3e, 0x05, 0x00, 0x00, 0x00, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /leave label_0x0002;/);
  assert.match(result.highLevel, /leave label_0x0007;/);
  assert.match(result.highLevel, /label_0x0002:/);
  assert.match(result.highLevel, /label_0x0007:/);
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
  assert.match(result.highLevel, /===/);
  assert.match(result.highLevel, /!==/);
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
  assert.match(result.highLevel, /return \[\]\[0\];/);
});

test("rewrites SETITEM as index assignment", () => {
  const script = new Uint8Array([0xc8, 0x10, 0x11, 0xd0, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /\{\}\[0\] = 1;/);
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
  assert.match(result.highLevel, /return has_key\(\{\}, 0\);/);
});

test("unpack of stored packed value keeps reverse3 stack shape", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, 0x12, 0x12, 0xc0, 0x70, // PUSH1; PUSH2; PUSH2; PACK; STLOC0
    0x13, 0x68, 0xc1, 0x45, 0x53, 0x40, // PUSH3; LDLOC0; UNPACK; DROP; REVERSE3; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /let loc0 = \[2, 1\];/);
  assert.match(result.highLevel, /reverse top 3 stack values/);
});

test("pick preserves packed shape metadata for unpack reverse4", () => {
  const script = new Uint8Array([
    0x57, 0x01, 0x00, // INITSLOT 1 local, 0 args
    0x11, 0x12, 0x12, 0xc0, 0x70, // PUSH1; PUSH2; PUSH2; PACK; STLOC0
    0x13, 0x68, 0x10, 0x4d, 0xc1, 0x45, 0x54, 0x40, // PUSH3; LDLOC0; PUSH0; PICK; UNPACK; DROP; REVERSE4; RET
  ]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /reverse top 4 stack values/);
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
  assert.match(keys.highLevel, /return keys\(\{\}\);/);
  assert.match(values.highLevel, /return values\(\{\}\);/);
});

test("rewrites APPEND and POPITEM using collection helpers", () => {
  const appendScript = new Uint8Array([0xc2, 0x11, 0xcf, 0x40]);
  const popitemScript = new Uint8Array([0xc2, 0xd4, 0x40]);
  const append = decompileHighLevelBytes(buildNefFromScript(appendScript));
  const popitem = decompileHighLevelBytes(buildNefFromScript(popitemScript));
  assert.match(append.highLevel, /append\(\[\], 1\);/);
  assert.match(popitem.highLevel, /return pop_item\(\[\]\);/);
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

test("describes XDROP with a dynamic index without unsupported placeholder", () => {
  const script = new Uint8Array([0x57, 0x00, 0x01, 0x11, 0x12, 0x78, 0x48, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /xdrop stack\[arg0\]/);
  assert.doesNotMatch(result.highLevel, /unsupported dynamic XDROP/);
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

test("lifts ROT into a stack reorder comment", () => {
  const script = new Uint8Array([0x11, 0x12, 0x13, 0x51, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /rotate top three stack values/);
});

test("lifts TUCK into a stack reorder comment", () => {
  const script = new Uint8Array([0x11, 0x12, 0x4e, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /tuck top of stack/);
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

test("lifts REVERSEN into a reverse comment", () => {
  const script = new Uint8Array([0x11, 0x12, 0x13, 0x13, 0x55, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /reverse top 3 stack values/);
});

test("lifts REVERSEN with a dynamic count comment", () => {
  const script = new Uint8Array([0x57, 0x00, 0x01, 0x11, 0x12, 0x78, 0x55, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /reverse top arg0 stack values/);
  assert.doesNotMatch(result.highLevel, /unsupported dynamic REVERSEN/);
});

test("lifts DEPTH and CLEAR stack ops", () => {
  const depthResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x12, 0x43, 0x40])),
  );
  const clearResult = decompileHighLevelBytes(
    buildNefFromScript(new Uint8Array([0x11, 0x49, 0x40])),
  );
  assert.match(depthResult.highLevel, /return 2;/);
  assert.match(clearResult.highLevel, /clear stack/);
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

test("emits slot declaration comments from INITSLOT", () => {
  const script = new Uint8Array([0x57, 0x01, 0x00, 0x11, 0x70, 0x68, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /\/\/ declare 1 locals, 0 arguments/);
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
  assert.match(result.highLevel, /\/\/ declare 2 static slots/);
  assert.match(result.highLevel, /let static0 = 1;/);
  assert.match(result.highLevel, /return static0;/);
});

test("rewrites SIZE and ISNULL helpers", () => {
  const sizeScript = new Uint8Array([0xc2, 0xca, 0x40]);
  const nullScript = new Uint8Array([0x0b, 0xd8, 0x40]);
  const size = decompileHighLevelBytes(buildNefFromScript(sizeScript));
  const isNull = decompileHighLevelBytes(buildNefFromScript(nullScript));
  assert.match(size.highLevel, /return len\(\[\]\);/);
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
  assert.match(remove.highLevel, /remove_item\(\{\}, 0\);/);
  assert.match(clear.highLevel, /clear_items\(\{\}\);/);
  assert.match(reverse.highLevel, /reverse_items\(\[\]\);/);
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
  assert.match(abortmsg.highLevel, /abort\(0x41\);/);
  assert.match(assertmsg.highLevel, /assert\(1, 0x42\);/);
});

test("assertmsg preserves the remaining stack value", () => {
  const script = new Uint8Array([0x12, 0x11, 0x0c, 0x01, 0x42, 0xe1, 0x40]);
  const result = decompileHighLevelBytes(buildNefFromScript(script));
  assert.match(result.highLevel, /assert\(1, 0x42\);/);
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
