import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import test from "node:test";

import {
  decompileBytes,
  decompileHighLevelBytes,
  decompileHighLevelBytesWithManifest,
} from "../src/index.js";

const ROOT = join(import.meta.dirname, "..", "..");
const RUST_BIN = join(ROOT, "target", "release", "neo-decompiler");
const ARTIFACTS_DIR = join(ROOT, "TestingArtifacts");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function findNefFiles(dir) {
  const results = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...findNefFiles(full));
    } else if (entry.name.endsWith(".nef")) {
      results.push(full);
    }
  }
  return results;
}

function runRust(subcommand, nefPath) {
  try {
    return execFileSync(RUST_BIN, [subcommand, nefPath], {
      encoding: "utf-8",
      timeout: 15_000,
      stdio: ["pipe", "pipe", "pipe"],
    });
  } catch (err) {
    // Return stderr + stdout combined so we can detect errors
    const out = (err.stdout ?? "") + (err.stderr ?? "");
    return { error: true, output: out, status: err.status };
  }
}

/**
 * Parse a Rust disasm line like "0000: PUSH1 1" into { offset, mnemonic, operand }.
 */
function parseRustDisasmLine(line) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("//")) return null;
  const match = trimmed.match(/^([0-9A-Fa-f]+):\s+(\S+)(?:\s+(.*))?$/);
  if (!match) return null;
  return {
    offset: parseInt(match[1], 16),
    mnemonic: match[2],
    operand: (match[3] ?? "").trim(),
  };
}

/**
 * Parse Rust disasm output into an array of parsed instructions.
 */
function parseRustDisasm(output) {
  return output
    .split("\n")
    .map(parseRustDisasmLine)
    .filter(Boolean);
}

/**
 * Count keyword occurrences in high-level output.
 */
function countKeywords(text) {
  const keywords = ["if ", "else ", "while ", "for ", "try ", "catch ", "goto ", "return", "fn "];
  const counts = {};
  for (const kw of keywords) {
    const regex = new RegExp(kw.trim() === "fn" ? "\\bfn\\s" : kw.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "g");
    counts[kw.trim()] = (text.match(regex) ?? []).length;
  }
  return counts;
}

/**
 * Count function bodies (implementations) in high-level output.
 * Only counts `fn name(...) {` patterns, not forward declarations like `fn name() -> int;`
 */
function countFunctionBodies(text) {
  return (text.match(/\bfn\s+\w+\([^)]*\)(?:\s*->\s*\w+)?\s*\{/g) ?? []).length;
}

/**
 * Find the manifest file path that matches a .nef file (same stem + .manifest.json).
 */
function findManifestForNef(nefPath) {
  const stem = nefPath.replace(/\.nef$/, "");
  const manifestPath = stem + ".manifest.json";
  try {
    statSync(manifestPath);
    return manifestPath;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Discover test files
// ---------------------------------------------------------------------------

const nefFiles = findNefFiles(ARTIFACTS_DIR);

test("differential: found test artifacts", () => {
  assert.ok(nefFiles.length > 0, `expected .nef files in ${ARTIFACTS_DIR}`);
});

// ---------------------------------------------------------------------------
// Disassembly comparison tests
// ---------------------------------------------------------------------------

for (const nefPath of nefFiles) {
  const label = relative(ROOT, nefPath);

  test(`differential disasm: ${label}`, () => {
    const bytes = readFileSync(nefPath);

    // Rust disassembly
    const rustRaw = runRust("disasm", nefPath);
    if (typeof rustRaw === "object" && rustRaw.error) {
      // Rust failed - check that JS also fails
      let jsFailed = false;
      try {
        decompileBytes(bytes);
      } catch {
        jsFailed = true;
      }
      // If Rust failed, it is acceptable for JS to either fail or succeed
      // (they may have different strictness). Log it.
      if (!jsFailed) {
        console.log(`  NOTE: Rust disasm failed but JS succeeded for ${label}`);
      }
      return;
    }

    // JS disassembly
    let jsResult;
    try {
      jsResult = decompileBytes(bytes);
    } catch (err) {
      assert.fail(`JS disassembly threw but Rust succeeded: ${err.message}`);
    }

    const rustInstructions = parseRustDisasm(rustRaw);
    const jsInstructions = jsResult.instructions;

    // Compare instruction counts
    assert.equal(
      jsInstructions.length,
      rustInstructions.length,
      `instruction count mismatch: JS=${jsInstructions.length} Rust=${rustInstructions.length}`,
    );

    // Compare opcode sequences
    const jsMnemonics = jsInstructions.map((i) => i.opcode.mnemonic);
    const rustMnemonics = rustInstructions.map((i) => i.mnemonic);

    for (let idx = 0; idx < Math.min(jsMnemonics.length, rustMnemonics.length); idx++) {
      assert.equal(
        jsMnemonics[idx],
        rustMnemonics[idx],
        `mnemonic mismatch at index ${idx} (offset 0x${rustInstructions[idx].offset.toString(16).padStart(4, "0")}): JS='${jsMnemonics[idx]}' Rust='${rustMnemonics[idx]}'`,
      );
    }

    // Compare offsets
    for (let idx = 0; idx < Math.min(jsInstructions.length, rustInstructions.length); idx++) {
      assert.equal(
        jsInstructions[idx].offset,
        rustInstructions[idx].offset,
        `offset mismatch at instruction ${idx}: JS=0x${jsInstructions[idx].offset.toString(16)} Rust=0x${rustInstructions[idx].offset.toString(16)}`,
      );
    }

    // Compare pseudocode text line-by-line (the formatted text form)
    const jsPseudo = jsResult.pseudocode.trimEnd();
    const rustText = rustRaw.trimEnd();
    const jsLines = jsPseudo.split("\n");
    const rustLines = rustText.split("\n");

    assert.equal(
      jsLines.length,
      rustLines.length,
      `pseudocode line count mismatch: JS=${jsLines.length} Rust=${rustLines.length}`,
    );

    const mismatches = [];
    for (let i = 0; i < Math.min(jsLines.length, rustLines.length); i++) {
      if (jsLines[i] !== rustLines[i]) {
        mismatches.push({
          line: i + 1,
          js: jsLines[i],
          rust: rustLines[i],
        });
      }
    }

    if (mismatches.length > 0) {
      const details = mismatches
        .slice(0, 10)
        .map((m) => `  line ${m.line}:\n    JS:   ${JSON.stringify(m.js)}\n    Rust: ${JSON.stringify(m.rust)}`)
        .join("\n");
      assert.fail(
        `pseudocode text differs in ${mismatches.length} line(s):\n${details}`,
      );
    }
  });
}

// ---------------------------------------------------------------------------
// High-level decompilation comparison tests
// ---------------------------------------------------------------------------

for (const nefPath of nefFiles) {
  const label = relative(ROOT, nefPath);

  test(`differential high-level: ${label}`, () => {
    const bytes = readFileSync(nefPath);

    // Rust high-level (auto-discovers manifest from sibling .manifest.json)
    const rustRaw = runRust("decompile", nefPath);
    if (typeof rustRaw === "object" && rustRaw.error) {
      // Rust failed - check JS
      let jsFailed = false;
      try {
        decompileHighLevelBytes(bytes);
      } catch {
        jsFailed = true;
      }
      if (!jsFailed) {
        console.log(`  NOTE: Rust decompile failed but JS succeeded for ${label}`);
      }
      return;
    }

    // JS high-level -- use manifest when available (matching Rust auto-discovery).
    // Pass `clean: true` so the JS render applies the same inline-single-use-temps
    // postprocess pass the Rust CLI uses by default. Without this, JS temps stay
    // un-inlined (`var t1 = 3; if (loc0 < t1) ...`) while the Rust CLI inlines
    // them (`if (loc0 < 3) ...`), spuriously inflating the line-by-line diff
    // counters this test logs.
    const manifestPath = findManifestForNef(nefPath);
    let jsResult;
    try {
      if (manifestPath) {
        const manifestJson = readFileSync(manifestPath, "utf-8");
        jsResult = decompileHighLevelBytesWithManifest(bytes, manifestJson, { clean: true });
      } else {
        jsResult = decompileHighLevelBytes(bytes, { clean: true });
      }
    } catch (err) {
      assert.fail(`JS high-level threw but Rust succeeded: ${err.message}`);
    }

    const rustText = rustRaw.trimEnd();
    const jsText = jsResult.highLevel.trimEnd();

    // ---- Structural comparison: function body count ----
    // Count only implementation bodies (`fn name() {`), not forward declarations (`fn name();`)
    const rustFnBodies = countFunctionBodies(rustText);
    const jsFnBodies = countFunctionBodies(jsText);

    if (jsFnBodies !== rustFnBodies) {
      // The Rust decompiler does more aggressive method splitting (e.g., splitting
      // on RET boundaries to discover unreachable sub-functions). The JS only splits
      // on manifest offsets, INITSLOT, and CALL targets. Log as a real discrepancy.
      console.log(
        `  DISCREPANCY: function body count: JS=${jsFnBodies} Rust=${rustFnBodies} for ${label}`,
      );
      // Hard-fail only if JS finds MORE functions than Rust (JS should never
      // split into more methods than Rust, since Rust applies a superset of heuristics)
      if (jsFnBodies > rustFnBodies) {
        assert.fail(
          `JS found more function bodies than Rust: JS=${jsFnBodies} Rust=${rustFnBodies}\nJS output:\n${jsText}\nRust output:\n${rustText}`,
        );
      }
    }

    // ---- Structural comparison: control flow keywords ----
    const rustKw = countKeywords(rustText);
    const jsKw = countKeywords(jsText);

    const kwDiffs = [];
    for (const kw of Object.keys(rustKw)) {
      if (rustKw[kw] !== jsKw[kw]) {
        kwDiffs.push(`  '${kw}': JS=${jsKw[kw]} Rust=${rustKw[kw]}`);
      }
    }

    if (kwDiffs.length > 0) {
      // Log keyword differences as informational rather than hard failures,
      // since slight rendering differences are expected between implementations.
      console.log(`  INFO: keyword count differences for ${label}:`);
      for (const d of kwDiffs) {
        console.log(d);
      }
    }

    // ---- Contract name wrapper ----
    // Both should start with "contract ... {"
    const rustContractMatch = rustText.match(/^contract\s+(\w+)\s*\{/);
    const jsContractMatch = jsText.match(/^contract\s+(\w+)\s*\{/);
    assert.ok(rustContractMatch, `Rust output does not start with 'contract ... {'`);
    assert.ok(jsContractMatch, `JS output does not start with 'contract ... {'`);

    // ---- Line-by-line diff (informational) ----
    const rustLines = rustText.split("\n");
    const jsLines = jsText.split("\n");
    let diffCount = 0;
    const maxLines = Math.max(rustLines.length, jsLines.length);
    for (let i = 0; i < maxLines; i++) {
      if ((rustLines[i] ?? "") !== (jsLines[i] ?? "")) {
        diffCount++;
      }
    }
    if (diffCount > 0) {
      const pct = ((diffCount / maxLines) * 100).toFixed(1);
      console.log(`  INFO: ${diffCount}/${maxLines} lines differ (${pct}%) for ${label}`);
    }
  });
}
