import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";

const raw = execFileSync("npm", ["pack", "--json", "--dry-run"], {
  cwd: new URL("../", import.meta.url),
  encoding: "utf8",
});

const result = JSON.parse(raw);
assert.ok(Array.isArray(result) && result.length === 1, "expected one pack result");

const pack = result[0];
const paths = new Set(pack.files.map((entry) => entry.path));

for (const required of [
  "README.md",
  "dist/index.js",
  "dist/index.d.ts",
  "dist/pkg/neo_decompiler.js",
  "dist/pkg/neo_decompiler.d.ts",
  "dist/pkg/neo_decompiler_bg.wasm",
  "dist/pkg/neo_decompiler_bg.wasm.d.ts",
  "dist/pkg/LICENSE-MIT",
  "dist/pkg/LICENSE-APACHE",
]) {
  assert.ok(paths.has(required), `missing packaged file: ${required}`);
}
