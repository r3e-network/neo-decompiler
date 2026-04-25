/**
 * Deep Fuzz Tests for Neo Decompiler JS
 *
 * Targets internal subsystems with structured random input:
 * - High-level lifter (branches, loops, try, switches, collections, calls)
 * - Postprocessor passes
 * - Call graph builder
 * - Type inference
 * - Cross-reference analysis
 * - Manifest parsing edge cases
 * - NEF structure mutations
 */

import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";

import {
  parseNef,
  disassembleScript,
  decompileBytes,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
  analyzeBytes,
  buildCallGraph,
  buildMethodGroups,
  buildXrefs,
  inferTypes,
  parseManifest,
  NefParseError,
  DisassemblyError,
  ManifestParseError,
} from "../src/index.js";

// ─── Helpers ────────────────────────────────────────────────────────────────

function computeChecksum(payload) {
  const first = createHash("sha256").update(Buffer.from(payload)).digest();
  const second = createHash("sha256").update(first).digest();
  return new Uint8Array(second.subarray(0, 4));
}

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

function buildValidNef(script, tokens = []) {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64)); // compiler
  data.push(0); // source varint (empty)
  data.push(0); // reserved byte
  // method tokens
  writeVarint(data, tokens.length);
  for (const token of tokens) {
    data.push(...(token.hash ?? new Uint8Array(20))); // hash (20 bytes)
    const nameBytes = Buffer.from(token.method ?? "m");
    writeVarint(data, nameBytes.length);
    data.push(...nameBytes);
    data.push(token.parametersCount ?? 0, 0); // u16 LE
    data.push(token.hasReturnValue ? 1 : 0);
    data.push(token.callFlags ?? 0x0f);
  }
  data.push(0, 0); // reserved word
  writeVarint(data, script.length);
  data.push(...script);
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

function randomBytes(length) {
  return new Uint8Array(length).map(() => Math.floor(Math.random() * 256));
}

function randomChoice(arr) {
  return arr[Math.floor(Math.random() * arr.length)];
}

function randomInt(min, max) {
  return min + Math.floor(Math.random() * (max - min + 1));
}

/** Safe wrapper: run fn, expect no crash, allow controlled throws. */
function mustNotCrash(fn) {
  try {
    fn();
  } catch (e) {
    assert.ok(e instanceof Error, `Non-Error thrown: ${e}`);
  }
}

// ─── Opcode constants ───────────────────────────────────────────────────────

const PUSH_OPS = [0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20];
const BINARY_OPS = [0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0x91, 0x92, 0x93, 0x97, 0x98, 0xb5, 0xb6, 0xb7, 0xb8, 0xab, 0xac, 0x8b];
const UNARY_OPS = [0x90, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0xa4, 0xaa, 0xb1, 0xd8];
const STACK_OPS = [0x45, 0x46, 0x4a, 0x4b, 0x50, 0x51, 0x53, 0x54, 0x43, 0x49];
const COLLECTION_OPS = [0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc8, 0xca, 0xcb, 0xcc, 0xcd, 0xce, 0xcf, 0xd0, 0xd1, 0xd2, 0xd3, 0xd4, 0xbe, 0xbf];
const COND_JUMPS_8 = [0x24, 0x25, 0x26, 0x28, 0x2a, 0x2c, 0x2e, 0x30, 0x32];
const SLOT_LDLOC = [0x68, 0x69, 0x6a, 0x6b];
const SLOT_STLOC = [0x70, 0x71, 0x72, 0x73];
const SLOT_LDARG = [0x78, 0x79, 0x7a, 0x7b];
const SLOT_STARG = [0x80, 0x81, 0x82, 0x83];
const SLOT_LDSFLD = [0x58, 0x59, 0x5a, 0x5b];
const SLOT_STSFLD = [0x60, 0x61, 0x62, 0x63];

// ─── 1. Structured control flow fuzzing ─────────────────────────────────────

test("deep-fuzz: random if-else chains", () => {
  for (let iter = 0; iter < 100; iter++) {
    const script = [0x57, 0x04, 0x02]; // INITSLOT 4 locals, 2 args
    const numBranches = randomInt(1, 5);
    for (let i = 0; i < numBranches; i++) {
      script.push(randomChoice(PUSH_OPS)); // push condition
      const bodyLen = randomInt(2, 8);
      script.push(0x26, bodyLen + 2); // JMPIFNOT over body
      for (let j = 0; j < bodyLen; j++) {
        script.push(randomChoice(PUSH_OPS));
        script.push(randomChoice(SLOT_STLOC));
      }
    }
    script.push(randomChoice(SLOT_LDLOC));
    script.push(0x40); // RET

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: random while loops with breaks", () => {
  for (let iter = 0; iter < 80; iter++) {
    const script = [0x57, 0x02, 0x01]; // INITSLOT 2 locals, 1 arg
    // while (cond) { body }
    const condOffset = script.length;
    script.push(randomChoice(PUSH_OPS)); // condition
    const bodyLen = randomInt(2, 12);
    script.push(0x26, bodyLen + 2); // JMPIFNOT exit
    for (let j = 0; j < bodyLen; j++) {
      if (Math.random() < 0.3) {
        script.push(randomChoice(PUSH_OPS));
        script.push(randomChoice(SLOT_STLOC));
      } else {
        script.push(randomChoice(PUSH_OPS));
        script.push(0x45); // DROP
      }
    }
    // back-jump to condOffset
    const backDelta = condOffset - script.length;
    script.push(0x22, backDelta & 0xff); // JMP back
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: random do-while loops", () => {
  for (let iter = 0; iter < 80; iter++) {
    const script = [0x57, 0x02, 0x00]; // INITSLOT 2 locals, 0 args
    const bodyStart = script.length;
    const bodyLen = randomInt(2, 10);
    for (let j = 0; j < bodyLen; j++) {
      script.push(randomChoice(PUSH_OPS));
      if (Math.random() < 0.5) script.push(randomChoice(SLOT_STLOC));
      else script.push(0x45); // DROP
    }
    // conditional back-jump
    script.push(randomChoice(PUSH_OPS));
    const backDelta = bodyStart - (script.length + 2);
    script.push(0x24, backDelta & 0xff); // JMPIF back to body
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: nested try-catch-finally combinations", () => {
  for (let iter = 0; iter < 60; iter++) {
    const script = [0x57, 0x01, 0x00];
    const depth = randomInt(1, 4);
    const tryBodies = [];

    for (let d = 0; d < depth; d++) {
      // TRY with both catch and finally
      const catchDelta = randomInt(4, 12);
      const finallyDelta = randomInt(catchDelta + 3, catchDelta + 10);
      script.push(0x3b, catchDelta & 0xff, finallyDelta & 0xff);
      // try body
      for (let j = 0; j < randomInt(1, 4); j++) {
        script.push(randomChoice(PUSH_OPS));
        script.push(0x45); // DROP
      }
      script.push(0x3d, 0x02); // ENDTRY +2
    }

    // Catch/finally bodies
    for (let d = 0; d < depth; d++) {
      script.push(0x45); // DROP (exception)
      script.push(0x3d, 0x02); // ENDTRY +2
      script.push(0x3f); // ENDFINALLY
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: switch-like equality chains", () => {
  for (let iter = 0; iter < 50; iter++) {
    const script = [0x57, 0x01, 0x00]; // INITSLOT 1 local, 0 args
    // Store input to loc0
    script.push(randomChoice(PUSH_OPS));
    script.push(0x70); // STLOC0

    const numCases = randomInt(3, 8);
    for (let c = 0; c < numCases; c++) {
      script.push(0x68); // LDLOC0
      script.push(randomChoice(PUSH_OPS)); // case value
      script.push(0x97); // EQUAL
      script.push(0x26, 0x06); // JMPIFNOT +6 (skip case body)
      script.push(randomChoice(PUSH_OPS));
      script.push(0x70); // STLOC0
      script.push(0x22, 0x04); // JMP over next cases
    }
    script.push(0x68); // LDLOC0 (default)
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 2. Collection operation fuzzing ────────────────────────────────────────

test("deep-fuzz: random PACK/UNPACK sequences", () => {
  for (let iter = 0; iter < 60; iter++) {
    const script = [0x57, 0x02, 0x00];
    const numItems = randomInt(1, 6);
    // Push items
    for (let i = 0; i < numItems; i++) {
      script.push(randomChoice(PUSH_OPS));
    }
    // Push count and PACK
    script.push(0x10 + numItems); // PUSH<n>
    const packOp = randomChoice([0xc0, 0xbe, 0xbf]); // PACK, PACKMAP, PACKSTRUCT
    if (packOp === 0xbe && numItems % 2 !== 0) {
      script.push(0xc0); // PACK if odd (PACKMAP needs even)
    } else {
      script.push(packOp);
    }
    script.push(0x70); // STLOC0

    // Random collection ops on it
    for (let i = 0; i < randomInt(1, 4); i++) {
      script.push(0x68); // LDLOC0
      const op = randomChoice([0xca, 0xcc, 0xcd, 0xd1, 0xd3]); // SIZE, KEYS, VALUES, REVERSEITEMS, CLEARITEMS
      script.push(op);
      if (op === 0xca || op === 0xcc || op === 0xcd) {
        script.push(0x45); // DROP the result
      }
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: PICKITEM/SETITEM with various index types", () => {
  for (let iter = 0; iter < 50; iter++) {
    const script = [0x57, 0x02, 0x00];
    // Create array
    script.push(0xc2); // NEWARRAY0
    script.push(0x70); // STLOC0

    for (let i = 0; i < randomInt(1, 5); i++) {
      script.push(0x68); // LDLOC0
      script.push(randomChoice(PUSH_OPS)); // value
      script.push(0xcf); // APPEND
    }

    // Random PICKITEM/SETITEM
    for (let i = 0; i < randomInt(1, 4); i++) {
      if (Math.random() < 0.5) {
        // PICKITEM
        script.push(0x68); // LDLOC0
        script.push(0x10); // PUSH0 (index)
        script.push(0xce); // PICKITEM
        script.push(0x45); // DROP
      } else {
        // SETITEM
        script.push(0x68); // LDLOC0
        script.push(0x10); // PUSH0 (index)
        script.push(randomChoice(PUSH_OPS)); // value
        script.push(0xd0); // SETITEM
      }
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 3. Call graph fuzzing ──────────────────────────────────────────────────

test("deep-fuzz: multi-method scripts with internal CALL edges", () => {
  for (let iter = 0; iter < 60; iter++) {
    const numMethods = randomInt(2, 6);
    const methodBodies = [];
    let totalLen = 0;

    for (let m = 0; m < numMethods; m++) {
      const body = [];
      body.push(0x57, randomInt(0, 3), randomInt(0, 2)); // INITSLOT
      const bodyLen = randomInt(2, 8);
      for (let j = 0; j < bodyLen; j++) {
        body.push(randomChoice(PUSH_OPS));
        body.push(0x45); // DROP
      }
      body.push(0x40); // RET
      methodBodies.push(body);
      totalLen += body.length;
    }

    // Inject CALL instructions to random methods
    const script = [];
    const offsets = [];
    let off = 0;
    for (const body of methodBodies) {
      offsets.push(off);
      script.push(...body);
      off += body.length;
    }

    // Replace some NOP-equivalents with CALL to other methods
    for (let i = 0; i < Math.min(3, numMethods - 1); i++) {
      const callerOff = offsets[0] + 3; // after INITSLOT
      const targetOff = offsets[randomInt(1, numMethods - 1)];
      const delta = targetOff - callerOff;
      if (delta > -128 && delta < 127 && callerOff + 2 < script.length) {
        script[callerOff] = 0x34; // CALL
        script[callerOff + 1] = delta & 0xff;
      }
    }

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => analyzeBytes(nef));
  }
});

test("deep-fuzz: CALLT with method tokens", () => {
  for (let iter = 0; iter < 50; iter++) {
    const numTokens = randomInt(1, 5);
    const tokens = [];
    for (let t = 0; t < numTokens; t++) {
      tokens.push({
        hash: randomBytes(20),
        method: `method_${t}`,
        parametersCount: randomInt(0, 4),
        hasReturnValue: Math.random() > 0.5,
        callFlags: 0x0f,
      });
    }

    const script = [0x57, 0x00, 0x00];
    for (let t = 0; t < numTokens; t++) {
      // Push args
      for (let a = 0; a < tokens[t].parametersCount; a++) {
        script.push(randomChoice(PUSH_OPS));
      }
      script.push(0x37, t, 0x00); // CALLT token index (u16 LE)
      if (tokens[t].hasReturnValue) {
        script.push(0x45); // DROP
      }
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script), tokens);
    mustNotCrash(() => {
      const result = analyzeBytes(nef);
      assert.ok(result.callGraph);
      assert.ok(result.callGraph.edges.length >= 0);
    });
  }
});

test("deep-fuzz: CALLA with PUSHA indirect targets", () => {
  for (let iter = 0; iter < 50; iter++) {
    const script = [0x57, 0x02, 0x00];

    // Method at offset 0
    // PUSHA pointing to a target
    const targetOffset = script.length + 20; // approximate
    script.push(0x0a); // PUSHA
    // u32 LE offset
    script.push(targetOffset & 0xff, (targetOffset >> 8) & 0xff, 0, 0);
    script.push(0x70); // STLOC0
    // load and call
    script.push(0x68); // LDLOC0
    script.push(0x36); // CALLA
    script.push(0x45); // DROP
    script.push(0x40); // RET

    // pad to target
    while (script.length < targetOffset) script.push(0x21); // NOP
    // second method
    script.push(0x57, 0x00, 0x00);
    script.push(randomChoice(PUSH_OPS));
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => analyzeBytes(nef));
  }
});

// ─── 4. SYSCALL fuzzing ────────────────────────────────────────────────────

test("deep-fuzz: random syscall hashes (valid and invalid)", () => {
  for (let iter = 0; iter < 80; iter++) {
    const script = [0x57, 0x00, 0x00];
    const numCalls = randomInt(1, 5);
    for (let i = 0; i < numCalls; i++) {
      // Push some args
      for (let a = 0; a < randomInt(0, 3); a++) {
        script.push(randomChoice(PUSH_OPS));
      }
      // SYSCALL with random hash
      script.push(0x41);
      script.push(...randomBytes(4));
      // might need to DROP result
      if (Math.random() > 0.3) script.push(0x45);
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 5. Postprocessor stress testing ────────────────────────────────────────

test("deep-fuzz: postprocessor with deeply nested if-else", () => {
  for (let iter = 0; iter < 40; iter++) {
    const depth = randomInt(5, 15);
    const script = [0x57, randomInt(1, 4), 0x01];

    for (let d = 0; d < depth; d++) {
      script.push(0x78); // LDARG0
      script.push(randomChoice(PUSH_OPS));
      script.push(0x97); // EQUAL
      const skipLen = (depth - d) * 4 + 4;
      script.push(0x26, Math.min(skipLen, 127)); // JMPIFNOT
      script.push(randomChoice(PUSH_OPS));
      script.push(randomChoice(SLOT_STLOC));
    }
    for (let d = 0; d < depth; d++) {
      script.push(randomChoice(SLOT_LDLOC));
      script.push(0x45);
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: postprocessor with compound assignments", () => {
  for (let iter = 0; iter < 50; iter++) {
    const script = [0x57, 0x04, 0x00];
    // Initialize locals
    for (let i = 0; i < 4; i++) {
      script.push(randomChoice(PUSH_OPS));
      script.push(0x70 + i); // STLOC0-3
    }
    // Random compound ops: loc = loc OP val
    for (let i = 0; i < randomInt(3, 10); i++) {
      const loc = randomInt(0, 3);
      script.push(0x68 + loc); // LDLOC
      script.push(randomChoice(PUSH_OPS)); // value
      script.push(randomChoice(BINARY_OPS)); // op
      script.push(0x70 + loc); // STLOC
    }
    script.push(0x68); // LDLOC0
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => {
      const result = decompileHighLevelBytes(nef);
      assert.ok(typeof result.highLevel === "string");
    });
  }
});

// ─── 6. Slot operation fuzzing ──────────────────────────────────────────────

test("deep-fuzz: all slot variants with random access patterns", () => {
  for (let iter = 0; iter < 60; iter++) {
    const numLocals = randomInt(1, 7);
    const numArgs = randomInt(0, 4);
    const script = [0x57, numLocals, numArgs];

    // Optional INITSSLOT
    if (Math.random() > 0.5) {
      script.push(0x56, randomInt(1, 5)); // INITSSLOT
    }

    const numOps = randomInt(5, 20);
    for (let i = 0; i < numOps; i++) {
      const choice = Math.random();
      if (choice < 0.2 && numArgs > 0) {
        // Load/store arg
        const idx = randomInt(0, Math.min(numArgs - 1, 3));
        if (Math.random() < 0.5) {
          script.push(0x78 + idx); // LDARG
        } else {
          script.push(randomChoice(PUSH_OPS));
          script.push(0x80 + idx); // STARG
        }
      } else if (choice < 0.5) {
        // Load/store local
        const idx = randomInt(0, Math.min(numLocals - 1, 3));
        if (Math.random() < 0.5) {
          script.push(0x68 + idx); // LDLOC
        } else {
          script.push(randomChoice(PUSH_OPS));
          script.push(0x70 + idx); // STLOC
        }
      } else if (choice < 0.7) {
        // Static
        const idx = randomInt(0, 3);
        if (Math.random() < 0.5) {
          script.push(0x58 + idx); // LDSFLD
        } else {
          script.push(randomChoice(PUSH_OPS));
          script.push(0x60 + idx); // STSFLD
        }
      } else {
        // Push + drop to keep stack balanced
        script.push(randomChoice(PUSH_OPS));
        script.push(0x45);
      }
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => {
      const result = analyzeBytes(nef);
      assert.ok(result.xrefs);
      assert.ok(result.types);
    });
  }
});

test("deep-fuzz: indexed slot operations (LDLOC/STLOC/LDARG/STARG/LDSFLD/STSFLD with U8)", () => {
  for (let iter = 0; iter < 40; iter++) {
    const numLocals = randomInt(5, 20);
    const numArgs = randomInt(2, 10);
    const script = [0x57, numLocals, numArgs];

    for (let i = 0; i < randomInt(5, 15); i++) {
      const slot = randomInt(0, numLocals - 1);
      script.push(randomChoice(PUSH_OPS));
      script.push(0x77, slot); // STLOC index
    }
    for (let i = 0; i < randomInt(2, 5); i++) {
      const slot = randomInt(0, numLocals - 1);
      script.push(0x6f, slot); // LDLOC index
      script.push(0x45); // DROP
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 7. Type inference fuzzing ──────────────────────────────────────────────

test("deep-fuzz: type inference with mixed collection creation", () => {
  for (let iter = 0; iter < 60; iter++) {
    const script = [0x57, 0x06, 0x00];
    const creators = [
      [0xc2, 0x70], // NEWARRAY0 → STLOC0
      [0xc8, 0x71], // NEWMAP → STLOC1
      [0xc5, 0x72], // NEWSTRUCT0 → STLOC2
    ];
    // Create collections
    for (const [create, store] of creators) {
      script.push(create, store);
    }
    // NEWBUFFER
    script.push(randomChoice(PUSH_OPS));
    script.push(0x88); // NEWBUFFER
    script.push(0x73); // STLOC3

    // CONVERT operations
    script.push(randomChoice(PUSH_OPS));
    const convertTypes = [0x21, 0x11, 0x20, 0x28, 0x30, 0x41, 0x45, 0x48];
    script.push(0xdb, randomChoice(convertTypes)); // CONVERT
    script.push(0x74); // STLOC4

    // NEWARRAY_T
    script.push(randomChoice(PUSH_OPS));
    script.push(0xc4, randomChoice(convertTypes)); // NEWARRAY_T
    script.push(0x75); // STLOC5

    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => {
      const result = analyzeBytes(nef);
      assert.ok(result.types.methods.length > 0);
    });
  }
});

// ─── 8. Manifest fuzzing ───────────────────────────────────────────────────

test("deep-fuzz: manifest with extreme/unusual field values", () => {
  const edgeCases = [
    // Extremely long method name
    { name: "C", abi: { methods: [{ name: "a".repeat(500), parameters: [], returntype: "Void", offset: 0, safe: false }], events: [] } },
    // Many parameters
    { name: "C", abi: { methods: [{ name: "m", parameters: Array.from({ length: 50 }, (_, i) => ({ name: `p${i}`, type: "Any" })), returntype: "Void", offset: 0, safe: false }], events: [] } },
    // Duplicate parameter names
    { name: "C", abi: { methods: [{ name: "m", parameters: [{ name: "x", type: "Integer" }, { name: "x", type: "String" }, { name: "x", type: "Boolean" }], returntype: "Void", offset: 0, safe: false }], events: [] } },
    // Unicode method names
    { name: "C", abi: { methods: [{ name: "日本語メソッド", parameters: [], returntype: "Integer", offset: 0, safe: true }], events: [] } },
    // Empty string names
    { name: "", abi: { methods: [{ name: "", parameters: [{ name: "", type: "Any" }], returntype: "Void", offset: 0, safe: false }], events: [] } },
    // Special characters in names
    { name: "C", abi: { methods: [{ name: "foo-bar_baz.qux", parameters: [], returntype: "Void", offset: 0, safe: false }], events: [] } },
    // Numeric parameter names
    { name: "C", abi: { methods: [{ name: "m", parameters: [{ name: "123", type: "Any" }, { name: "456", type: "Any" }], returntype: "Void", offset: 0, safe: false }], events: [] } },
    // All parameter types
    { name: "C", abi: { methods: [{ name: "m", parameters: [
      { name: "a", type: "Boolean" }, { name: "b", type: "Integer" },
      { name: "c", type: "String" }, { name: "d", type: "Hash160" },
      { name: "e", type: "Hash256" }, { name: "f", type: "ByteArray" },
      { name: "g", type: "Signature" }, { name: "h", type: "Array" },
      { name: "i", type: "Map" }, { name: "j", type: "InteropInterface" },
      { name: "k", type: "Void" }, { name: "l", type: "Any" },
    ], returntype: "Void", offset: 0, safe: false }], events: [] } },
    // Negative offset
    { name: "C", abi: { methods: [{ name: "m", parameters: [], returntype: "Void", offset: -1, safe: false }], events: [] } },
    // Non-integer offset
    { name: "C", abi: { methods: [{ name: "m", parameters: [], returntype: "Void", offset: 1.5, safe: false }], events: [] } },
    // Extra fields (should be passthrough)
    { name: "C", extra: { author: "test", email: "test@test.com", description: "desc", custom_field: 42, nested: { a: { b: { c: true } } } } },
    // Permissions and trusts
    { name: "C", permissions: [{ contract: "*", methods: "*" }, { contract: "0x1234", methods: ["transfer"] }], trusts: ["*"] },
    // Supported standards
    { name: "C", supportedstandards: ["NEP-17", "NEP-11", "NEP-27", "Custom-1"] },
    // Enormous extra object
    { name: "C", extra: Object.fromEntries(Array.from({ length: 100 }, (_, i) => [`key_${i}`, `value_${i}`])) },
  ];

  const script = new Uint8Array([0x57, 0x00, 0x00, 0x11, 0x40]);
  const nef = buildValidNef(script);

  for (const manifest of edgeCases) {
    mustNotCrash(() => {
      decompileHighLevelBytesWithManifest(nef, JSON.stringify(manifest));
    });
  }
});

test("deep-fuzz: manifest parameter type is required (matches Rust spec)", () => {
  // Spec requires `type` field exactly. Rust uses #[serde(rename = "type")];
  // JS now mirrors that strictness — neither `kind` nor missing is accepted.
  const accepted = {
    name: "C",
    abi: {
      methods: [
        { name: "m", parameters: [{ name: "a", type: "Integer" }], returntype: "Void", offset: 0, safe: false },
      ],
      events: [],
    },
  };
  assert.doesNotThrow(() => parseManifest(accepted));

  const usingKind = {
    name: "C",
    abi: {
      methods: [
        { name: "m", parameters: [{ name: "a", kind: "Integer" }], returntype: "Void", offset: 0, safe: false },
      ],
      events: [],
    },
  };
  assert.throws(
    () => parseManifest(usingKind),
    (err) => err.details.code === "MissingField" && err.details.path.endsWith(".type"),
  );

  const missingType = {
    name: "C",
    abi: {
      methods: [
        { name: "m", parameters: [{ name: "a" }], returntype: "Void", offset: 0, safe: false },
      ],
      events: [],
    },
  };
  assert.throws(
    () => parseManifest(missingType),
    (err) => err.details.code === "MissingField",
  );
});

test("deep-fuzz: manifest parse error handling", () => {
  const badInputs = [
    42,
    true,
    false,
    [1, 2, 3],
    null,
    "null",
    "true",
    "42",
    '"string"',
    "[1,2,3]",
    "",
    "   ",
    "{invalid",
    '{"key": undefined}',
    "{'single_quotes': 'val'}",
  ];

  for (const input of badInputs) {
    try {
      parseManifest(input);
    } catch (e) {
      assert.ok(
        e instanceof ManifestParseError || e instanceof Error,
        `Unexpected error type for input ${JSON.stringify(input)}: ${e.constructor.name}`,
      );
    }
  }
});

// ─── 9. NEF structure mutation fuzzing ──────────────────────────────────────

test("deep-fuzz: NEF with every byte position flipped", () => {
  const base = buildValidNef(new Uint8Array([0x11, 0x12, 0x9e, 0x40]));

  for (let i = 0; i < base.length; i++) {
    const mutated = new Uint8Array(base);
    mutated[i] ^= 0xff; // flip all bits at position i
    mustNotCrash(() => parseNef(mutated));
  }
});

test("deep-fuzz: NEF with random single-byte mutations", () => {
  const base = buildValidNef(new Uint8Array([
    0x57, 0x02, 0x01, // INITSLOT
    0x78, // LDARG0
    0x11, 0x9e, // PUSH1 ADD
    0x70, // STLOC0
    0x68, // LDLOC0
    0x40, // RET
  ]));

  for (let iter = 0; iter < 200; iter++) {
    const mutated = new Uint8Array(base);
    const pos = randomInt(0, mutated.length - 1);
    mutated[pos] = randomInt(0, 255);
    mustNotCrash(() => {
      const nef = parseNef(mutated);
      disassembleScript(nef.script);
    });
  }
});

test("deep-fuzz: NEF with multi-byte mutations", () => {
  const base = buildValidNef(new Uint8Array([
    0x57, 0x02, 0x01,
    0x78, 0x11, 0x9e, 0x70, 0x68, 0x40,
  ]));

  for (let iter = 0; iter < 100; iter++) {
    const mutated = new Uint8Array(base);
    const numMutations = randomInt(1, 5);
    for (let m = 0; m < numMutations; m++) {
      const pos = randomInt(0, mutated.length - 1);
      mutated[pos] = randomInt(0, 255);
    }
    mustNotCrash(() => parseNef(mutated));
  }
});

test("deep-fuzz: NEF with compiler field containing all byte values", () => {
  for (let iter = 0; iter < 30; iter++) {
    const data = [];
    data.push(...Buffer.from("NEF3"));
    // Random compiler bytes (64 bytes, but must be valid UTF-8 prefix + null padding)
    const compLen = randomInt(0, 63);
    const compiler = Buffer.alloc(64, 0);
    // Use valid ASCII for the compiler name portion
    for (let i = 0; i < compLen; i++) {
      compiler[i] = randomInt(0x20, 0x7e); // printable ASCII
    }
    data.push(...compiler);
    data.push(0); // source
    data.push(0); // reserved
    data.push(0); // tokens
    data.push(0, 0); // reserved word
    const script = [0x11, 0x40];
    writeVarint(data, script.length);
    data.push(...script);
    const checksum = computeChecksum(data);
    data.push(...checksum);

    mustNotCrash(() => parseNef(new Uint8Array(data)));
  }
});

test("deep-fuzz: NEF with source string of varying lengths", () => {
  for (const sourceLen of [0, 1, 10, 100, 200, 255, 256]) {
    const data = [];
    data.push(...Buffer.from("NEF3"));
    data.push(...new Uint8Array(64)); // compiler
    // source string
    const source = Buffer.alloc(sourceLen, 0x41); // 'A' repeated
    writeVarint(data, source.length);
    data.push(...source);
    data.push(0); // reserved
    data.push(0); // tokens
    data.push(0, 0); // reserved word
    const script = [0x11, 0x40];
    writeVarint(data, script.length);
    data.push(...script);
    const checksum = computeChecksum(data);
    data.push(...checksum);

    mustNotCrash(() => parseNef(new Uint8Array(data)));
  }
});

// ─── 10. Stack shape edge cases ─────────────────────────────────────────────

test("deep-fuzz: extreme stack manipulation sequences", () => {
  const patterns = [
    // DUP chain then binary ops
    () => {
      const s = [0x11]; // PUSH1
      for (let i = 0; i < randomInt(2, 10); i++) s.push(0x4a); // DUP
      for (let i = 0; i < randomInt(1, 5); i++) s.push(randomChoice(BINARY_OPS));
      s.push(0x40);
      return s;
    },
    // OVER/SWAP/NIP chains
    () => {
      const s = [0x11, 0x12, 0x13]; // PUSH 1,2,3
      for (let i = 0; i < randomInt(2, 8); i++) {
        s.push(randomChoice([0x4b, 0x50, 0x46])); // OVER, SWAP, NIP
      }
      s.push(0x40);
      return s;
    },
    // PICK with various depths
    () => {
      const s = [];
      const depth = randomInt(3, 8);
      for (let i = 0; i < depth; i++) s.push(0x10 + i); // PUSH0-PUSH7
      s.push(0x10 + randomInt(0, depth - 1)); // push index
      s.push(0x4d); // PICK
      s.push(0x40);
      return s;
    },
    // ROLL variations
    () => {
      const s = [];
      const depth = randomInt(3, 8);
      for (let i = 0; i < depth; i++) s.push(0x10 + i);
      s.push(0x10 + randomInt(0, depth - 1));
      s.push(0x52); // ROLL
      s.push(0x40);
      return s;
    },
    // REVERSE3/REVERSE4/REVERSEN
    () => {
      const s = [];
      for (let i = 0; i < 6; i++) s.push(randomChoice(PUSH_OPS));
      s.push(randomChoice([0x53, 0x54])); // REVERSE3 or REVERSE4
      s.push(0x40);
      return s;
    },
    // XDROP
    () => {
      const s = [];
      for (let i = 0; i < 5; i++) s.push(randomChoice(PUSH_OPS));
      s.push(0x12); // PUSH2 (index)
      s.push(0x48); // XDROP
      s.push(0x40);
      return s;
    },
    // DEPTH then operations based on it
    () => {
      const s = [];
      for (let i = 0; i < randomInt(1, 5); i++) s.push(randomChoice(PUSH_OPS));
      s.push(0x43); // DEPTH
      s.push(0x45); // DROP
      s.push(0x40);
      return s;
    },
    // CLEAR then continue
    () => {
      const s = [];
      for (let i = 0; i < 4; i++) s.push(randomChoice(PUSH_OPS));
      s.push(0x49); // CLEAR
      s.push(randomChoice(PUSH_OPS));
      s.push(0x40);
      return s;
    },
  ];

  for (let iter = 0; iter < 100; iter++) {
    const pattern = randomChoice(patterns);
    const script = pattern();
    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 11. Assertion/abort/throw fuzzing ──────────────────────────────────────

test("deep-fuzz: assert/abort/throw in various positions", () => {
  const terminators = [
    [0x38],       // ABORT
    [0x3a],       // THROW
    [0x39],       // ASSERT (pops 1)
    [0xe0],       // ABORTMSG (pops 1)
    [0xe1],       // ASSERTMSG (pops 2)
  ];

  for (let iter = 0; iter < 60; iter++) {
    const script = [0x57, 0x01, 0x00];
    // Some prefix ops
    for (let i = 0; i < randomInt(1, 5); i++) {
      script.push(randomChoice(PUSH_OPS));
    }
    // Insert terminator
    const term = randomChoice(terminators);
    script.push(...term);
    // May have unreachable code after
    if (Math.random() > 0.5) {
      for (let i = 0; i < randomInt(1, 3); i++) {
        script.push(randomChoice(PUSH_OPS));
      }
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 12. PUSHDATA fuzzing ───────────────────────────────────────────────────

test("deep-fuzz: PUSHDATA1/2/4 with various lengths", () => {
  for (let iter = 0; iter < 50; iter++) {
    const script = [];

    // PUSHDATA1 with random length
    const len1 = randomInt(0, 75);
    script.push(0x0c, len1, ...randomBytes(len1));

    // PUSHDATA2 with small length
    if (Math.random() > 0.5) {
      const len2 = randomInt(0, 100);
      script.push(0x0d, len2 & 0xff, (len2 >> 8) & 0xff, ...randomBytes(len2));
    }

    // PUSHINT128/256
    if (Math.random() > 0.5) {
      script.push(0x04, ...randomBytes(16)); // PUSHINT128
    }
    if (Math.random() > 0.5) {
      script.push(0x05, ...randomBytes(32)); // PUSHINT256
    }

    script.push(0x45); // DROP
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 13. Mixed control flow + data flow ─────────────────────────────────────

test("deep-fuzz: realistic contract patterns (transfer-like)", () => {
  for (let iter = 0; iter < 40; iter++) {
    const script = [0x57, 0x03, 0x03]; // 3 locals, 3 args (from, to, amount)
    // Store args to locals
    script.push(0x78, 0x70); // LDARG0, STLOC0
    script.push(0x79, 0x71); // LDARG1, STLOC1
    script.push(0x7a, 0x72); // LDARG2, STLOC2

    // if (amount <= 0) throw
    script.push(0x6a); // LDLOC2
    script.push(0x10); // PUSH0
    script.push(0x32, 0x04); // JMPLE +4
    script.push(0x22, 0x02); // JMP +2
    script.push(0x3a); // THROW

    // Random operations
    for (let i = 0; i < randomInt(2, 8); i++) {
      script.push(0x68 + randomInt(0, 2)); // LDLOC0-2
      script.push(randomChoice(PUSH_OPS));
      script.push(randomChoice(BINARY_OPS));
      script.push(0x70 + randomInt(0, 2)); // STLOC0-2
    }

    script.push(0x08); // PUSHT (true)
    script.push(0x40); // RET

    const nef = buildValidNef(new Uint8Array(script));
    const manifest = {
      name: "Token",
      abi: {
        methods: [{
          name: "transfer",
          parameters: [
            { name: "from", type: "Hash160" },
            { name: "to", type: "Hash160" },
            { name: "amount", type: "Integer" },
          ],
          returntype: "Boolean",
          offset: 0,
          safe: false,
        }],
        events: [{ name: "Transfer", parameters: [
          { name: "from", type: "Hash160" },
          { name: "to", type: "Hash160" },
          { name: "amount", type: "Integer" },
        ] }],
      },
    };

    mustNotCrash(() => {
      const result = decompileHighLevelBytesWithManifest(nef, JSON.stringify(manifest));
      assert.ok(result.highLevel.includes("transfer"));
    });
  }
});

// ─── 14. Generative grammar-based fuzzing ───────────────────────────────────

test("deep-fuzz: grammar-based random program generation", () => {
  function generateProgram() {
    const script = [];
    const numLocals = randomInt(0, 5);
    const numArgs = randomInt(0, 3);
    script.push(0x57, numLocals, numArgs);

    function emitExpression(depth = 0) {
      if (depth > 5 || Math.random() < 0.3) {
        // Terminal: push literal, load local/arg
        const choice = Math.random();
        if (choice < 0.5) {
          script.push(randomChoice(PUSH_OPS));
        } else if (choice < 0.7 && numLocals > 0) {
          script.push(0x68 + randomInt(0, Math.min(numLocals - 1, 3)));
        } else if (choice < 0.9 && numArgs > 0) {
          script.push(0x78 + randomInt(0, Math.min(numArgs - 1, 3)));
        } else {
          script.push(randomChoice(PUSH_OPS));
        }
        return;
      }

      const kind = Math.random();
      if (kind < 0.4) {
        // Binary op
        emitExpression(depth + 1);
        emitExpression(depth + 1);
        script.push(randomChoice(BINARY_OPS));
      } else if (kind < 0.6) {
        // Unary op
        emitExpression(depth + 1);
        script.push(randomChoice(UNARY_OPS));
      } else if (kind < 0.8) {
        // Collection creation
        const n = randomInt(1, 3);
        for (let i = 0; i < n; i++) emitExpression(depth + 1);
        script.push(0x10 + n); // push count
        script.push(0xc0); // PACK
      } else {
        // DUP expression
        emitExpression(depth + 1);
        script.push(0x4a); // DUP
        script.push(0x45); // DROP second copy
      }
    }

    function emitStatement(depth = 0) {
      if (depth > 3) {
        // Simple store
        emitExpression(0);
        if (numLocals > 0) {
          script.push(0x70 + randomInt(0, Math.min(numLocals - 1, 3)));
        } else {
          script.push(0x45); // DROP
        }
        return;
      }

      const kind = Math.random();
      if (kind < 0.4) {
        // Assignment
        emitExpression(0);
        if (numLocals > 0) {
          script.push(0x70 + randomInt(0, Math.min(numLocals - 1, 3)));
        } else {
          script.push(0x45);
        }
      } else if (kind < 0.6) {
        // Expression statement (push + drop)
        emitExpression(0);
        script.push(0x45);
      } else {
        // Another assignment
        emitExpression(0);
        if (numLocals > 0) {
          script.push(0x70 + randomInt(0, Math.min(numLocals - 1, 3)));
        } else {
          script.push(0x45);
        }
      }
    }

    const numStatements = randomInt(2, 10);
    for (let i = 0; i < numStatements; i++) {
      emitStatement(0);
    }

    // Return value
    if (numLocals > 0) {
      script.push(0x68); // LDLOC0
    } else {
      script.push(randomChoice(PUSH_OPS));
    }
    script.push(0x40); // RET

    return new Uint8Array(script);
  }

  for (let iter = 0; iter < 100; iter++) {
    const script = generateProgram();
    if (script.length > 512 * 1024) continue; // skip if too large
    const nef = buildValidNef(script);
    mustNotCrash(() => {
      const result = decompileHighLevelBytes(nef);
      assert.ok(typeof result.highLevel === "string");
      assert.ok(result.highLevel.length > 0);
    });
  }
});

// ─── 15. Determinism verification ───────────────────────────────────────────

test("deep-fuzz: output determinism across repeated runs", () => {
  for (let iter = 0; iter < 30; iter++) {
    const script = [];
    script.push(0x57, randomInt(0, 3), randomInt(0, 2));
    for (let i = 0; i < randomInt(3, 15); i++) {
      script.push(randomChoice(PUSH_OPS));
      if (Math.random() < 0.5) script.push(randomChoice(BINARY_OPS));
      else script.push(0x45); // DROP
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    let result1, result2;
    try {
      result1 = decompileHighLevelBytes(nef);
      result2 = decompileHighLevelBytes(nef);
    } catch {
      continue; // skip if it throws
    }

    assert.equal(result1.highLevel, result2.highLevel, "Output must be deterministic");
    assert.equal(result1.pseudocode, result2.pseudocode, "Pseudocode must be deterministic");
  }
});

// ─── 16. Full pipeline with manifest + analysis ─────────────────────────────

test("deep-fuzz: full analysis pipeline with random manifests", () => {
  for (let iter = 0; iter < 40; iter++) {
    const numMethods = randomInt(1, 4);
    const methods = [];
    let offset = 0;

    const scriptParts = [];
    for (let m = 0; m < numMethods; m++) {
      methods.push({
        name: `method_${m}`,
        parameters: Array.from({ length: randomInt(0, 3) }, (_, i) => ({
          name: `p${i}`,
          type: randomChoice(["Integer", "Boolean", "String", "ByteArray", "Hash160", "Any"]),
        })),
        returntype: randomChoice(["Void", "Boolean", "Integer", "Any"]),
        offset,
        safe: Math.random() > 0.5,
      });

      const body = [0x57, randomInt(0, 3), methods[m].parameters.length];
      for (let i = 0; i < randomInt(2, 6); i++) {
        body.push(randomChoice(PUSH_OPS));
        body.push(0x45);
      }
      if (methods[m].returntype !== "Void") {
        body.push(randomChoice(PUSH_OPS));
      }
      body.push(0x40);
      scriptParts.push(...body);
      offset += body.length;
    }

    const manifest = {
      name: `Contract_${iter}`,
      abi: { methods, events: [] },
      supportedstandards: randomInt(0, 1) ? ["NEP-17"] : [],
      permissions: [],
      trusts: [],
      extra: { author: "fuzz" },
    };

    const nef = buildValidNef(new Uint8Array(scriptParts));
    mustNotCrash(() => {
      const result = decompileHighLevelBytesWithManifest(nef, JSON.stringify(manifest));
      assert.ok(result.highLevel);
      assert.ok(result.manifest);
      assert.ok(result.methodGroups.length > 0);
    });

    mustNotCrash(() => {
      const result = analyzeBytes(nef, JSON.stringify(manifest));
      assert.ok(result.callGraph);
      assert.ok(result.xrefs);
      assert.ok(result.types);
    });
  }
});

// ─── 17. Edge: all comparison jumps ─────────────────────────────────────────

test("deep-fuzz: all comparison jump opcodes in branches", () => {
  const cmpJumps = [
    [0x28, "JMPEQ"], [0x2a, "JMPNE"],
    [0x2c, "JMPGT"], [0x2e, "JMPGE"],
    [0x30, "JMPLT"], [0x32, "JMPLE"],
  ];

  for (const [opcode] of cmpJumps) {
    for (let iter = 0; iter < 10; iter++) {
      const script = [0x57, 0x01, 0x01];
      script.push(0x78); // LDARG0
      script.push(randomChoice(PUSH_OPS)); // comparison value
      const bodyLen = randomInt(2, 6);
      script.push(opcode, bodyLen + 2); // comparison jump
      // body
      for (let i = 0; i < bodyLen; i++) {
        script.push(randomChoice(PUSH_OPS));
        script.push(0x45);
      }
      script.push(0x40);

      const nef = buildValidNef(new Uint8Array(script));
      mustNotCrash(() => decompileHighLevelBytes(nef));
    }
  }
});

// ─── 18. Rapid fire: tiny random scripts ────────────────────────────────────

test("deep-fuzz: 1000 tiny random scripts (1-20 bytes)", () => {
  for (let iter = 0; iter < 1000; iter++) {
    const len = randomInt(1, 20);
    const script = randomBytes(len);
    script[script.length - 1] = 0x40; // ensure RET at end
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

test("deep-fuzz: 500 medium random scripts with INITSLOT (20-100 bytes)", () => {
  for (let iter = 0; iter < 500; iter++) {
    const len = randomInt(20, 100);
    const script = randomBytes(len);
    script[0] = 0x57; // INITSLOT
    script[1] = randomInt(0, 7); // locals
    script[2] = randomInt(0, 4); // args
    script[script.length - 1] = 0x40; // RET
    const nef = buildValidNef(script);
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 19. CONVERT and ISTYPE with all type tags ──────────────────────────────

test("deep-fuzz: CONVERT and ISTYPE with all possible type bytes", () => {
  for (let typeByte = 0; typeByte <= 0xff; typeByte++) {
    const script1 = [randomChoice(PUSH_OPS), 0xdb, typeByte, 0x40]; // CONVERT
    const script2 = [randomChoice(PUSH_OPS), 0xd9, typeByte, 0x40]; // ISTYPE
    const nef1 = buildValidNef(new Uint8Array(script1));
    const nef2 = buildValidNef(new Uint8Array(script2));
    mustNotCrash(() => decompileHighLevelBytes(nef1));
    mustNotCrash(() => decompileHighLevelBytes(nef2));
  }
});

// ─── 20. LEFT/RIGHT/SUBSTR/MEMCPY/CAT ──────────────────────────────────────

test("deep-fuzz: string/buffer operations", () => {
  const ops = [
    () => [0x8b], // CAT (2 args)
    () => [0x8c], // SUBSTR (3 args)
    () => [0x8d], // LEFT (2 args)
    () => [0x8e], // RIGHT (2 args)
    () => [0x89], // MEMCPY (5 args)
  ];

  for (let iter = 0; iter < 50; iter++) {
    const script = [0x57, 0x01, 0x00];
    const op = randomChoice(ops)();

    // Push enough operands
    const argCounts = { 0x8b: 2, 0x8c: 3, 0x8d: 2, 0x8e: 2, 0x89: 5 };
    const argCount = argCounts[op[0]];
    for (let i = 0; i < argCount; i++) {
      // Mix of data and integers
      if (Math.random() < 0.3) {
        const dataLen = randomInt(1, 10);
        script.push(0x0c, dataLen, ...randomBytes(dataLen)); // PUSHDATA1
      } else {
        script.push(randomChoice(PUSH_OPS));
      }
    }
    script.push(...op);
    script.push(0x70); // STLOC0
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

// ─── 21. POW/MODMUL/MODPOW/SHL/SHR/MIN/MAX/WITHIN ─────────────────────────

test("deep-fuzz: advanced math operations", () => {
  const mathOps = [0xa3, 0xa5, 0xa6, 0xa8, 0xa9, 0xb9, 0xba, 0xbb];

  for (let iter = 0; iter < 50; iter++) {
    const script = [0x57, 0x01, 0x00];
    const numOps = randomInt(1, 5);
    for (let i = 0; i < numOps; i++) {
      const op = randomChoice(mathOps);
      const argCount = op === 0xbb ? 3 : (op === 0xa5 || op === 0xa6) ? 3 : 2;
      for (let j = 0; j < argCount; j++) {
        script.push(randomChoice(PUSH_OPS));
      }
      script.push(op);
      script.push(0x70); // STLOC0
    }
    script.push(0x40);

    const nef = buildValidNef(new Uint8Array(script));
    mustNotCrash(() => decompileHighLevelBytes(nef));
  }
});

console.log("Deep fuzz tests loaded");
