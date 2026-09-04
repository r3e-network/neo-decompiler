import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { verifyPackedTarball } from "./check-pack.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const webDir = resolve(here, "..");

export function createReleasePack(packageDirectory = webDir) {
  // build:package must already have run. --ignore-scripts guarantees packing
  // does not rebuild content after tests and validation. Windows cannot spawn
  // a .cmd file directly; this shell command contains no caller interpolation.
  const [command, args] = process.platform === "win32"
    ? [process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", "npm.cmd pack --json --ignore-scripts"]]
    : ["npm", ["pack", "--json", "--ignore-scripts"]];
  const raw = execFileSync(command, args, {
    cwd: packageDirectory,
    encoding: "utf8",
  });
  const packed = JSON.parse(raw);
  assert.ok(Array.isArray(packed) && packed.length === 1, "expected one npm pack result");
  assert.equal(
    packed[0].filename,
    `neo-decompiler-web-${packed[0].version}.tgz`,
    "npm returned an unexpected archive filename",
  );
  return verifyPackedTarball(join(packageDirectory, packed[0].filename));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.stdout.write(`${JSON.stringify(createReleasePack(), null, 2)}\n`);
}
