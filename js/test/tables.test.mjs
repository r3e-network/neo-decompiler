// Drift guards for the generated syscall and native-contract tables.
// Each committed entry must agree with its authoritative source so a stale
// generated file (the root cause of the earlier syscall-hash drift and the
// phantom native methods) is caught immediately.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import test from "node:test";

import { SYSCALLS } from "../src/generated/syscalls.js";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");

function trueSyscallHash(name) {
  return createHash("sha256").update(name).digest().readUInt32LE(0);
}

const nativeJson = JSON.parse(
  readFileSync(resolve(repoRoot, "tools", "data", "native_contracts.json"), "utf8"),
);
const nativeJsSource = readFileSync(resolve(here, "..", "src", "native-contracts.js"), "utf8");
const nativeRustSource = readFileSync(
  resolve(repoRoot, "src", "native_contracts_generated.rs"),
  "utf8",
);

test("tables: every SYSCALLS entry's key and hash equal SHA256-LE of its name", () => {
  for (const [key, entry] of SYSCALLS) {
    const expected = trueSyscallHash(entry.name);
    assert.equal(key, expected, `map key for ${entry.name}`);
    assert.equal(entry.hash, expected, `entry.hash for ${entry.name}`);
  }
});

test("tables: native method lists contain no phantom (non-ContractMethod) entries", () => {
  // OnManifestCompose is a protected manifest hook; VoteInternal is an
  // internal helper; ShouldRefreshCommittee is a public static helper —
  // none are [ContractMethod]s in the upstream sources.
  const phantoms = ["OnManifestCompose", "VoteInternal", "ShouldRefreshCommittee"];
  for (const contract of nativeJson) {
    for (const phantom of phantoms) {
      assert.ok(
        !contract.methods.includes(phantom),
        `${contract.name} must not list phantom method ${phantom}`,
      );
    }
  }
  for (const phantom of phantoms) {
    assert.ok(!nativeJsSource.includes(`"${phantom}"`), `JS table must not list ${phantom}`);
    assert.ok(!nativeRustSource.includes(`"${phantom}"`), `Rust table must not list ${phantom}`);
  }
});

test("tables: NeoToken exposes getCandidates across all three committed files", () => {
  const neo = nativeJson.find((c) => c.name === "NeoToken");
  assert.ok(neo, "NeoToken present in json");
  assert.ok(neo.methods.includes("GetCandidates"), "json NeoToken lists GetCandidates");
  // Both generated tables carry it too (Governance also lists it).
  assert.ok(nativeJsSource.includes('"GetCandidates"'), "JS table lists GetCandidates");
  assert.ok(nativeRustSource.includes('"GetCandidates"'), "Rust table lists GetCandidates");
});

test("tables: JS and Rust native tables list the same methods as the json source", () => {
  for (const contract of nativeJson) {
    for (const method of contract.methods) {
      assert.ok(
        nativeJsSource.includes(`"${method}"`),
        `JS table missing ${contract.name}.${method}`,
      );
      assert.ok(
        nativeRustSource.includes(`"${method}"`),
        `Rust table missing ${contract.name}.${method}`,
      );
    }
  }
});
