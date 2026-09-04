import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");
const cargoTomlPath = resolve(repoRoot, "Cargo.toml");
const webDir = resolve(repoRoot, "web");
const packageJsonPath = resolve(webDir, "package.json");
const packageLockPath = resolve(webDir, "package-lock.json");

const cargoToml = readFileSync(cargoTomlPath, "utf8");
const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
const packageLock = JSON.parse(readFileSync(packageLockPath, "utf8"));

const cargoVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1];
assert.ok(cargoVersion, "could not read Cargo.toml package version");

const mode = process.argv.includes("--write") ? "write" : "check";

if (mode === "check") {
  assert.equal(
    packageJson.version,
    cargoVersion,
    `web/package.json version ${packageJson.version} does not match Cargo.toml version ${cargoVersion}`,
  );
  assert.equal(packageLock.version, cargoVersion, "package-lock.json version does not match Cargo.toml");
  assert.equal(packageLock.packages[""].version, cargoVersion, "package-lock.json root version does not match Cargo.toml");
} else {
  packageJson.version = cargoVersion;
  packageLock.version = cargoVersion;
  packageLock.packages[""].version = cargoVersion;
  writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);
  writeFileSync(packageLockPath, `${JSON.stringify(packageLock, null, 2)}\n`);
}
