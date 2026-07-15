/**
 * Real TestingArtifacts NEF+manifest fixtures must decompile to high-level
 * Neo N3 C# SmartContract source with pattern headers (parity with Rust gates).
 */
import assert from "node:assert/strict";
import { readdirSync, readFileSync, existsSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { decompileHighLevelBytesWithManifest } from "../src/index.js";

const ROOT = join(fileURLToPath(new URL(".", import.meta.url)), "../..");
const ARTIFACTS = join(ROOT, "TestingArtifacts");

function collectNefs(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "decompiled") {
        collectNefs(join(path, "embedded"), out);
        continue;
      }
      collectNefs(path, out);
      continue;
    }
    if (entry.name.endsWith(".nef")) out.push(path);
  }
  return out;
}

function skipList() {
  const skips = [];
  for (const name of ["expected_invalid.txt", "known_unsupported.txt"]) {
    const path = join(ARTIFACTS, name);
    if (!existsSync(path)) continue;
    for (const line of readFileSync(path, "utf8").split("\n")) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      skips.push(trimmed.split(":")[0].trim());
    }
  }
  return skips;
}

test("all supported TestingArtifacts decompile to high-level C# contracts", () => {
  const skips = skipList();
  const nefs = collectNefs(ARTIFACTS).sort();
  assert.ok(nefs.length > 0, "expected TestingArtifacts NEFs");

  let checked = 0;
  for (const nefPath of nefs) {
    const rel = relative(ARTIFACTS, nefPath).replaceAll("\\", "/");
    if (skips.some((skip) => rel.includes(skip))) continue;

    const stem = nefPath.slice(0, -".nef".length);
    const manifestPath = `${stem}.manifest.json`;
    assert.ok(existsSync(manifestPath), `manifest for ${rel}`);

    const bytes = new Uint8Array(readFileSync(nefPath));
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    const result = decompileHighLevelBytesWithManifest(bytes, manifest);
    const csharp = result.csharp;
    const highLevel = result.highLevel;

    assert.match(csharp, /SmartContract/, `${rel} missing SmartContract`);
    assert.match(csharp, /public static \w+ \w+\s*\(/, `${rel} missing methods`);
    assert.match(csharp, /pattern confidence:/, `${rel} missing pattern header`);
    assert.match(highLevel, /contract /, `${rel} missing high-level envelope`);
    assert.match(highLevel, /fn /, `${rel} missing high-level methods`);

    if (rel.includes("LoopIf")) {
      assert.doesNotMatch(csharp, /while \(true\)/, `LoopIf while(true):\n${csharp}`);
      assert.doesNotMatch(highLevel, /loop \{/, `LoopIf loop form:\n${highLevel}`);
      assert.match(csharp, /for \(|while \(/, `LoopIf structured loop:\n${csharp}`);
    }
    if (rel.includes("events/Events")) {
      assert.match(csharp, /public static event /, `Events missing events:\n${csharp}`);
      assert.match(
        csharp,
        /inferred patterns: events/,
        `Events missing pattern:\n${csharp}`,
      );
    }
    if (rel.includes("multi/MultiMethod")) {
      assert.match(csharp, /main\(/, `MultiMethod main:\n${csharp}`);
      assert.match(csharp, /helper\(/, `MultiMethod helper:\n${csharp}`);
    }
    checked += 1;
  }
  assert.ok(checked >= 6, `checked only ${checked} fixtures`);
});
