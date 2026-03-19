import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");
const cargoTomlPath = resolve(repoRoot, "Cargo.toml");
const webDir = resolve(repoRoot, "web");
const packageJsonPath = resolve(webDir, "package.json");

const cargoToml = readFileSync(cargoTomlPath, "utf8");
const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));

const cargoVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1];
assert.ok(cargoVersion, "could not read Cargo.toml package version");

const mode = process.argv.includes("--write") ? "write" : "check";

if (mode === "check") {
  assert.equal(
    packageJson.version,
    cargoVersion,
    `web/package.json version ${packageJson.version} does not match Cargo.toml version ${cargoVersion}`,
  );
} else {
  packageJson.version = cargoVersion;
  writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
  execFileSync("npm", ["install", "--package-lock-only"], {
    cwd: webDir,
    stdio: "inherit",
  });
}
