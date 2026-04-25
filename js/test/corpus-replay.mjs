/**
 * Corpus replay tester: feeds every NEF corpus file from fuzz/corpus/fuzz_nef_parse
 * and fuzz/corpus/fuzz_decompile through both the Rust CLI and the JS API,
 * then verifies they agree on success / failure for each input.
 *
 * Usage:  node js/test/corpus-replay.mjs
 */

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

import { decompileBytes, decompileHighLevelBytes } from "../src/index.js";

const ROOT = join(import.meta.dirname, "..", "..");
const RUST_BIN = join(ROOT, "target", "release", "neo-decompiler");
const CORPUS_DIRS = [
  join(ROOT, "fuzz", "corpus", "fuzz_nef_parse"),
  join(ROOT, "fuzz", "corpus", "fuzz_decompile"),
];

function runRust(cmd, path) {
  try {
    execFileSync(RUST_BIN, [cmd, path], {
      encoding: "utf-8",
      timeout: 5_000,
      stdio: ["pipe", "pipe", "pipe"],
    });
    return true;
  } catch {
    return false;
  }
}

const stats = {
  total: 0,
  bothDisasmOk: 0,
  bothDisasmFail: 0,
  rustOnlyDisasm: 0,
  jsOnlyDisasm: 0,
  bothDecompileOk: 0,
  bothDecompileFail: 0,
  rustOnlyDecompile: 0,
  jsOnlyDecompile: 0,
};
const mismatches = [];

const cases = [];
for (const dir of CORPUS_DIRS) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    continue;
  }
  for (const e of entries) {
    const p = join(dir, e);
    try {
      const s = statSync(p);
      if (s.isFile() && s.size > 0 && s.size <= 1024 * 1024) {
        cases.push(p);
      }
    } catch {}
  }
}

console.log(`Replaying ${cases.length} corpus inputs against Rust + JS...`);
let i = 0;
for (const path of cases) {
  i++;
  if (i % 500 === 0) console.log(`  ... ${i}/${cases.length}`);
  stats.total++;

  let bytes;
  try {
    bytes = readFileSync(path);
  } catch {
    continue;
  }

  const rustDisasm = runRust("disasm", path);
  let jsDisasm = false;
  try {
    decompileBytes(bytes);
    jsDisasm = true;
  } catch {}

  if (rustDisasm && jsDisasm) stats.bothDisasmOk++;
  else if (!rustDisasm && !jsDisasm) stats.bothDisasmFail++;
  else if (rustDisasm) {
    stats.jsOnlyDisasm++;
    if (mismatches.length < 30) {
      mismatches.push({ path, type: "disasm_js_only_fail", size: bytes.length });
    }
  } else {
    stats.rustOnlyDisasm++;
    if (mismatches.length < 30) {
      mismatches.push({ path, type: "disasm_rust_only_fail", size: bytes.length });
    }
  }

  const rustDecompile = runRust("decompile", path);
  let jsDecompile = false;
  try {
    decompileHighLevelBytes(bytes);
    jsDecompile = true;
  } catch {}

  if (rustDecompile && jsDecompile) stats.bothDecompileOk++;
  else if (!rustDecompile && !jsDecompile) stats.bothDecompileFail++;
  else if (rustDecompile) {
    stats.jsOnlyDecompile++;
    if (mismatches.length < 30) {
      mismatches.push({ path, type: "decompile_js_only_fail", size: bytes.length });
    }
  } else {
    stats.rustOnlyDecompile++;
    if (mismatches.length < 30) {
      mismatches.push({ path, type: "decompile_rust_only_fail", size: bytes.length });
    }
  }
}

console.log();
console.log("=".repeat(70));
console.log("CORPUS REPLAY RESULTS");
console.log("=".repeat(70));
console.log();
console.log(`Total inputs: ${stats.total}`);
console.log();
console.log("--- Disassembly ---");
console.log(`  Both succeeded:           ${stats.bothDisasmOk}`);
console.log(`  Both failed:              ${stats.bothDisasmFail}`);
console.log(`  Rust-only failure:        ${stats.rustOnlyDisasm}`);
console.log(`  JS-only failure:          ${stats.jsOnlyDisasm}`);
console.log();
console.log("--- High-level decompile ---");
console.log(`  Both succeeded:           ${stats.bothDecompileOk}`);
console.log(`  Both failed:              ${stats.bothDecompileFail}`);
console.log(`  Rust-only failure:        ${stats.rustOnlyDecompile}`);
console.log(`  JS-only failure:          ${stats.jsOnlyDecompile}`);

if (mismatches.length > 0) {
  console.log();
  console.log(`MISMATCHES (showing up to 30):`);
  for (const m of mismatches.slice(0, 30)) {
    console.log(`  ${m.type}  size=${m.size}  ${m.path}`);
  }
}

console.log();
const ok =
  stats.rustOnlyDisasm === 0 &&
  stats.jsOnlyDisasm === 0 &&
  stats.rustOnlyDecompile === 0 &&
  stats.jsOnlyDecompile === 0;
console.log(ok ? "RESULT: Rust and JS agree on every corpus input." : "RESULT: Divergences found.");
if (!ok) process.exitCode = 1;
