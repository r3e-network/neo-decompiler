/**
 * Differential fuzz tester: generates random valid NEF files and compares
 * the Rust CLI output against the JS API output.
 *
 * Usage:  node js/test/differential-fuzz.mjs
 */

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { decompileBytes, decompileHighLevelBytes } from "../src/index.js";
import { OPCODES } from "../src/generated/opcodes.js";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const NUM_CASES = 200;
const ROOT = join(import.meta.dirname, "..", "..");
const RUST_BIN = join(ROOT, "target", "release", "neo-decompiler");
const TEMP_DIR = mkdtempSync(join(tmpdir(), "neo-diff-fuzz-"));

// ---------------------------------------------------------------------------
// Opcode tables
// ---------------------------------------------------------------------------

/** Opcode tables grouped by operand encoding kind. */
const NONE_OPCODES = [];
const U8_OPCODES = [];
const I8_OPCODES = [];
const I16_OPCODES = [];
const U16_OPCODES = [];
const I32_OPCODES = [];
const U32_OPCODES = [];
const I64_OPCODES = [];
const JUMP8_OPCODES = [];
const JUMP32_OPCODES = [];
const SYSCALL_OPCODES = [];
const DATA1_OPCODES = [];
const DATA2_OPCODES = [];
const DATA4_OPCODES = [];
/** Bytes-encoded opcodes carry a fixed-length payload, e.g. PUSHINT128 (16). */
const BYTES_OPCODES = []; // each entry: { byte, length }

for (const [byte, op] of OPCODES) {
  switch (op.operandEncoding.kind) {
    case "None":     NONE_OPCODES.push(byte); break;
    case "U8":       U8_OPCODES.push(byte); break;
    case "I8":       I8_OPCODES.push(byte); break;
    case "I16":      I16_OPCODES.push(byte); break;
    case "U16":      U16_OPCODES.push(byte); break;
    case "I32":      I32_OPCODES.push(byte); break;
    case "U32":      U32_OPCODES.push(byte); break;
    case "I64":      I64_OPCODES.push(byte); break;
    case "Jump8":    JUMP8_OPCODES.push(byte); break;
    case "Jump32":   JUMP32_OPCODES.push(byte); break;
    case "Syscall":  SYSCALL_OPCODES.push(byte); break;
    case "Data1":    DATA1_OPCODES.push(byte); break;
    case "Data2":    DATA2_OPCODES.push(byte); break;
    case "Data4":    DATA4_OPCODES.push(byte); break;
    case "Bytes":    BYTES_OPCODES.push({ byte, length: op.operandEncoding.length }); break;
  }
}

// Filter to opcodes that are "safe" for standalone generation.
// INITSLOT (0x57) needs a Bytes operand of exactly 2, handle it specially.
const INITSLOT_BYTE = 0x57;
// RET (0x40)
const RET_BYTE = 0x40;
// NOP (0x21)
const NOP_BYTE = 0x21;

// ---------------------------------------------------------------------------
// NEF builder
// ---------------------------------------------------------------------------

function computeChecksum(data) {
  const first = createHash("sha256").update(Buffer.from(data)).digest();
  const second = createHash("sha256").update(first).digest();
  return new Uint8Array(second.subarray(0, 4));
}

function writeVarint(buf, value) {
  if (value <= 0xfc) {
    buf.push(value);
  } else if (value <= 0xffff) {
    buf.push(0xfd, value & 0xff, value >> 8);
  } else {
    buf.push(
      0xfe,
      value & 0xff,
      (value >> 8) & 0xff,
      (value >> 16) & 0xff,
      (value >> 24) & 0xff,
    );
  }
}

function buildNefFromScript(scriptBytes) {
  const script = Array.from(scriptBytes);
  const data = [];
  // Magic
  data.push(...Buffer.from("NEF3"));
  // Compiler (64 bytes, zero-padded)
  const compiler = new Uint8Array(64);
  compiler.set(Buffer.from("fuzz-test"), 0);
  data.push(...compiler);
  // Source (empty var-string)
  data.push(0);
  // Reserved byte
  data.push(0);
  // Method tokens (count=0)
  data.push(0);
  // Reserved word
  data.push(0x00, 0x00);
  // Script
  writeVarint(data, script.length);
  data.push(...script);
  // Checksum
  const checksum = computeChecksum(data);
  data.push(...checksum);
  return new Uint8Array(data);
}

// ---------------------------------------------------------------------------
// Random script generation
// ---------------------------------------------------------------------------

function randInt(min, max) {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function pickRandom(arr) {
  return arr[Math.floor(Math.random() * arr.length)];
}

function pushRandomBytes(bytes, count) {
  for (let i = 0; i < count; i++) bytes.push(randInt(0, 255));
}

/**
 * Build the list of emit-strategies that fit within `remaining` bytes.
 * Each strategy returns the number of bytes it consumed.
 */
function eligibleStrategies(remaining) {
  const strategies = [];
  // Single-byte ops are always eligible.
  strategies.push(() => {
    bytes.push(pickRandom(NONE_OPCODES));
    return 1;
  });
  if (remaining >= 2) {
    strategies.push(() => { bytes.push(pickRandom(U8_OPCODES)); bytes.push(randInt(0, 255)); return 2; });
    strategies.push(() => { bytes.push(pickRandom(I8_OPCODES)); bytes.push(randInt(0, 255)); return 2; });
    // Jump8: emit a small forward offset so we land on a valid boundary if possible.
    strategies.push(() => { bytes.push(pickRandom(JUMP8_OPCODES)); bytes.push(2); return 2; });
  }
  if (remaining >= 3) {
    strategies.push(() => { bytes.push(pickRandom(I16_OPCODES)); pushRandomBytes(bytes, 2); return 3; });
    // U16 (CALLT) — emit a token index of 0; without method tokens it'll fail
    // decompile in both impls (which still counts as "agree").
    if (U16_OPCODES.length > 0) {
      strategies.push(() => { bytes.push(pickRandom(U16_OPCODES)); bytes.push(0, 0); return 3; });
    }
    // Bytes:2 (TRY, INITSLOT)
    strategies.push(() => {
      const op = BYTES_OPCODES.find((o) => o.length === 2);
      if (!op) return 0;
      bytes.push(op.byte);
      pushRandomBytes(bytes, 2);
      return 3;
    });
  }
  if (remaining >= 5) {
    strategies.push(() => { bytes.push(pickRandom(I32_OPCODES)); pushRandomBytes(bytes, 4); return 5; });
    strategies.push(() => { bytes.push(pickRandom(U32_OPCODES)); pushRandomBytes(bytes, 4); return 5; });
    // Jump32: forward jump of 5 (lands on next instruction if no operand follows).
    strategies.push(() => { bytes.push(pickRandom(JUMP32_OPCODES)); bytes.push(5, 0, 0, 0); return 5; });
    if (SYSCALL_OPCODES.length > 0) {
      strategies.push(() => { bytes.push(pickRandom(SYSCALL_OPCODES)); pushRandomBytes(bytes, 4); return 5; });
    }
    // Data1 with small payload (length 0–3)
    strategies.push(() => {
      const len = randInt(0, Math.min(3, remaining - 2));
      bytes.push(pickRandom(DATA1_OPCODES));
      bytes.push(len);
      pushRandomBytes(bytes, len);
      return 2 + len;
    });
  }
  if (remaining >= 4) {
    // Data2 with payload length 0–1
    strategies.push(() => {
      const len = randInt(0, Math.min(1, remaining - 3));
      bytes.push(pickRandom(DATA2_OPCODES));
      bytes.push(len & 0xff, (len >> 8) & 0xff);
      pushRandomBytes(bytes, len);
      return 3 + len;
    });
  }
  if (remaining >= 6) {
    // Data4 with payload length 0–1
    strategies.push(() => {
      const len = randInt(0, Math.min(1, remaining - 5));
      bytes.push(pickRandom(DATA4_OPCODES));
      bytes.push(len & 0xff, (len >> 8) & 0xff, (len >> 16) & 0xff, (len >> 24) & 0xff);
      pushRandomBytes(bytes, len);
      return 5 + len;
    });
  }
  if (remaining >= 9) {
    strategies.push(() => { bytes.push(pickRandom(I64_OPCODES)); pushRandomBytes(bytes, 8); return 9; });
    // Bytes:8 (TRY_L)
    strategies.push(() => {
      const op = BYTES_OPCODES.find((o) => o.length === 8);
      if (!op) return 0;
      bytes.push(op.byte);
      pushRandomBytes(bytes, 8);
      return 9;
    });
  }
  if (remaining >= 17) {
    // Bytes:16 (PUSHINT128)
    strategies.push(() => {
      const op = BYTES_OPCODES.find((o) => o.length === 16);
      if (!op) return 0;
      bytes.push(op.byte);
      pushRandomBytes(bytes, 16);
      return 17;
    });
  }
  if (remaining >= 33) {
    // Bytes:32 (PUSHINT256)
    strategies.push(() => {
      const op = BYTES_OPCODES.find((o) => o.length === 32);
      if (!op) return 0;
      bytes.push(op.byte);
      pushRandomBytes(bytes, 32);
      return 33;
    });
  }
  return strategies;
}

// Closure-captured `bytes` buffer used by emit strategies.
let bytes = [];

/**
 * Generate a random script of the specified byte-length target.
 * Mixes opcodes from every operand encoding kind (None, U8, I8, I16, U16,
 * I32, U32, I64, Jump8, Jump32, Syscall, Bytes, Data1, Data2, Data4) so
 * differential coverage isn't restricted to single-byte operands.
 * Always ends with RET.
 */
function generateRandomScript(targetLen) {
  if (targetLen < 1) targetLen = 1;
  bytes = [];

  if (targetLen >= 4 && Math.random() < 0.3) {
    bytes.push(INITSLOT_BYTE);
    bytes.push(randInt(0, 4));
    bytes.push(randInt(0, 3));
  }

  while (bytes.length < targetLen - 1) {
    const remaining = targetLen - 1 - bytes.length;
    const strategies = eligibleStrategies(remaining);
    const consumed = pickRandom(strategies)();
    if (consumed === 0) {
      // Strategy declined (e.g. no Bytes:N opcode of needed size); fall back.
      bytes.push(pickRandom(NONE_OPCODES));
    }
  }

  bytes.push(RET_BYTE);
  return new Uint8Array(bytes);
}

// ---------------------------------------------------------------------------
// Rust CLI runner
// ---------------------------------------------------------------------------

function runRustDisasm(nefPath) {
  try {
    const stdout = execFileSync(RUST_BIN, ["disasm", nefPath], {
      encoding: "utf-8",
      timeout: 10_000,
      stdio: ["pipe", "pipe", "pipe"],
    });
    return { ok: true, output: stdout };
  } catch (err) {
    return { ok: false, output: (err.stdout ?? "") + (err.stderr ?? ""), status: err.status };
  }
}

function runRustDecompile(nefPath) {
  try {
    const stdout = execFileSync(RUST_BIN, ["decompile", nefPath], {
      encoding: "utf-8",
      timeout: 10_000,
      stdio: ["pipe", "pipe", "pipe"],
    });
    return { ok: true, output: stdout };
  } catch (err) {
    return { ok: false, output: (err.stdout ?? "") + (err.stderr ?? ""), status: err.status };
  }
}

// ---------------------------------------------------------------------------
// Hand-crafted edge cases (targeted blind-spot probes)
// ---------------------------------------------------------------------------

/**
 * Each entry is { name, script: number[] }. These deliberately probe
 * boundary cases that random generation hits with low probability.
 */
function buildEdgeCases() {
  const cases = [];

  // PUSHDATA1 with empty payload
  cases.push({ name: "PUSHDATA1_empty", script: [0x0c, 0x00, 0x40] });
  // PUSHDATA1 with single byte
  cases.push({ name: "PUSHDATA1_single", script: [0x0c, 0x01, 0xab, 0x40] });
  // PUSHDATA1 max length (255)
  cases.push({
    name: "PUSHDATA1_max",
    script: [0x0c, 0xff, ...new Array(255).fill(0xcc), 0x40],
  });
  // PUSHDATA2 with empty payload
  cases.push({ name: "PUSHDATA2_empty", script: [0x0d, 0x00, 0x00, 0x40] });
  // PUSHDATA4 with empty payload
  cases.push({ name: "PUSHDATA4_empty", script: [0x0e, 0x00, 0x00, 0x00, 0x00, 0x40] });

  // PUSHINT128 — 16-byte fixed payload (boundary check)
  cases.push({
    name: "PUSHINT128",
    script: [0x04, ...new Array(16).fill(0xff), 0x40],
  });
  // PUSHINT256 — 32-byte fixed payload
  cases.push({
    name: "PUSHINT256",
    script: [0x05, ...new Array(32).fill(0x55), 0x40],
  });

  // PUSHINT8/16/32/64 with sign-bit-set values
  cases.push({ name: "PUSHINT8_neg", script: [0x00, 0xff, 0x40] });
  cases.push({ name: "PUSHINT16_neg", script: [0x01, 0xff, 0xff, 0x40] });
  cases.push({ name: "PUSHINT32_neg", script: [0x02, 0xff, 0xff, 0xff, 0xff, 0x40] });
  cases.push({
    name: "PUSHINT64_neg",
    script: [0x03, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x40],
  });
  // PUSHINT64 with i64::MIN
  cases.push({
    name: "PUSHINT64_min",
    script: [0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x40],
  });

  // TRY with zero offsets (no catch, no finally) — bare try
  cases.push({ name: "TRY_zero_offsets", script: [0x3b, 0x00, 0x00, 0x40] });
  // TRY with explicit forward catch+finally to RET
  cases.push({
    name: "TRY_forward_catch_finally",
    script: [
      0x3b, 0x05, 0x07, // TRY catch=+5, finally=+7 (relative to TRY offset 0)
      0x21,             // NOP (try body, offset 3)
      0x3d, 0x00,       // ENDTRY +0 (offset 4)
      0x21,             // NOP (catch body, offset 6 — not at +5 due to ENDTRY)
      0x3f,             // ENDFINALLY (offset 7)
      0x40,             // RET
    ],
  });

  // CALLT with token index 0 (no tokens declared, will fail decompile in both)
  cases.push({ name: "CALLT_index0", script: [0x37, 0x00, 0x00, 0x40] });

  // PUSHA pointer to next instruction
  cases.push({ name: "PUSHA_next", script: [0x0a, 0x05, 0x00, 0x00, 0x00, 0x40] });

  // Jump32 forward to RET
  cases.push({
    name: "JMP_L_to_ret",
    script: [0x23, 0x05, 0x00, 0x00, 0x00, 0x40],
  });

  // Deep nesting: chain of JMPIFNOT each skipping one NOP
  const deep = [0x57, 0x00, 0x00];
  for (let i = 0; i < 80; i++) deep.push(0x11, 0x26, 0x02, 0x21);
  deep.push(0x40);
  cases.push({ name: "deep_nesting_80", script: deep });

  // INITSLOT with max counts (255 locals, 255 args)
  cases.push({
    name: "INITSLOT_max",
    script: [0x57, 0xff, 0xff, 0x40],
  });

  // Unreachable code after RET
  cases.push({
    name: "unreachable_after_ret",
    script: [0x40, 0x11, 0x12, 0x40],
  });

  // SYSCALL with a known hash (System.Runtime.Log = 0xcfe74796)
  cases.push({
    name: "SYSCALL_System_Runtime_Log",
    script: [0x0c, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x41, 0x96, 0x47, 0xe7, 0xcf, 0x40],
  });

  return cases;
}

const edgeCases = buildEdgeCases();
const edgeStats = { total: 0, agree: 0, disagree: 0 };
const edgeMismatches = [];

console.log(`Edge-case probes: ${edgeCases.length} hand-crafted scripts`);
for (const ec of edgeCases) {
  edgeStats.total++;
  const nef = buildNefFromScript(new Uint8Array(ec.script));
  const path = join(TEMP_DIR, `edge_${ec.name}.nef`);
  writeFileSync(path, nef);

  const rustOk = runRustDisasm(path).ok;
  let jsOk = false;
  let jsCount = -1;
  try {
    const r = decompileBytes(nef);
    jsOk = true;
    jsCount = r.instructions.length;
  } catch {}

  // Disasm parity: both succeed or both fail
  if (rustOk === jsOk) {
    edgeStats.agree++;
  } else {
    edgeStats.disagree++;
    edgeMismatches.push({ name: ec.name, rustOk, jsOk, jsCount });
  }
}
console.log(
  `  Edge-case agreement: ${edgeStats.agree}/${edgeStats.total}` +
    (edgeStats.disagree > 0 ? ` (${edgeStats.disagree} mismatch)` : ""),
);
if (edgeMismatches.length > 0) {
  for (const m of edgeMismatches) {
    console.log(`    MISMATCH: ${m.name}  rustOk=${m.rustOk} jsOk=${m.jsOk}`);
  }
}
console.log();

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

console.log(`Differential fuzz: generating ${NUM_CASES} random NEF files in ${TEMP_DIR}`);
console.log(`Rust binary: ${RUST_BIN}`);
console.log();

const stats = {
  total: 0,
  bothDisasmOk: 0,
  bothDisasmFail: 0,
  rustDisasmOnlyFail: 0,
  jsDisasmOnlyFail: 0,
  disasmCountMatch: 0,
  disasmCountMismatch: 0,
  disasmMnemonicMismatch: 0,
  bothDecompileOk: 0,
  bothDecompileFail: 0,
  rustDecompileOnlyFail: 0,
  jsDecompileOnlyFail: 0,
};

const mismatches = [];

for (let i = 0; i < NUM_CASES; i++) {
  stats.total++;
  const scriptLen = randInt(1, 64);
  const script = generateRandomScript(scriptLen);
  const nefBytes = buildNefFromScript(script);
  const nefPath = join(TEMP_DIR, `fuzz_${i.toString().padStart(4, "0")}.nef`);
  writeFileSync(nefPath, nefBytes);

  // ---- Disassembly comparison ----
  const rustDisasm = runRustDisasm(nefPath);

  let jsDisasmOk = false;
  let jsResult = null;
  try {
    jsResult = decompileBytes(nefBytes);
    jsDisasmOk = true;
  } catch {
    jsDisasmOk = false;
  }

  if (rustDisasm.ok && jsDisasmOk) {
    stats.bothDisasmOk++;

    // Compare instruction counts
    const rustLines = rustDisasm.output.trim().split("\n").filter((l) => l.match(/^[0-9A-Fa-f]+:/));
    const rustCount = rustLines.length;
    const jsCount = jsResult.instructions.length;

    if (rustCount === jsCount) {
      stats.disasmCountMatch++;
    } else {
      stats.disasmCountMismatch++;
      mismatches.push({
        file: nefPath,
        type: "disasm_count",
        rust: rustCount,
        js: jsCount,
        scriptHex: Buffer.from(script).toString("hex"),
      });
    }

    // Compare mnemonic sequences
    if (rustCount === jsCount) {
      const rustMnemonics = rustLines.map((l) => {
        const m = l.trim().match(/^[0-9A-Fa-f]+:\s+(\S+)/);
        return m ? m[1] : "?";
      });
      const jsMnemonics = jsResult.instructions.map((i) => i.opcode.mnemonic);
      let mnemonicDiff = false;
      for (let j = 0; j < rustCount; j++) {
        if (rustMnemonics[j] !== jsMnemonics[j]) {
          mnemonicDiff = true;
          if (mismatches.length < 50) {
            mismatches.push({
              file: nefPath,
              type: "disasm_mnemonic",
              index: j,
              rust: rustMnemonics[j],
              js: jsMnemonics[j],
              scriptHex: Buffer.from(script).toString("hex"),
            });
          }
          break;
        }
      }
      if (mnemonicDiff) {
        stats.disasmMnemonicMismatch++;
      }
    }
  } else if (!rustDisasm.ok && !jsDisasmOk) {
    stats.bothDisasmFail++;
  } else if (!rustDisasm.ok && jsDisasmOk) {
    stats.rustDisasmOnlyFail++;
    if (mismatches.length < 50) {
      mismatches.push({
        file: nefPath,
        type: "disasm_rust_only_fail",
        rustOutput: rustDisasm.output.slice(0, 200),
        scriptHex: Buffer.from(script).toString("hex"),
      });
    }
  } else {
    stats.jsDisasmOnlyFail++;
    if (mismatches.length < 50) {
      mismatches.push({
        file: nefPath,
        type: "disasm_js_only_fail",
        scriptHex: Buffer.from(script).toString("hex"),
      });
    }
  }

  // ---- High-level decompilation comparison ----
  const rustDecompile = runRustDecompile(nefPath);

  let jsHighOk = false;
  let jsHighResult = null;
  try {
    jsHighResult = decompileHighLevelBytes(nefBytes);
    jsHighOk = true;
  } catch {
    jsHighOk = false;
  }

  if (rustDecompile.ok && jsHighOk) {
    stats.bothDecompileOk++;
  } else if (!rustDecompile.ok && !jsHighOk) {
    stats.bothDecompileFail++;
  } else if (!rustDecompile.ok && jsHighOk) {
    stats.rustDecompileOnlyFail++;
    if (mismatches.length < 50) {
      mismatches.push({
        file: nefPath,
        type: "decompile_rust_only_fail",
        rustOutput: rustDecompile.output.slice(0, 200),
        scriptHex: Buffer.from(script).toString("hex"),
      });
    }
  } else {
    stats.jsDecompileOnlyFail++;
    if (mismatches.length < 50) {
      mismatches.push({
        file: nefPath,
        type: "decompile_js_only_fail",
        scriptHex: Buffer.from(script).toString("hex"),
      });
    }
  }

  // Progress
  if ((i + 1) % 50 === 0) {
    console.log(`  ... ${i + 1}/${NUM_CASES} cases processed`);
  }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

console.log();
console.log("=".repeat(70));
console.log("DIFFERENTIAL FUZZ RESULTS");
console.log("=".repeat(70));
console.log();
console.log(`Total cases: ${stats.total}`);
console.log();
console.log("--- Disassembly ---");
console.log(`  Both succeeded:            ${stats.bothDisasmOk}`);
console.log(`  Both failed:               ${stats.bothDisasmFail}`);
console.log(`  Rust-only failure:         ${stats.rustDisasmOnlyFail}`);
console.log(`  JS-only failure:           ${stats.jsDisasmOnlyFail}`);
console.log(`  Instruction count match:   ${stats.disasmCountMatch}`);
console.log(`  Instruction count MISMATCH:${stats.disasmCountMismatch}`);
console.log(`  Mnemonic sequence MISMATCH:${stats.disasmMnemonicMismatch}`);
console.log();
console.log("--- High-level decompilation ---");
console.log(`  Both succeeded:            ${stats.bothDecompileOk}`);
console.log(`  Both failed:               ${stats.bothDecompileFail}`);
console.log(`  Rust-only failure:         ${stats.rustDecompileOnlyFail}`);
console.log(`  JS-only failure:           ${stats.jsDecompileOnlyFail}`);

if (mismatches.length > 0) {
  console.log();
  console.log("=".repeat(70));
  console.log(`MISMATCHES (${mismatches.length} total, showing up to 20):`);
  console.log("=".repeat(70));
  for (const m of mismatches.slice(0, 20)) {
    console.log();
    console.log(`  Type: ${m.type}`);
    console.log(`  File: ${m.file}`);
    if (m.scriptHex) console.log(`  Script (hex): ${m.scriptHex}`);
    if (m.type === "disasm_count") {
      console.log(`  Rust instruction count: ${m.rust}`);
      console.log(`  JS instruction count:   ${m.js}`);
    }
    if (m.type === "disasm_mnemonic") {
      console.log(`  Index: ${m.index}`);
      console.log(`  Rust mnemonic: ${m.rust}`);
      console.log(`  JS mnemonic:   ${m.js}`);
    }
    if (m.rustOutput) console.log(`  Rust output: ${m.rustOutput}`);
  }
}

// Cleanup
try {
  rmSync(TEMP_DIR, { recursive: true, force: true });
} catch {
  // best-effort cleanup
}

console.log();
if (
  stats.disasmCountMismatch === 0 &&
  stats.disasmMnemonicMismatch === 0 &&
  stats.jsDisasmOnlyFail === 0 &&
  stats.rustDisasmOnlyFail === 0 &&
  stats.jsDecompileOnlyFail === 0 &&
  stats.rustDecompileOnlyFail === 0
) {
  console.log("RESULT: All fuzz cases agree between Rust and JS implementations.");
} else {
  console.log("RESULT: Discrepancies found -- see details above.");
  process.exitCode = 1;
}
