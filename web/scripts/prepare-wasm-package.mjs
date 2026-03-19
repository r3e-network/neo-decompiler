import { rmSync } from "node:fs";
import { join } from "node:path";

const pkgDir = new URL("../dist/pkg/", import.meta.url);

for (const name of [".gitignore", "package.json", "README.md"]) {
  rmSync(join(pkgDir.pathname, name), { force: true });
}
