/**
 * Manifest corpus replay: feeds every JSON file from fuzz/corpus/fuzz_manifest
 * through both Rust (via CLI --manifest flag with /dev/null script) and JS
 * parseManifest. Verifies success/failure outcomes match.
 *
 * Usage:  node js/test/manifest-corpus-replay.mjs
 */

import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { parseManifest } from "../src/index.js";

const ROOT = join(import.meta.dirname, "..", "..");
const RUST_BIN = join(ROOT, "target", "release", "neo-decompiler");
const CORPUS_DIR = join(ROOT, "fuzz", "corpus", "fuzz_manifest");
const TEMP_DIR = mkdtempSync(join(tmpdir(), "neo-mfst-"));

function runRust(manifestPath) {
  try {
    execFileSync(RUST_BIN, ["decompile", "--manifest", manifestPath, "/dev/null"], {
      encoding: "utf-8",
      timeout: 5_000,
      stdio: ["pipe", "pipe", "pipe"],
    });
    return { ok: true };
  } catch (err) {
    const stderr = err.stderr ?? "";
    // The Rust CLI fails with "file too short" because /dev/null isn't a NEF.
    // That's the expected failure for "manifest accepted but no script" — treat
    // as Rust accepting the manifest. Any "manifest json parse error" means
    // Rust rejected the manifest itself.
    const isManifestError = stderr.includes("manifest json parse error");
    return { ok: !isManifestError, stderr };
  }
}

const stats = {
  total: 0,
  bothOk: 0,
  bothFail: 0,
  rustOnlyFail: 0,
  jsOnlyFail: 0,
};
const mismatches = [];

let entries;
try {
  entries = readdirSync(CORPUS_DIR);
} catch {
  console.error(`No corpus dir: ${CORPUS_DIR}`);
  process.exit(0);
}

console.log(`Replaying ${entries.length} manifest corpus inputs...`);
let i = 0;
for (const e of entries) {
  i++;
  if (i % 500 === 0) console.log(`  ... ${i}/${entries.length}`);

  const path = join(CORPUS_DIR, e);
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch {
    continue;
  }
  if (bytes.length === 0 || bytes.length > 100_000) continue;

  // Both impls expect UTF-8 JSON, so test only inputs that decode.
  let text;
  try {
    text = bytes.toString("utf-8");
  } catch {
    continue;
  }

  // Write to a temp file for the Rust CLI.
  const tmpPath = join(TEMP_DIR, `m_${i}.json`);
  writeFileSync(tmpPath, text);

  stats.total++;
  const rust = runRust(tmpPath);
  let jsOk = false;
  try {
    parseManifest(text);
    jsOk = true;
  } catch {}

  if (rust.ok && jsOk) stats.bothOk++;
  else if (!rust.ok && !jsOk) stats.bothFail++;
  else if (rust.ok && !jsOk) {
    stats.jsOnlyFail++;
    if (mismatches.length < 30) {
      mismatches.push({ path, type: "js_only_fail", size: bytes.length });
    }
  } else {
    stats.rustOnlyFail++;
    if (mismatches.length < 30) {
      mismatches.push({
        path,
        type: "rust_only_fail",
        size: bytes.length,
        rust: rust.stderr.slice(0, 120),
      });
    }
  }
}

console.log();
console.log("=".repeat(60));
console.log("MANIFEST CORPUS RESULTS");
console.log("=".repeat(60));
console.log(`Total: ${stats.total}`);
console.log(`Both succeeded:    ${stats.bothOk}`);
console.log(`Both failed:       ${stats.bothFail}`);
console.log(`Rust-only failure: ${stats.rustOnlyFail}`);
console.log(`JS-only failure:   ${stats.jsOnlyFail}`);

if (mismatches.length > 0) {
  console.log();
  console.log(`Mismatches (showing up to 30):`);
  for (const m of mismatches) {
    console.log(`  ${m.type}  size=${m.size}  ${m.path}`);
    if (m.rust) console.log(`    rust: ${m.rust}`);
  }
}

try {
  rmSync(TEMP_DIR, { recursive: true, force: true });
} catch {}

if (stats.rustOnlyFail + stats.jsOnlyFail === 0) {
  console.log("\nRESULT: Rust and JS agree on every manifest corpus input.");
} else {
  console.log("\nRESULT: Divergences found.");
  process.exitCode = 1;
}
