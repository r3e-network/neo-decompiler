import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { lstatSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { gzipSync } from "node:zlib";

import { verifyPackedTarball } from "../scripts/check-pack.mjs";
import { createReleasePack } from "../scripts/create-release-pack.mjs";

const PACKAGE_FILES = [
  "README.md",
  "dist/index.js",
  "dist/index.d.ts",
  "dist/pkg/neo_decompiler.js",
  "dist/pkg/neo_decompiler.d.ts",
  "dist/pkg/neo_decompiler_bg.wasm",
  "dist/pkg/neo_decompiler_bg.wasm.d.ts",
  "dist/pkg/LICENSE-MIT",
  "dist/pkg/LICENSE-APACHE",
];

const TAR_BLOCK_BYTES = 512;

function writeTarString(header, offset, length, value) {
  const encoded = Buffer.from(value, "utf8");
  assert.ok(encoded.length <= length, `test tar field is too long: ${value}`);
  encoded.copy(header, offset);
}

function writeTarOctal(header, offset, length, value) {
  const encoded = value.toString(8).padStart(length - 1, "0");
  assert.equal(encoded.length, length - 1, "test tar number does not fit");
  writeTarString(header, offset, length, `${encoded}\0`);
}

function buildTarHeader({ name, size = 0, type = "0", linkTarget = "" }) {
  const header = Buffer.alloc(TAR_BLOCK_BYTES);
  writeTarString(header, 0, 100, name);
  writeTarOctal(header, 100, 8, type === "5" ? 0o755 : 0o644);
  writeTarOctal(header, 108, 8, 0);
  writeTarOctal(header, 116, 8, 0);
  writeTarOctal(header, 124, 12, size);
  writeTarOctal(header, 136, 12, 0);
  header.fill(0x20, 148, 156);
  writeTarString(header, 156, 1, type);
  writeTarString(header, 157, 100, linkTarget);
  writeTarString(header, 257, 6, "ustar\0");
  writeTarString(header, 263, 2, "00");
  refreshChecksum(header);
  return header;
}

function refreshChecksum(header) {
  header.fill(0x20, 148, 156);
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  writeTarString(header, 148, 8, `${checksum.toString(8).padStart(6, "0")}\0 `);
}

function buildTar(entries) {
  const chunks = [];
  for (const entry of entries) {
    const data = Buffer.from(entry.data ?? []);
    const size = entry.size ?? data.length;
    assert.equal(data.length, size, "test tar data must match its declared size");
    chunks.push(buildTarHeader({ ...entry, size }), data);
    const padding = (TAR_BLOCK_BYTES - (data.length % TAR_BLOCK_BYTES)) % TAR_BLOCK_BYTES;
    if (padding > 0) {
      chunks.push(Buffer.alloc(padding));
    }
  }
  chunks.push(Buffer.alloc(TAR_BLOCK_BYTES * 2));
  return Buffer.concat(chunks);
}

function paxRecord(key, value) {
  const body = Buffer.from(`${key}=${value}\n`, "utf8");
  let digits = 1;
  for (;;) {
    const length = digits + 1 + body.length;
    const encodedLength = String(length);
    if (encodedLength.length === digits) {
      return Buffer.concat([Buffer.from(`${encodedLength} `, "ascii"), body]);
    }
    digits = encodedLength.length;
  }
}

function packageFileData(path) {
  if (path === "package.json") {
    return Buffer.from(`${JSON.stringify({ name: "neo-decompiler-web", version: "1.2.3" })}\n`);
  }
  return path.endsWith(".wasm") ? Buffer.from([0, 97, 115, 109]) : Buffer.from(`${path}\n`);
}

function buildExtendedHeaderPackage() {
  const entries = [
    { name: "package/", type: "5" },
    { name: "package/dist/", type: "5" },
    { name: "package/dist/pkg/", type: "5" },
    { name: "package/package.json", data: packageFileData("package.json") },
  ];
  for (const path of PACKAGE_FILES) {
    const archivePath = `package/${path}`;
    if (path === "dist/index.js") {
      entries.push(
        { name: "PaxHeaders/index.js", type: "x", data: paxRecord("path", archivePath) },
        { name: "pax-placeholder", data: packageFileData(path) },
      );
    } else if (path === "dist/index.d.ts") {
      entries.push(
        { name: "././@LongLink", type: "L", data: Buffer.from(`${archivePath}\0`) },
        { name: "gnu-placeholder", data: packageFileData(path) },
      );
    } else {
      entries.push({ name: archivePath, data: packageFileData(path) });
    }
  }
  return buildTar(entries);
}

function writeTarball(root, tar) {
  const archive = join(root, "neo-decompiler-web-1.2.3.tgz");
  writeFileSync(archive, gzipSync(tar));
  return archive;
}

function createPackageTree(packageRoot) {
  mkdirSync(packageRoot, { recursive: true });
  writeFileSync(
    join(packageRoot, "package.json"),
    `${JSON.stringify({ name: "neo-decompiler-web", version: "1.2.3" })}\n`,
  );
  for (const path of PACKAGE_FILES) {
    const target = join(packageRoot, ...path.split("/"));
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, path.endsWith(".wasm") ? Buffer.from([0, 97, 115, 109]) : `${path}\n`);
  }
}

function buildFixture({ extraFile = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), "neo-decompiler-pack-test-"));
  const packageRoot = join(root, "staging", "package");
  createPackageTree(packageRoot);
  if (extraFile) {
    writeFileSync(join(packageRoot, "dist", "unexpected.js"), "unexpected\n");
  }

  const archive = join(root, "neo-decompiler-web-1.2.3.tgz");
  execFileSync("tar", ["--format=ustar", "-czf", archive, "-C", join(root, "staging"), "package"]);
  return { archive, root };
}

function buildNpmFixture() {
  const root = mkdtempSync(join(tmpdir(), "neo-decompiler-npm-pack-test-"));
  const packageRoot = join(root, "source");
  createPackageTree(packageRoot);
  const command =
    process.platform === "win32"
      ? [
          process.env.ComSpec,
          ["/d", "/s", "/c", "npm.cmd pack --json --ignore-scripts --pack-destination .."],
        ]
      : ["npm", ["pack", "--json", "--ignore-scripts", "--pack-destination", ".."]];
  const packed = JSON.parse(
    execFileSync(command[0], command[1], { cwd: packageRoot, encoding: "utf8" }),
  );
  assert.equal(packed.length, 1);
  return { archive: join(root, packed[0].filename), root };
}

test("release package verifier accepts the exact allowlist", () => {
  const fixture = buildFixture();
  try {
    const result = verifyPackedTarball(fixture.archive);
    assert.equal(result.filename, "neo-decompiler-web-1.2.3.tgz");
    assert.match(result.sha256, /^[0-9a-f]{64}$/);
    assert.equal(result.files.length, PACKAGE_FILES.length + 1);
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test("release package verifier rejects unexpected files", () => {
  const fixture = buildFixture({ extraFile: true });
  try {
    assert.throws(
      () => verifyPackedTarball(fixture.archive),
      /archive contains an unexpected path: package\/dist\/unexpected\.js/,
    );
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test("release package verifier accepts a normal npm-generated tarball", () => {
  const fixture = buildNpmFixture();
  try {
    const result = verifyPackedTarball(fixture.archive);
    assert.equal(result.filename, "neo-decompiler-web-1.2.3.tgz");
    assert.equal(result.files.length, PACKAGE_FILES.length + 1);
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test("release archive creation works from native paths and never reruns prepack", () => {
  const root = mkdtempSync(join(tmpdir(), "neo release pack % "));
  try {
    createPackageTree(root);
    writeFileSync(join(root, "package.json"), JSON.stringify({
      name: "neo-decompiler-web",
      version: "1.2.3",
      files: ["dist", "README.md"],
      scripts: { prepack: "node -e \"throw new Error('prepack must not run')\"" },
    }));
    const result = createReleasePack(root);
    assert.equal(result.filename, "neo-decompiler-web-1.2.3.tgz");
    assert.equal(result.files.length, PACKAGE_FILES.length + 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("tar preflight rejects octal encodings that ASCII decoding would alias", () => {
  for (const mutate of [
    (header) => { header[124] |= 0x80; },
    (header) => { header[125] |= 0x80; },
    (header) => { header[124] = 0x09; },
    (header) => { header[130] = 0; },
  ]) {
    const root = mkdtempSync(join(tmpdir(), "neo-tar-number-test-"));
    try {
      const tar = buildExtendedHeaderPackage();
      // The first directory declares size zero; mutate only the size encoding.
      const header = tar.subarray(0, TAR_BLOCK_BYTES);
      mutate(header);
      refreshChecksum(header);
      assert.throws(() => verifyPackedTarball(writeTarball(root, tar)), /tar member size|non-zero declared size/u);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("tar preflight rejects GNU magic before interpreting the ustar prefix", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-tar-dialect-test-"));
  try {
    const tar = buildExtendedHeaderPackage();
    const header = tar.subarray(0, TAR_BLOCK_BYTES);
    Buffer.from("ustar  \0").copy(header, 257);
    refreshChecksum(header);
    assert.throws(() => verifyPackedTarball(writeTarball(root, tar)), /not POSIX ustar/u);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("tar preflight rejects high-bit PAX keywords and record lengths", () => {
  for (const mutate of [
    (record) => { record[0] |= 0x80; },
    (record) => { record[record.indexOf(0x20) + 1] |= 0x80; },
  ]) {
    const root = mkdtempSync(join(tmpdir(), "neo-pax-encoding-test-"));
    try {
      const data = paxRecord("path", "package/README.md");
      mutate(data);
      const tar = buildTar([{ name: "PaxHeaders/readme", type: "x", data }]);
      assert.throws(() => verifyPackedTarball(writeTarball(root, tar)), /PAX record has an invalid length|PAX keyword is not valid UTF-8/u);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("metadata-only local extensions cannot be stacked or orphaned", () => {
  for (const count of [1, 2]) {
    const root = mkdtempSync(join(tmpdir(), "neo-pax-metadata-test-"));
    try {
      const tar = buildTar(Array.from({ length: count }, () => ({
        name: "PaxHeaders/readme", type: "x", data: paxRecord("comment", "note"),
      })));
      assert.throws(() => verifyPackedTarball(writeTarball(root, tar)), /local extension header/u);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("tar preflight does not strip BOM bytes or regular-file trailing slashes", () => {
  for (const [name, message] of [
    ["\ufeffpackage/README.md", /archive contains an unexpected path/u],
    ["package/README.md/", /regular file path ends with a slash/u],
  ]) {
    const root = mkdtempSync(join(tmpdir(), "neo-tar-path-alias-test-"));
    try {
      const tar = buildTar([{ name, data: Buffer.from("readme") }]);
      assert.throws(() => verifyPackedTarball(writeTarball(root, tar)), message);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("tar preflight requires a NUL-terminated GNU long name", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-tar-long-name-test-"));
  try {
    const tar = buildTar([{
      name: "././@LongLink", type: "L", data: Buffer.from("package/README.md\n"),
    }]);
    assert.throws(
      () => verifyPackedTarball(writeTarball(root, tar)),
      /GNU long member name is not NUL-terminated/u,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("release package verifier accepts bounded PAX and GNU long-name headers", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-decompiler-extension-test-"));
  try {
    const archive = writeTarball(root, buildExtendedHeaderPackage());
    const result = verifyPackedTarball(archive);
    assert.equal(result.files.length, PACKAGE_FILES.length + 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

for (const [type, description] of [
  ["1", "hard link"],
  ["2", "symbolic link"],
]) {
  test(`release package verifier rejects a ${description} before extraction`, () => {
    const root = mkdtempSync(join(tmpdir(), "neo-decompiler-link-test-"));
    try {
      const archive = writeTarball(
        root,
        buildTar([
          {
            name: "package/dist/index.js",
            type,
            linkTarget: "package/README.md",
          },
        ]),
      );
      assert.throws(
        () => verifyPackedTarball(archive),
        new RegExp(`archive contains a ${description}: package/dist/index\\.js`),
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}

test("release package verifier rejects an oversized declared member", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-decompiler-size-test-"));
  try {
    const oversized = Buffer.alloc(2 * 1024 * 1024 + 1);
    const archive = writeTarball(
      root,
      buildTar([{ name: "package/dist/index.js", data: oversized }]),
    );
    assert.throws(
      () => verifyPackedTarball(archive),
      /package\/dist\/index\.js declares 2097153 bytes; maximum is 2097152/u,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("release package verifier rejects an oversized cumulative declaration", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-decompiler-total-size-test-"));
  try {
    const mib = 1024 * 1024;
    const archive = writeTarball(
      root,
      buildTar([
        { name: "package/dist/pkg/neo_decompiler_bg.wasm", data: Buffer.alloc(8 * mib) },
        { name: "package/dist/index.js", data: Buffer.alloc(2 * mib) },
        { name: "package/dist/index.d.ts", data: Buffer.alloc(2 * mib) },
        { name: "package/dist/pkg/neo_decompiler.js", data: Buffer.alloc(2 * mib) },
        { name: "package/dist/pkg/neo_decompiler.d.ts", data: Buffer.alloc(2 * mib) },
        { name: "package/dist/pkg/neo_decompiler_bg.wasm.d.ts", data: Buffer.alloc(1) },
      ]),
    );
    assert.throws(
      () => verifyPackedTarball(archive),
      /package declares 16777217 unpacked bytes; maximum is 16777216/u,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("release package verifier keeps duplicate and traversal paths fail closed", () => {
  for (const [entries, message] of [
    [
      [
        { name: "package/README.md", data: Buffer.from("one") },
        { name: "package/README.md", data: Buffer.from("two") },
      ],
      /archive contains a duplicate path: package\/README\.md/u,
    ],
    [
      [{ name: "../package/README.md", data: Buffer.from("escape") }],
      /archive contains a parent-directory path/u,
    ],
  ]) {
    const root = mkdtempSync(join(tmpdir(), "neo-decompiler-path-test-"));
    try {
      assert.throws(() => verifyPackedTarball(writeTarball(root, buildTar(entries))), message);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("release package verifier bounds highly compressed tar streams", () => {
  const root = mkdtempSync(join(tmpdir(), "neo-decompiler-compression-test-"));
  try {
    const archive = writeTarball(root, Buffer.alloc(20 * 1024 * 1024));
    assert.ok(
      lstatSync(archive).size < 128 * 1024,
      "test fixture should have a high compression ratio",
    );
    assert.throws(
      () => verifyPackedTarball(archive),
      /archive expands beyond the \d+-byte tar stream limit/u,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
