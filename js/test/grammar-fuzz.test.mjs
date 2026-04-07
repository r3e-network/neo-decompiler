/**
 * Grammar-based fuzz tests for Neo Decompiler JS
 *
 * Generates STRUCTURALLY VALID NeoVM programs using a seeded PRNG
 * and verifies that the decompiler handles them without crashing.
 *
 * Strategies:
 *  1. Stack-balanced arithmetic
 *  2. Slot-aware programs (INITSLOT + load/store)
 *  3. Forward-jump control flow
 *  4. Try/catch programs
 *  5. Collection programs
 *  6. Multi-method (CALL with correct offsets)
 *  7. Extreme valid (deep stacks, many locals, nested control flow)
 */

import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";

import {
  decompileBytes,
  decompileHighLevelBytes,
  analyzeBytes,
} from "../src/index.js";

// ---- NeoVM opcode bytes (from js/src/generated/opcodes.js) ----

const OP = {
  // Push value opcodes
  PUSHINT8:   0x00,
  PUSHINT16:  0x01,
  PUSHINT32:  0x02,
  PUSHINT64:  0x03,
  PUSHT:      0x08,
  PUSHF:      0x09,
  PUSHNULL:   0x0B,
  PUSHDATA1:  0x0C,
  PUSHM1:     0x0F,
  PUSH0:      0x10,
  PUSH1:      0x11,
  PUSH2:      0x12,
  PUSH3:      0x13,
  PUSH4:      0x14,
  PUSH5:      0x15,
  PUSH6:      0x16,
  PUSH7:      0x17,
  PUSH8:      0x18,
  PUSH9:      0x19,
  PUSH10:     0x1A,
  PUSH11:     0x1B,
  PUSH12:     0x1C,
  PUSH13:     0x1D,
  PUSH14:     0x1E,
  PUSH15:     0x1F,
  PUSH16:     0x20,

  // Flow control
  NOP:        0x21,
  JMP:        0x22,
  JMP_L:      0x23,
  JMPIF:      0x24,
  JMPIF_L:    0x25,
  JMPIFNOT:   0x26,
  JMPIFNOT_L: 0x27,
  JMPEQ:      0x28,
  JMPNE:      0x2A,
  JMPGT:      0x2C,
  JMPGE:      0x2E,
  JMPLT:      0x30,
  JMPLE:      0x32,
  CALL:       0x34,
  CALL_L:     0x35,
  ABORT:      0x38,
  ASSERT:     0x39,
  THROW:      0x3A,
  TRY:        0x3B,
  TRY_L:      0x3C,
  ENDTRY:     0x3D,
  ENDTRY_L:   0x3E,
  ENDFINALLY: 0x3F,
  RET:        0x40,

  // Stack ops
  DEPTH:      0x43,
  DROP:       0x45,
  NIP:        0x46,
  XDROP:      0x48,
  CLEAR:      0x49,
  DUP:        0x4A,
  OVER:       0x4B,
  PICK:       0x4D,
  TUCK:       0x4E,
  SWAP:       0x50,
  ROT:        0x51,
  REVERSE3:   0x53,
  REVERSE4:   0x54,

  // Slots
  INITSSLOT:  0x56,
  INITSLOT:   0x57,
  LDSFLD0:    0x58,
  LDSFLD1:    0x59,
  LDSFLD2:    0x5A,
  LDSFLD:     0x5F,
  STSFLD0:    0x60,
  STSFLD1:    0x61,
  STSFLD2:    0x62,
  STSFLD:     0x67,
  LDLOC0:     0x68,
  LDLOC1:     0x69,
  LDLOC2:     0x6A,
  LDLOC3:     0x6B,
  LDLOC4:     0x6C,
  LDLOC5:     0x6D,
  LDLOC6:     0x6E,
  LDLOC:      0x6F,
  STLOC0:     0x70,
  STLOC1:     0x71,
  STLOC2:     0x72,
  STLOC3:     0x73,
  STLOC4:     0x74,
  STLOC5:     0x75,
  STLOC6:     0x76,
  STLOC:      0x77,
  LDARG0:     0x78,
  LDARG1:     0x79,
  LDARG2:     0x7A,
  LDARG3:     0x7B,
  LDARG:      0x7F,
  STARG0:     0x80,
  STARG1:     0x81,
  STARG2:     0x82,
  STARG3:     0x83,
  STARG:      0x87,

  // Buffer / string
  NEWBUFFER:  0x88,
  CAT:        0x8B,
  SUBSTR:     0x8C,
  LEFT:       0x8D,
  RIGHT:      0x8E,

  // Bitwise
  INVERT:     0x90,
  AND:        0x91,
  OR:         0x92,
  XOR:        0x93,

  // Comparison
  EQUAL:      0x97,
  NOTEQUAL:   0x98,

  // Numeric unary
  SIGN:       0x99,
  ABS:        0x9A,
  NEGATE:     0x9B,
  INC:        0x9C,
  DEC:        0x9D,

  // Numeric binary
  ADD:        0x9E,
  SUB:        0x9F,
  MUL:        0xA0,
  DIV:        0xA1,
  MOD:        0xA2,
  POW:        0xA3,
  SQRT:       0xA4,
  SHL:        0xA8,
  SHR:        0xA9,

  // Boolean
  NOT:        0xAA,
  BOOLAND:    0xAB,
  BOOLOR:     0xAC,
  NZ:         0xB1,

  // Numeric comparison
  NUMEQUAL:     0xB3,
  NUMNOTEQUAL:  0xB4,
  LT:           0xB5,
  LE:           0xB6,
  GT:           0xB7,
  GE:           0xB8,
  MIN:          0xB9,
  MAX:          0xBA,
  WITHIN:       0xBB,

  // Collections
  PACK:         0xC0,
  UNPACK:       0xC1,
  NEWARRAY0:    0xC2,
  NEWARRAY:     0xC3,
  NEWARRAY_T:   0xC4,
  NEWSTRUCT0:   0xC5,
  NEWSTRUCT:    0xC6,
  NEWMAP:       0xC8,
  SIZE:         0xCA,
  HASKEY:       0xCB,
  KEYS:         0xCC,
  VALUES:       0xCD,
  PICKITEM:     0xCE,
  APPEND:       0xCF,
  SETITEM:      0xD0,
  REVERSEITEMS: 0xD1,
  REMOVE:       0xD2,
  CLEARITEMS:   0xD3,
  POPITEM:      0xD4,

  // Type
  ISNULL:     0xD8,
  ISTYPE:     0xD9,
  CONVERT:    0xDB,

  // Abort/assert with message
  ABORTMSG:   0xE0,
  ASSERTMSG:  0xE1,
};

// Convenient grouped lists
const PUSH_SMALL = [
  OP.PUSH0, OP.PUSH1, OP.PUSH2, OP.PUSH3, OP.PUSH4, OP.PUSH5,
  OP.PUSH6, OP.PUSH7, OP.PUSH8, OP.PUSH9, OP.PUSH10, OP.PUSH11,
  OP.PUSH12, OP.PUSH13, OP.PUSH14, OP.PUSH15, OP.PUSH16, OP.PUSHM1,
];

const BINARY_ARITH = [
  OP.ADD, OP.SUB, OP.MUL, OP.DIV, OP.MOD, OP.AND, OP.OR, OP.XOR,
  OP.SHL, OP.SHR, OP.NUMEQUAL, OP.NUMNOTEQUAL, OP.LT, OP.LE, OP.GT,
  OP.GE, OP.MIN, OP.MAX, OP.BOOLAND, OP.BOOLOR, OP.EQUAL, OP.NOTEQUAL,
];

const UNARY_ARITH = [
  OP.INC, OP.DEC, OP.NEGATE, OP.ABS, OP.SIGN, OP.NOT, OP.NZ,
  OP.INVERT, OP.SQRT,
];

// ---- Helpers ----

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

function buildNef(script) {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("grammar-fuzz"), 0);
  data.push(...compiler);

  // source (empty string)
  writeVarint(data, 0);

  data.push(0); // reserved byte

  // tokens (none)
  writeVarint(data, 0);

  data.push(0x00, 0x00); // reserved word

  const scriptBytes = Array.from(script);
  writeVarint(data, scriptBytes.length);
  data.push(...scriptBytes);

  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

/** Seeded PRNG (xorshift32) for reproducible fuzzing. */
function createRng(seed) {
  let state = seed | 0 || 1;
  return {
    next() {
      state ^= state << 13;
      state ^= state >> 17;
      state ^= state << 5;
      return (state >>> 0) / 0x100000000;
    },
    nextInt(max) {
      return Math.floor(this.next() * max);
    },
    nextByte() {
      return this.nextInt(256);
    },
    pick(arr) {
      return arr[this.nextInt(arr.length)];
    },
    nextBool() {
      return this.next() < 0.5;
    },
  };
}

/**
 * Run decompiler functions, catching controlled errors.
 * Returns true if all completed or threw only Error instances.
 * Fails the test on non-Error throws.
 */
function mustNotCrash(fn, label) {
  try {
    fn();
    return true;
  } catch (e) {
    assert.ok(
      e instanceof Error,
      `${label}: threw non-Error value: ${String(e)}`,
    );
    return false;
  }
}

function runAllDecompilers(nef, label) {
  mustNotCrash(() => decompileBytes(nef), `${label} decompileBytes`);
  mustNotCrash(() => decompileHighLevelBytes(nef), `${label} decompileHighLevelBytes`);
  mustNotCrash(() => analyzeBytes(nef), `${label} analyzeBytes`);
}

// Write a signed 8-bit offset as a single byte
function writeI8(value) {
  return [value & 0xff];
}

// Write a signed 32-bit offset as 4 little-endian bytes
function writeI32(value) {
  const buf = new ArrayBuffer(4);
  new DataView(buf).setInt32(0, value, true);
  return [...new Uint8Array(buf)];
}


// ---- ProgramGenerator ----

class ProgramGenerator {
  constructor(rng) {
    this.rng = rng;
    this.bytes = [];
    this.stackDepth = 0;
    this.instructionOffsets = [];
    this.localCount = 0;
    this.argCount = 0;
  }

  get offset() {
    return this.bytes.length;
  }

  recordOffset() {
    this.instructionOffsets.push(this.offset);
  }

  emit(...bytesArr) {
    this.bytes.push(...bytesArr);
  }

  // ---- Emitters with stack tracking ----

  emitPushSmall() {
    this.recordOffset();
    this.emit(this.rng.pick(PUSH_SMALL));
    this.stackDepth++;
  }

  emitPushInt8() {
    this.recordOffset();
    this.emit(OP.PUSHINT8, this.rng.nextByte());
    this.stackDepth++;
  }

  emitPushAny() {
    if (this.rng.nextBool()) {
      this.emitPushSmall();
    } else {
      this.emitPushInt8();
    }
  }

  emitBinaryOp() {
    if (this.stackDepth < 2) return false;
    this.recordOffset();
    this.emit(this.rng.pick(BINARY_ARITH));
    this.stackDepth--; // pops 2 pushes 1
    return true;
  }

  emitUnaryOp() {
    if (this.stackDepth < 1) return false;
    this.recordOffset();
    this.emit(this.rng.pick(UNARY_ARITH));
    // pops 1 pushes 1 -- net 0
    return true;
  }

  emitDup() {
    if (this.stackDepth < 1) return false;
    this.recordOffset();
    this.emit(OP.DUP);
    this.stackDepth++;
    return true;
  }

  emitSwap() {
    if (this.stackDepth < 2) return false;
    this.recordOffset();
    this.emit(OP.SWAP);
    return true;
  }

  emitOver() {
    if (this.stackDepth < 2) return false;
    this.recordOffset();
    this.emit(OP.OVER);
    this.stackDepth++;
    return true;
  }

  emitDrop() {
    if (this.stackDepth < 1) return false;
    this.recordOffset();
    this.emit(OP.DROP);
    this.stackDepth--;
    return true;
  }

  emitNop() {
    this.recordOffset();
    this.emit(OP.NOP);
  }

  emitRet() {
    this.recordOffset();
    this.emit(OP.RET);
  }

  emitClear() {
    this.recordOffset();
    this.emit(OP.CLEAR);
    this.stackDepth = 0;
  }

  emitInitSlot(locals, args) {
    this.recordOffset();
    this.emit(OP.INITSLOT, locals, args);
    this.localCount = locals;
    this.argCount = args;
  }

  emitStoreLocal(idx) {
    if (this.stackDepth < 1 || idx >= this.localCount) return false;
    this.recordOffset();
    if (idx <= 6) {
      this.emit(OP.STLOC0 + idx);
    } else {
      this.emit(OP.STLOC, idx);
    }
    this.stackDepth--;
    return true;
  }

  emitLoadLocal(idx) {
    if (idx >= this.localCount) return false;
    this.recordOffset();
    if (idx <= 6) {
      this.emit(OP.LDLOC0 + idx);
    } else {
      this.emit(OP.LDLOC, idx);
    }
    this.stackDepth++;
    return true;
  }

  emitStoreArg(idx) {
    if (this.stackDepth < 1 || idx >= this.argCount) return false;
    this.recordOffset();
    if (idx <= 3) {
      this.emit(OP.STARG0 + idx);
    } else {
      this.emit(OP.STARG, idx);
    }
    this.stackDepth--;
    return true;
  }

  emitLoadArg(idx) {
    if (idx >= this.argCount) return false;
    this.recordOffset();
    if (idx <= 3) {
      this.emit(OP.LDARG0 + idx);
    } else {
      this.emit(OP.LDARG, idx);
    }
    this.stackDepth++;
    return true;
  }

  // ---- Collection emitters ----

  emitNewArray0() {
    this.recordOffset();
    this.emit(OP.NEWARRAY0);
    this.stackDepth++;
  }

  emitNewMap() {
    this.recordOffset();
    this.emit(OP.NEWMAP);
    this.stackDepth++;
  }

  emitAppend() {
    // array + value -> void (pops 2 pushes 0)
    if (this.stackDepth < 2) return false;
    this.recordOffset();
    this.emit(OP.APPEND);
    this.stackDepth -= 2;
    return true;
  }

  emitSize() {
    // collection -> int (pops 1 pushes 1)
    if (this.stackDepth < 1) return false;
    this.recordOffset();
    this.emit(OP.SIZE);
    return true;
  }

  emitPack(count) {
    // pops count items + the count on stack, pushes array
    // We push count onto stack then PACK
    if (this.stackDepth < count) return false;
    this.recordOffset();
    // push the count first
    if (count <= 16) {
      this.emit(OP.PUSH0 + count);
    } else {
      this.emit(OP.PUSHINT8, count & 0xff);
    }
    this.stackDepth++;
    this.recordOffset();
    this.emit(OP.PACK);
    // PACK pops (count+1) pushes 1
    this.stackDepth -= count; // net: -(count+1)+1 = -count
    return true;
  }

  emitPickItem() {
    // collection + index -> value (pops 2 pushes 1)
    if (this.stackDepth < 2) return false;
    this.recordOffset();
    this.emit(OP.PICKITEM);
    this.stackDepth--;
    return true;
  }

  emitPopItem() {
    // array -> value (pops 1 pushes 1)
    if (this.stackDepth < 1) return false;
    this.recordOffset();
    this.emit(OP.POPITEM);
    return true;
  }

  // ---- Full strategy builders ----

  /** Strategy 1: stack-balanced arithmetic */
  buildArithmetic() {
    const numOps = 5 + this.rng.nextInt(20);

    for (let i = 0; i < numOps; i++) {
      const choice = this.rng.nextInt(10);
      if (choice < 3 || this.stackDepth < 2) {
        // Push a value
        this.emitPushAny();
      } else if (choice < 6) {
        this.emitBinaryOp();
      } else if (choice < 8) {
        this.emitUnaryOp();
      } else if (choice < 9) {
        this.emitDup();
      } else {
        this.emitSwap();
      }
    }

    // Drain stack
    while (this.stackDepth > 0) {
      if (this.stackDepth >= 2 && this.rng.nextBool()) {
        this.emitBinaryOp();
      } else {
        this.emitDrop();
      }
    }
    this.emitRet();
    return this.bytes;
  }

  /** Strategy 2: slot-aware programs */
  buildSlotAware() {
    const locals = 1 + this.rng.nextInt(7); // 1-7
    const args = this.rng.nextInt(5);        // 0-4
    this.emitInitSlot(locals, args);

    const numOps = 8 + this.rng.nextInt(20);
    for (let i = 0; i < numOps; i++) {
      const choice = this.rng.nextInt(12);
      if (choice < 3) {
        this.emitPushAny();
      } else if (choice < 5 && this.stackDepth >= 1) {
        const idx = this.rng.nextInt(locals);
        this.emitStoreLocal(idx);
      } else if (choice < 7) {
        const idx = this.rng.nextInt(locals);
        this.emitLoadLocal(idx);
      } else if (choice < 8 && args > 0) {
        const idx = this.rng.nextInt(args);
        this.emitLoadArg(idx);
      } else if (choice < 9 && args > 0 && this.stackDepth >= 1) {
        const idx = this.rng.nextInt(args);
        this.emitStoreArg(idx);
      } else if (choice < 10 && this.stackDepth >= 2) {
        this.emitBinaryOp();
      } else if (choice < 11 && this.stackDepth >= 1) {
        this.emitUnaryOp();
      } else {
        this.emitPushAny();
      }
    }

    // Drain stack
    while (this.stackDepth > 0) {
      this.emitDrop();
    }
    this.emitRet();
    return this.bytes;
  }

  /** Strategy 3: forward-jump control flow */
  buildForwardJumps() {
    // Generate basic blocks as arrays of bytes, then stitch with jumps
    const numBlocks = 2 + this.rng.nextInt(5);
    const blocks = [];

    for (let b = 0; b < numBlocks; b++) {
      const gen = new ProgramGenerator(this.rng);
      gen.stackDepth = 0; // each block starts fresh after a conditional
      const numInstr = 2 + this.rng.nextInt(5);
      for (let j = 0; j < numInstr; j++) {
        gen.emitPushAny();
        if (gen.stackDepth >= 2 && this.rng.nextBool()) {
          gen.emitBinaryOp();
        }
      }
      // Drop everything at end of block
      while (gen.stackDepth > 0) {
        gen.emitDrop();
      }
      blocks.push(gen.bytes);
    }

    // Now assemble: for each block except the last, optionally add a conditional
    // forward jump to the next block boundary.
    // Layout: [push condition] [JMPIF offset] [block_i bytes] ...
    const assembled = [];
    for (let b = 0; b < blocks.length; b++) {
      // Add a conditional jump that may skip this block
      if (b < blocks.length - 1 && this.rng.nextBool()) {
        assembled.push(OP.PUSH1); // push condition (true)
        // JMPIF with 8-bit offset: offset = block.length + 2 (jump over block bytes)
        // The jump offset is relative to the JMPIF instruction itself
        const blockLen = blocks[b].length;
        const jumpOffset = blockLen + 2; // +2 for the JMPIF + operand itself
        if (jumpOffset >= -128 && jumpOffset <= 127) {
          assembled.push(OP.JMPIF, jumpOffset & 0xff);
        } else {
          // Use JMPIF_L for larger offsets
          assembled.push(OP.JMPIF_L, ...writeI32(jumpOffset + 3)); // adjust for size diff
        }
      }
      assembled.push(...blocks[b]);
    }

    assembled.push(OP.RET);
    this.bytes = assembled;
    return this.bytes;
  }

  /** Strategy 4: try/catch programs */
  buildTryCatch() {
    // Structure:
    //   TRY catch_offset finally_offset
    //   <try body: push + ENDTRY end_offset>
    //   <catch body: ENDTRY end_offset>  (or 0 offset for no catch)
    //   <finally body: ENDFINALLY>  (or 0 offset for no finally)
    //   <end: RET>

    const hasCatch = this.rng.nextBool() || true; // always have at least one handler
    const hasFinally = this.rng.nextBool();

    // Build body sections first to calculate offsets
    const tryBody = [];
    const numTryOps = 1 + this.rng.nextInt(4);
    for (let i = 0; i < numTryOps; i++) {
      tryBody.push(this.rng.pick(PUSH_SMALL));
      tryBody.push(OP.DROP);
    }

    const catchBody = [];
    if (hasCatch) {
      const numCatchOps = 1 + this.rng.nextInt(3);
      for (let i = 0; i < numCatchOps; i++) {
        catchBody.push(this.rng.pick(PUSH_SMALL));
        catchBody.push(OP.DROP);
      }
    }

    const finallyBody = [];
    if (hasFinally) {
      const numFinallyOps = 1 + this.rng.nextInt(2);
      for (let i = 0; i < numFinallyOps; i++) {
        finallyBody.push(this.rng.pick(PUSH_SMALL));
        finallyBody.push(OP.DROP);
      }
      finallyBody.push(OP.ENDFINALLY);
    }

    // Calculate offsets relative to the TRY instruction (offset 0)
    // TRY itself is 3 bytes (opcode + 2 offset bytes)
    const tryStart = 3; // byte after TRY instruction
    const endtrySize = 2; // ENDTRY + 1 byte offset
    const tryEnd = tryStart + tryBody.length + endtrySize;

    const catchStart = tryEnd;
    const catchEnd = catchStart + catchBody.length + (hasCatch ? endtrySize : 0);

    const finallyStart = catchEnd;
    const finallyEnd = finallyStart + finallyBody.length;

    // endOffset = where RET is (the instruction after all handlers)
    const endOffset = finallyEnd;

    // catch_offset: relative to TRY instruction position
    const catchOffset = hasCatch ? catchStart : 0;
    const finallyOffset = hasFinally ? finallyStart : 0;

    // Assemble
    const result = [];
    // TRY instruction: opcode + catch_offset(i8) + finally_offset(i8)
    result.push(OP.TRY, catchOffset & 0xff, finallyOffset & 0xff);

    // Try body
    result.push(...tryBody);
    // ENDTRY jumps to end (relative to this ENDTRY instruction)
    const endtryToEnd = endOffset - (tryStart + tryBody.length);
    result.push(OP.ENDTRY, endtryToEnd & 0xff);

    // Catch body
    if (hasCatch) {
      result.push(...catchBody);
      const catchEndtryToEnd = endOffset - catchStart - catchBody.length;
      result.push(OP.ENDTRY, catchEndtryToEnd & 0xff);
    }

    // Finally body
    if (hasFinally) {
      result.push(...finallyBody);
    }

    // End
    result.push(OP.RET);

    this.bytes = result;
    return this.bytes;
  }

  /** Strategy 5: collection programs */
  buildCollections() {
    const strategy = this.rng.nextInt(4);

    if (strategy === 0) {
      // NEWARRAY0 + APPEND items + SIZE
      this.emitNewArray0();
      const count = 1 + this.rng.nextInt(5);
      for (let i = 0; i < count; i++) {
        this.emitDup(); // dup the array ref
        this.emitPushSmall(); // push value
        this.emitAppend(); // append (pops array+value)
      }
      // Array is still on stack
      this.emitSize();
      this.emitDrop();
      this.emitRet();
    } else if (strategy === 1) {
      // NEWMAP + operations
      this.emitNewMap();
      this.emitDup();
      this.emitPushSmall(); // key
      this.emitPushSmall(); // value
      // SETITEM: pops map, key, value
      if (this.stackDepth >= 3) {
        this.recordOffset();
        this.emit(OP.SETITEM);
        this.stackDepth -= 3;
      }
      // The original map reference was consumed, but we have the dup
      // Actually SETITEM modifies in place and pops 3, so we need the dup
      // We already duped, so after SETITEM the dup is gone too
      // Let's just SIZE+DROP what we have
      while (this.stackDepth > 0) {
        this.emitDrop();
      }
      this.emitRet();
    } else if (strategy === 2) {
      // PACK: push N items, push count, PACK
      const count = 1 + this.rng.nextInt(5);
      for (let i = 0; i < count; i++) {
        this.emitPushSmall();
      }
      this.emitPack(count);
      // Now we have an array on stack
      this.emitDup();
      this.emitSize();
      this.emitDrop(); // drop size
      this.emitDrop(); // drop array
      this.emitRet();
    } else {
      // NEWARRAY0 + POPITEM after appending
      this.emitNewArray0();
      this.emitDup();
      this.emitPushSmall();
      this.emitAppend();
      // array is on stack
      this.emitPopItem(); // pops array, pushes last item
      this.emitDrop();
      this.emitRet();
    }
    return this.bytes;
  }

  /** Strategy 6: multi-method (separated by RET, linked by CALL) */
  buildMultiMethod() {
    const numMethods = 2 + this.rng.nextInt(4); // 2-5
    const methodBodies = [];

    // Generate method bodies first (each is simple push+arithmetic+RET)
    for (let m = 0; m < numMethods; m++) {
      const body = [];
      const numInstr = 2 + this.rng.nextInt(5);
      for (let i = 0; i < numInstr; i++) {
        body.push(this.rng.pick(PUSH_SMALL));
      }
      // drop all but one (return value)
      for (let i = 0; i < numInstr - 1; i++) {
        body.push(OP.DROP);
      }
      body.push(OP.RET);
      methodBodies.push(body);
    }

    // The entry method calls other methods via CALL with proper offsets.
    // CALL uses a signed i8 offset relative to the CALL instruction itself.
    // Layout: [entry: CALLs + cleanup + RET] [method1] [method2] ...

    // Build entry method
    const entry = [];
    // First figure out entry method size so we can compute offsets
    // Entry will be: for each called method: CALL(i8) + DROP, then RET
    // Each CALL is 2 bytes, each DROP is 1 byte
    const callsPerMethod = 3; // CALL(2) + DROP(1)
    const entrySize = (numMethods - 1) * callsPerMethod + 1; // +1 for final RET

    // Calculate method start offsets relative to start of entire program
    const methodStarts = [];
    let pos = entrySize;
    for (let m = 0; m < numMethods; m++) {
      methodStarts.push(pos);
      pos += methodBodies[m].length;
    }

    // Emit CALL instructions to each method (except entry itself)
    for (let m = 0; m < numMethods - 1; m++) {
      const callOffset = entry.length; // offset within entry where CALL sits
      const targetOffset = methodStarts[m]; // absolute offset of target method
      // CALL offset is relative to the CALL instruction position
      const relOffset = targetOffset - callOffset;
      if (relOffset >= -128 && relOffset <= 127) {
        entry.push(OP.CALL, relOffset & 0xff);
      } else {
        entry.push(OP.CALL_L, ...writeI32(relOffset));
      }
      entry.push(OP.DROP); // drop return value
    }
    entry.push(OP.RET);

    // Assemble
    const result = [...entry];
    for (const body of methodBodies) {
      result.push(...body);
    }
    this.bytes = result;
    return this.bytes;
  }

  /** Strategy 7: extreme valid (deep stacks, many locals) */
  buildExtreme() {
    const variant = this.rng.nextInt(3);

    if (variant === 0) {
      // Deep stack: push 500+ values then drain
      const depth = 500 + this.rng.nextInt(200);
      for (let i = 0; i < depth; i++) {
        this.emitPushSmall();
      }
      // Drain with mix of binary ops and drops
      while (this.stackDepth > 0) {
        if (this.stackDepth >= 2 && this.rng.nextInt(3) < 2) {
          this.emitBinaryOp();
        } else {
          this.emitDrop();
        }
      }
      this.emitRet();
    } else if (variant === 1) {
      // Many locals (up to 127, INITSLOT operand is a byte pair)
      const locals = 100 + this.rng.nextInt(28); // 100-127
      this.emitInitSlot(locals, 0);

      // Initialize all locals
      for (let i = 0; i < locals; i++) {
        this.emitPushSmall();
        this.emitStoreLocal(i);
      }

      // Load a bunch back and do arithmetic
      const numOps = 50 + this.rng.nextInt(50);
      for (let i = 0; i < numOps; i++) {
        const idx = this.rng.nextInt(locals);
        this.emitLoadLocal(idx);
        if (this.stackDepth >= 2 && this.rng.nextBool()) {
          this.emitBinaryOp();
        }
      }

      // Drain
      while (this.stackDepth > 0) {
        this.emitDrop();
      }
      this.emitRet();
    } else {
      // Nested forward jumps (multiple layers of JMPIF)
      const layers = 5 + this.rng.nextInt(5);
      // Build from inside out: innermost block first
      let inner = [this.rng.pick(PUSH_SMALL), OP.DROP];

      for (let l = 0; l < layers; l++) {
        const wrapped = [];
        wrapped.push(OP.PUSH1); // condition
        // JMPIFNOT: skip this block if false
        const blockLen = inner.length;
        const jumpOffset = blockLen + 2; // +2 for JMPIFNOT + operand
        wrapped.push(OP.JMPIFNOT, jumpOffset & 0xff);
        wrapped.push(...inner);
        inner = wrapped;
      }

      inner.push(OP.RET);
      this.bytes = inner;
      this.emitRet();
    }
    return this.bytes;
  }
}


// ---- Tests ----

test("Grammar Fuzz: stack-balanced arithmetic (100 iterations)", async (t) => {
  for (let i = 0; i < 100; i++) {
    await t.test(`arithmetic seed=${i}`, () => {
      const rng = createRng(1000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildArithmetic();
      const nef = buildNef(script);
      runAllDecompilers(nef, `arith[${i}]`);
    });
  }
});

test("Grammar Fuzz: slot-aware programs (100 iterations)", async (t) => {
  for (let i = 0; i < 100; i++) {
    await t.test(`slot seed=${i}`, () => {
      const rng = createRng(2000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildSlotAware();
      const nef = buildNef(script);
      runAllDecompilers(nef, `slot[${i}]`);
    });
  }
});

test("Grammar Fuzz: forward-jump control flow (100 iterations)", async (t) => {
  for (let i = 0; i < 100; i++) {
    await t.test(`jump seed=${i}`, () => {
      const rng = createRng(3000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildForwardJumps();
      const nef = buildNef(script);
      runAllDecompilers(nef, `jump[${i}]`);
    });
  }
});

test("Grammar Fuzz: try/catch programs (50 iterations)", async (t) => {
  for (let i = 0; i < 50; i++) {
    await t.test(`trycatch seed=${i}`, () => {
      const rng = createRng(4000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildTryCatch();
      const nef = buildNef(script);
      runAllDecompilers(nef, `trycatch[${i}]`);
    });
  }
});

test("Grammar Fuzz: collection programs (100 iterations)", async (t) => {
  for (let i = 0; i < 100; i++) {
    await t.test(`collection seed=${i}`, () => {
      const rng = createRng(5000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildCollections();
      const nef = buildNef(script);
      runAllDecompilers(nef, `collection[${i}]`);
    });
  }
});

test("Grammar Fuzz: multi-method programs (50 iterations)", async (t) => {
  for (let i = 0; i < 50; i++) {
    await t.test(`multi-method seed=${i}`, () => {
      const rng = createRng(6000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildMultiMethod();
      const nef = buildNef(script);
      runAllDecompilers(nef, `multi[${i}]`);
    });
  }
});

test("Grammar Fuzz: extreme valid programs (50 iterations)", async (t) => {
  for (let i = 0; i < 50; i++) {
    await t.test(`extreme seed=${i}`, () => {
      const rng = createRng(7000 + i);
      const gen = new ProgramGenerator(rng);
      const script = gen.buildExtreme();
      const nef = buildNef(script);
      runAllDecompilers(nef, `extreme[${i}]`);
    });
  }
});
