import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

test("version sync updates both lockfile versions without npm or dependency changes", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-version-sync-"));
  try {
    mkdirSync(join(root, "web", "scripts"), { recursive: true });
    copyFileSync(new URL("../scripts/sync-version.mjs", import.meta.url), join(root, "web", "scripts", "sync-version.mjs"));
    writeFileSync(join(root, "Cargo.toml"), '[package]\nversion = "0.14.0"\n');
    const metadata = { name: "neo-decompiler-web", version: "0.12.0" };
    const dependency = { version: "5.9.3", integrity: "unchanged" };
    writeFileSync(join(root, "web", "package.json"), JSON.stringify(metadata));
    writeFileSync(join(root, "web", "package-lock.json"), JSON.stringify({
      ...metadata, lockfileVersion: 3, packages: { "": metadata, "node_modules/typescript": dependency },
    }));
    const run = (mode) => spawnSync(process.execPath, [join(root, "web", "scripts", "sync-version.mjs"), mode], {
      encoding: "utf8", env: { ...process.env, PATH: "", Path: "" },
    });
    assert.notEqual(run("--check").status, 0);
    const write = run("--write");
    assert.equal(write.status, 0, write.stderr);
    const check = run("--check");
    assert.equal(check.status, 0, check.stderr);
    const lock = JSON.parse(readFileSync(join(root, "web", "package-lock.json"), "utf8"));
    assert.equal(lock.version, "0.14.0");
    assert.equal(lock.packages[""].version, "0.14.0");
    assert.deepEqual(lock.packages["node_modules/typescript"], dependency);
    lock.packages[""].version = "0.12.0";
    writeFileSync(join(root, "web", "package-lock.json"), JSON.stringify(lock));
    assert.notEqual(run("--check").status, 0, "stale lockfile root must fail the release gate");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
