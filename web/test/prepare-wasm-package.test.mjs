import assert from "node:assert/strict";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";

import { prepareWasmPackage } from "../scripts/prepare-wasm-package.mjs";

test("wasm preparation handles native drive letters and percent-encoded paths", () => {
  const directory = mkdtempSync(join(tmpdir(), "neo wasm % 中文 "));
  const unwanted = [".gitignore", "package.json", "README.md"];
  try {
    for (const name of [...unwanted, "neo_decompiler.js"]) {
      writeFileSync(join(directory, name), "fixture");
    }
    prepareWasmPackage(pathToFileURL(directory + sep));
    for (const name of unwanted) {
      assert.equal(existsSync(join(directory, name)), false, `${name} was not removed`);
    }
    assert.equal(existsSync(join(directory, "neo_decompiler.js")), true);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
