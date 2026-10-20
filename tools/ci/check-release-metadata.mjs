#!/usr/bin/env node
// Fail-closed release metadata gate.
//
// The crate version in Cargo.toml is the authoritative release identity: it
// names the crates.io artifact, the Git tag, and the npm Web package. The Web
// package version is a derived mirror of it, and the independently versioned JS
// package must record its Rust counterpart in the js/README.md mapping table.
//
// A release tag cut before those facts agree publishes inconsistent metadata:
// the crate lands on crates.io as one version while the npm packages carry
// another, and neither the tag-triggered workflow nor a later audit can
// reconcile them. This gate fails closed when they drift apart, so the
// inconsistency is caught before a tag exists rather than after.
//
// Usage: node tools/ci/check-release-metadata.mjs [--json]

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

const problems = [];
const fail = (message) => problems.push(message);

function readText(relativePath) {
  try {
    return readFileSync(resolve(repoRoot, relativePath), "utf8");
  } catch (error) {
    fail(`${relativePath}: cannot read (${error.message})`);
    return null;
  }
}

function readJson(relativePath) {
  const text = readText(relativePath);
  if (text === null) {
    return null;
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    fail(`${relativePath}: invalid JSON (${error.message})`);
    return null;
  }
}

// Cargo.toml declares exactly one top-level `version` key, in the [package]
// table, before any dependency table. Dependency versions are indented, so the
// anchored pattern cannot match them.
const cargoToml = readText("Cargo.toml");
const crateVersion = cargoToml?.match(/^version = "([^"]+)"$/m)?.[1] ?? null;
if (!crateVersion) {
  fail("Cargo.toml: could not read the [package] version");
} else if (!SEMVER.test(crateVersion)) {
  fail(`Cargo.toml: version ${JSON.stringify(crateVersion)} is not valid semver`);
}

const webPackage = readJson("web/package.json");
const webLock = readJson("web/package-lock.json");
const jsPackage = readJson("js/package.json");
const jsReadme = readText("js/README.md");

if (webPackage && webPackage.version !== crateVersion) {
  fail(
    `web/package.json: version ${JSON.stringify(webPackage.version)} does not match ` +
      `the crate version ${JSON.stringify(crateVersion)}`,
  );
}

if (webLock) {
  if (webLock.version !== crateVersion) {
    fail(
      `web/package-lock.json: version ${JSON.stringify(webLock.version)} does not match ` +
        `the crate version ${JSON.stringify(crateVersion)}`,
    );
  }
  const rootVersion = webLock.packages?.[""]?.version;
  if (rootVersion !== crateVersion) {
    fail(
      `web/package-lock.json: root package version ${JSON.stringify(rootVersion)} does not match ` +
        `the crate version ${JSON.stringify(crateVersion)}`,
    );
  }
}

let jsVersion = null;
if (jsPackage) {
  jsVersion = jsPackage.version;
  if (typeof jsVersion !== "string" || !SEMVER.test(jsVersion)) {
    fail(`js/package.json: version ${JSON.stringify(jsVersion)} is not valid semver`);
    jsVersion = null;
  }
  for (const field of ["dependencies", "optionalDependencies", "peerDependencies", "bundledDependencies"]) {
    const names = Object.keys(jsPackage[field] ?? {});
    if (names.length > 0) {
      fail(`js/package.json: ${field} must stay empty, found ${names.join(", ")}`);
    }
  }
}

if (jsReadme !== null && jsVersion) {
  // The mapping table rows look like `| 2.1.3 | 0.14.3 |`. The header row
  // carries a package name and the separator row carries dashes, so neither
  // matches the semver pattern used to recognize a real mapping.
  const mappings = jsReadme
    .split("\n")
    .filter((line) => line.trimStart().startsWith("|"))
    .map((line) => line.split("|").map((cell) => cell.trim()))
    .filter((cells) => cells.length >= 4 && SEMVER.test(cells[1]) && SEMVER.test(cells[2]));

  if (mappings.length === 0) {
    fail("js/README.md: no version-mapping rows were found");
  }
  if (!mappings.some((cells) => cells[1] === jsVersion && cells[2] === crateVersion)) {
    const known = mappings.map((cells) => `${cells[1]} -> ${cells[2]}`).join(", ");
    fail(
      `js/README.md: the version mapping table must map ${jsVersion} to ${crateVersion} ` +
        `(known mappings: ${known || "none"})`,
    );
  }
}

const report = {
  crate_version: crateVersion,
  web_version: webPackage?.version ?? null,
  js_version: jsVersion,
  problems,
};

if (process.argv.includes("--json")) {
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} else if (problems.length === 0) {
  process.stdout.write(
    `Release metadata is consistent: crate ${crateVersion}, web ${webPackage?.version}, js ${jsVersion}\n`,
  );
}

if (problems.length > 0) {
  for (const problem of problems) {
    process.stderr.write(`release metadata error: ${problem}\n`);
  }
  process.exit(1);
}
