import { rmSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function prepareWasmPackage(pkgDir = new URL("../dist/pkg/", import.meta.url)) {
  for (const name of [".gitignore", "package.json", "README.md"]) {
    // Pass file URLs directly: URL.pathname is percent-encoded and starts with
    // /C:/ on Windows, neither of which is a native filesystem path.
    rmSync(new URL(name, pkgDir), { force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  prepareWasmPackage();
}
