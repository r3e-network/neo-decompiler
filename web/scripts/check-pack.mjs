import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstatSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

const MAX_ARCHIVE_BYTES = 12 * 1024 * 1024;
const MAX_UNPACKED_BYTES = 16 * 1024 * 1024;
const MAX_DEFAULT_FILE_BYTES = 2 * 1024 * 1024;
const MAX_WASM_BYTES = 8 * 1024 * 1024;
const TAR_BLOCK_BYTES = 512;
const MAX_TAR_STREAM_BYTES = MAX_UNPACKED_BYTES + 2 * 1024 * 1024;
const MAX_EXTENSION_BYTES = 128 * 1024;
const MAX_TOTAL_EXTENSION_BYTES = 1024 * 1024;
const MAX_TAR_HEADERS = 128;
// Preserve a leading BOM so it cannot turn a different archive path into an
// allowlisted ASCII path while an external extractor retains those bytes.
const UTF8_DECODER = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

const EXPECTED_FILE_LIMITS = new Map([
  ["package/README.md", 512 * 1024],
  ["package/package.json", 128 * 1024],
  ["package/dist/index.js", MAX_DEFAULT_FILE_BYTES],
  ["package/dist/index.d.ts", MAX_DEFAULT_FILE_BYTES],
  ["package/dist/pkg/neo_decompiler.js", MAX_DEFAULT_FILE_BYTES],
  ["package/dist/pkg/neo_decompiler.d.ts", MAX_DEFAULT_FILE_BYTES],
  ["package/dist/pkg/neo_decompiler_bg.wasm", MAX_WASM_BYTES],
  ["package/dist/pkg/neo_decompiler_bg.wasm.d.ts", MAX_DEFAULT_FILE_BYTES],
  ["package/dist/pkg/LICENSE-MIT", 256 * 1024],
  ["package/dist/pkg/LICENSE-APACHE", 256 * 1024],
]);

const EXPECTED_DIRECTORIES = new Set([
  "package",
  "package/dist",
  "package/dist/pkg",
]);

function normalizeArchiveEntry(entry) {
  return entry.replace(/^\.\//, "").replace(/\/$/, "");
}

function assertSafeEntry(entry) {
  assert.ok(entry.length > 0, "archive contains an empty path");
  assert.ok(!entry.includes("\\"), `archive path uses a backslash separator: ${entry}`);
  assert.ok(!entry.startsWith("/"), `archive contains an absolute path: ${entry}`);
  assert.ok(!/^[A-Za-z]:\//u.test(entry), `archive contains an absolute path: ${entry}`);
  assert.ok(!entry.includes("\0"), "archive path contains a NUL byte");
  assert.ok(
    !/[\u0000-\u001f\u007f-\u009f]/u.test(entry),
    "archive path contains a control character",
  );
  assert.ok(!entry.split("/").includes(""), `archive contains an empty path component: ${entry}`);
  assert.ok(!entry.split("/").includes("."), `archive contains a dot path component: ${entry}`);
  assert.ok(
    !entry.split("/").includes(".."),
    `archive contains a parent-directory path: ${entry}`,
  );
}

function decodeUtf8(bytes, label) {
  try {
    return UTF8_DECODER.decode(bytes);
  } catch (error) {
    throw new Error(`${label} is not valid UTF-8`, { cause: error });
  }
}

function decodeTarString(field, label) {
  const nul = field.indexOf(0);
  const end = nul === -1 ? field.length : nul;
  if (nul !== -1) {
    assert.ok(
      field.subarray(nul).every((byte) => byte === 0),
      `${label} contains bytes after its NUL terminator`,
    );
  }
  return decodeUtf8(field.subarray(0, end), label);
}

function parseTarNumber(field, label) {
  if ((field[0] & 0x80) !== 0) {
    assert.equal(field[0] & 0x40, 0, `${label} must not be negative`);
    let value = BigInt(field[0] & 0x7f);
    for (const byte of field.subarray(1)) {
      value = (value << 8n) | BigInt(byte);
    }
    return value;
  }

  const nul = field.indexOf(0);
  const end = nul === -1 ? field.length : nul;
  if (nul !== -1) {
    assert.ok(
      field.subarray(nul).every((byte) => byte === 0 || byte === 0x20),
      `${label} contains bytes after its NUL terminator`,
    );
  }
  // Buffer's "ascii" decoder clears the high bit. Using it here could turn
  // non-ASCII bytes into valid octal digits that an extractor reads differently.
  const encoded = field.subarray(0, end).toString("latin1").replace(/^ +| +$/gu, "");
  if (encoded.length === 0) {
    return 0n;
  }
  assert.match(encoded, /^[0-7]+$/u, `${label} is not a valid tar number`);
  return BigInt(`0o${encoded}`);
}

function parsePaxSize(value) {
  assert.match(value, /^(?:0|[1-9][0-9]*)$/u, "PAX size is not a non-negative integer");
  assert.ok(value.length <= 20, "PAX size is too large");
  return BigInt(value);
}

const SAFE_PAX_KEYS = new Set([
  "path",
  "linkpath",
  "size",
  "mode",
  "uid",
  "gid",
  "uname",
  "gname",
  "mtime",
  "atime",
  "ctime",
  "birthtime",
  "dev",
  "ino",
  "nlink",
  "comment",
  "charset",
  "LIBARCHIVE.creationtime",
]);

function parsePaxRecords(payload) {
  const records = new Map();
  let offset = 0;
  while (offset < payload.length) {
    const space = payload.indexOf(0x20, offset);
    assert.ok(space > offset, "PAX record has no length delimiter");
    const encodedLength = payload.subarray(offset, space).toString("latin1");
    assert.match(encodedLength, /^[1-9][0-9]*$/u, "PAX record has an invalid length");
    assert.ok(encodedLength.length <= 10, "PAX record length is too large");
    const recordLength = Number(encodedLength);
    const recordEnd = offset + recordLength;
    assert.ok(recordEnd <= payload.length, "PAX record exceeds its header payload");
    assert.equal(payload[recordEnd - 1], 0x0a, "PAX record is not newline-terminated");

    const body = payload.subarray(space + 1, recordEnd - 1);
    const equals = body.indexOf(0x3d);
    assert.ok(equals > 0, "PAX record has no key/value delimiter");
    const key = decodeUtf8(body.subarray(0, equals), "PAX keyword");
    assert.match(key, /^[A-Za-z0-9_.-]+$/u, "PAX record has an invalid keyword");
    assert.ok(SAFE_PAX_KEYS.has(key), `archive contains an unsupported PAX keyword: ${key}`);
    assert.ok(!records.has(key), `PAX header contains a duplicate keyword: ${key}`);
    records.set(key, decodeUtf8(body.subarray(equals + 1), `PAX ${key}`));
    offset = recordEnd;
  }
  return records;
}

function decodeLongName(payload, label) {
  const nul = payload.indexOf(0);
  assert.ok(nul !== -1, `${label} is not NUL-terminated`);
  assert.ok(
    payload.subarray(nul).every((byte) => byte === 0),
    `${label} contains bytes after its NUL terminator`,
  );
  return decodeUtf8(payload.subarray(0, nul), label);
}

function tarHeaderChecksum(header) {
  let checksum = 0;
  for (let index = 0; index < header.length; index += 1) {
    checksum += index >= 148 && index < 156 ? 0x20 : header[index];
  }
  return BigInt(checksum);
}

function isZeroBlock(block) {
  return block.every((byte) => byte === 0);
}

function paddedTarSize(size) {
  return Math.ceil(size / TAR_BLOCK_BYTES) * TAR_BLOCK_BYTES;
}

function parseTarHeader(header, headerIndex) {
  // npm emits POSIX ustar headers. In the old GNU dialect, bytes 345..500
  // have a different meaning from the ustar path prefix. Reject that dialect
  // instead of deriving a safe path that the extractor would not use.
  assert.ok(
    header.subarray(257, 265).equals(Buffer.from("ustar\0" + "00", "ascii")),
    `tar header ${headerIndex} is not POSIX ustar`,
  );
  const expectedChecksum = parseTarNumber(header.subarray(148, 156), "tar header checksum");
  assert.equal(
    expectedChecksum,
    tarHeaderChecksum(header),
    `tar header ${headerIndex} has an invalid checksum`,
  );

  const name = decodeTarString(header.subarray(0, 100), "tar member name");
  const prefix = decodeTarString(header.subarray(345, 500), "tar member prefix");
  const path = prefix.length > 0 ? `${prefix}/${name}` : name;
  const linkTarget = decodeTarString(header.subarray(157, 257), "tar link target");
  const typeByte = header[156];
  const type = typeByte === 0 ? "0" : String.fromCharCode(typeByte);
  const size = parseTarNumber(header.subarray(124, 136), "tar member size");
  return { path, linkTarget, type, size };
}

function decompressTarArchive(archiveBytes) {
  try {
    return gunzipSync(archiveBytes, { maxOutputLength: MAX_TAR_STREAM_BYTES });
  } catch (error) {
    if (error?.code === "ERR_BUFFER_TOO_LARGE" || /larger than/u.test(error?.message ?? "")) {
      throw new Error(
        `archive expands beyond the ${MAX_TAR_STREAM_BYTES}-byte tar stream limit`,
        { cause: error },
      );
    }
    throw new Error("package archive is not valid gzip data", { cause: error });
  }
}

function preflightTarArchive(archiveBytes) {
  const tar = decompressTarArchive(archiveBytes);
  assert.equal(tar.length % TAR_BLOCK_BYTES, 0, "tar stream is not block-aligned");

  const members = [];
  const unique = new Set();
  let offset = 0;
  let headerCount = 0;
  let extensionBytes = 0;
  let unpackedBytes = 0;
  let zeroBlocks = 0;
  let sawEndMarker = false;
  let pendingPath = null;
  let pendingLinkTarget = null;
  let pendingSize = null;
  let pendingLocalExtension = false;

  while (offset + TAR_BLOCK_BYTES <= tar.length) {
    const header = tar.subarray(offset, offset + TAR_BLOCK_BYTES);
    offset += TAR_BLOCK_BYTES;
    if (isZeroBlock(header)) {
      zeroBlocks += 1;
      if (zeroBlocks === 2) {
        sawEndMarker = true;
        assert.ok(
          tar.subarray(offset).every((byte) => byte === 0),
          "tar stream contains data after its end marker",
        );
        break;
      }
      continue;
    }
    assert.equal(zeroBlocks, 0, "tar stream contains an isolated zero block");

    headerCount += 1;
    assert.ok(
      headerCount <= MAX_TAR_HEADERS,
      `tar archive has more than ${MAX_TAR_HEADERS} headers`,
    );
    const parsed = parseTarHeader(header, headerCount);

    if (["x", "g", "L", "K"].includes(parsed.type)) {
      // Keep the accepted dialect unambiguous between this parser and the
      // external extractor. Local PAX/GNU extensions must describe the next
      // ordinary member directly rather than being stacked onto another
      // extension header with implementation-dependent precedence.
      assert.ok(
        !pendingLocalExtension,
        "tar archive contains stacked local extension headers",
      );
      assert.ok(parsed.size <= BigInt(MAX_EXTENSION_BYTES), "tar extension header is too large");
      const extensionSize = Number(parsed.size);
      extensionBytes += extensionSize;
      assert.ok(
        extensionBytes <= MAX_TOTAL_EXTENSION_BYTES,
        "tar extension headers exceed the metadata limit",
      );
      assert.ok(
        offset + paddedTarSize(extensionSize) <= tar.length,
        "tar extension payload extends past the archive",
      );
      const payload = tar.subarray(offset, offset + extensionSize);
      offset += paddedTarSize(extensionSize);
      pendingLocalExtension = parsed.type !== "g";

      if (parsed.type === "x" || parsed.type === "g") {
        const pax = parsePaxRecords(payload);
        if (parsed.type === "g") {
          for (const forbidden of ["path", "linkpath", "size"]) {
            assert.ok(!pax.has(forbidden), `global PAX header must not set ${forbidden}`);
          }
        } else {
          if (pax.has("path")) {
            pendingPath = pax.get("path");
          }
          if (pax.has("linkpath")) {
            pendingLinkTarget = pax.get("linkpath");
          }
          if (pax.has("size")) {
            pendingSize = parsePaxSize(pax.get("size"));
          }
        }
      } else if (parsed.type === "L") {
        pendingPath = decodeLongName(payload, "GNU long member name");
      } else {
        pendingLinkTarget = decodeLongName(payload, "GNU long link target");
      }
      continue;
    }

    const rawPath = pendingPath ?? parsed.path;
    const path = normalizeArchiveEntry(rawPath);
    const linkTarget = pendingLinkTarget ?? parsed.linkTarget;
    const size = pendingSize ?? parsed.size;
    if (pendingSize !== null) {
      assert.equal(pendingSize, parsed.size, `PAX size disagrees with tar header for ${path}`);
    }
    pendingPath = null;
    pendingLinkTarget = null;
    pendingSize = null;
    pendingLocalExtension = false;

    assertSafeEntry(path);
    assert.ok(!unique.has(path), `archive contains a duplicate path: ${path}`);
    unique.add(path);

    if (parsed.type === "1") {
      assert.fail(`archive contains a hard link: ${path} -> ${linkTarget}`);
    }
    if (parsed.type === "2") {
      assert.fail(`archive contains a symbolic link: ${path} -> ${linkTarget}`);
    }
    assert.ok(
      parsed.type === "0" || parsed.type === "5",
      `archive contains an unsupported member type ${JSON.stringify(parsed.type)}: ${path}`,
    );
    assert.equal(linkTarget, "", `archive member has an unexpected link target: ${path}`);

    if (parsed.type === "5") {
      assert.ok(
        EXPECTED_DIRECTORIES.has(path),
        `archive contains an unexpected directory: ${path}`,
      );
      assert.equal(size, 0n, `archive directory has a non-zero declared size: ${path}`);
    } else {
      assert.ok(!rawPath.endsWith("/"), `regular file path ends with a slash: ${path}`);
      const limit = EXPECTED_FILE_LIMITS.get(path);
      assert.ok(limit !== undefined, `archive contains an unexpected path: ${path}`);
      assert.ok(size > 0n, `packaged file is empty: ${path}`);
      assert.ok(
        size <= BigInt(limit),
        `packaged file ${path} declares ${size} bytes; maximum is ${limit}`,
      );
      unpackedBytes += Number(size);
      assert.ok(
        unpackedBytes <= MAX_UNPACKED_BYTES,
        `package declares ${unpackedBytes} unpacked bytes; maximum is ${MAX_UNPACKED_BYTES}`,
      );
      members.push({ path, size: Number(size) });
    }

    const contentSize = Number(size);
    assert.ok(
      offset + paddedTarSize(contentSize) <= tar.length,
      `tar member payload extends past the archive: ${path}`,
    );
    offset += paddedTarSize(contentSize);
  }

  assert.ok(sawEndMarker, "tar stream is missing its two-block end marker");
  assert.ok(!pendingLocalExtension, "tar stream ends after a local extension header");
  assert.equal(pendingPath, null, "tar stream ends after a name extension header");
  assert.equal(pendingLinkTarget, null, "tar stream ends after a link extension header");
  assert.equal(pendingSize, null, "tar stream ends after a PAX size header");
  for (const required of EXPECTED_FILE_LIMITS.keys()) {
    assert.ok(unique.has(required), `missing packaged file: ${required}`);
  }
  assert.equal(
    members.length,
    EXPECTED_FILE_LIMITS.size,
    "archive file allowlist does not match exactly",
  );

  return { files: members, unpackedBytes };
}

function walkExtractedTree(root, current = root, entries = []) {
  for (const name of readdirSync(current)) {
    const path = join(current, name);
    const metadata = lstatSync(path);
    const archivePath = relative(root, path).replaceAll("\\", "/");
    assert.ok(!metadata.isSymbolicLink(), `archive contains a symbolic link: ${archivePath}`);
    if (metadata.isDirectory()) {
      assert.ok(
        EXPECTED_DIRECTORIES.has(archivePath),
        `archive contains an unexpected directory: ${archivePath}`,
      );
      walkExtractedTree(root, path, entries);
    } else {
      assert.ok(metadata.isFile(), `archive contains a non-regular file: ${archivePath}`);
      entries.push({ path: archivePath, size: metadata.size });
    }
  }
  return entries;
}

export function verifyPackedTarball(inputPath) {
  const archivePath = resolve(inputPath);
  const archiveMetadata = lstatSync(archivePath);
  assert.ok(archiveMetadata.isFile(), "package archive must be a regular file");
  assert.ok(
    archiveMetadata.size <= MAX_ARCHIVE_BYTES,
    `package archive is ${archiveMetadata.size} bytes; maximum is ${MAX_ARCHIVE_BYTES}`,
  );

  const archiveBytes = readFileSync(archivePath);
  const preflight = preflightTarArchive(archiveBytes);
  const declaredFiles = new Map(preflight.files.map((entry) => [entry.path, entry.size]));

  const temporaryRoot = mkdtempSync(join(tmpdir(), "neo-decompiler-pack-"));
  try {
    // Extract the exact bytes that passed preflight, avoiding a path-swap race
    // between validation and the external tar invocation.
    execFileSync("tar", ["-xzf", "-", "-C", temporaryRoot], {
      input: archiveBytes,
      stdio: "pipe",
      maxBuffer: 1024 * 1024,
    });

    const extractedRoot = realpathSync(temporaryRoot);
    const files = walkExtractedTree(extractedRoot).sort((left, right) =>
      left.path.localeCompare(right.path),
    );
    const actualPaths = new Set(files.map((entry) => entry.path));

    for (const [required, limit] of EXPECTED_FILE_LIMITS) {
      assert.ok(actualPaths.has(required), `missing packaged file: ${required}`);
      const entry = files.find((candidate) => candidate.path === required);
      assert.ok(entry.size > 0, `packaged file is empty: ${required}`);
      assert.ok(
        entry.size <= limit,
        `packaged file ${required} is ${entry.size} bytes; maximum is ${limit}`,
      );
      assert.equal(
        entry.size,
        declaredFiles.get(required),
        `extracted size disagrees with the tar header: ${required}`,
      );
    }
    assert.equal(
      actualPaths.size,
      EXPECTED_FILE_LIMITS.size,
      "archive file allowlist does not match exactly",
    );

    const unpackedBytes = files.reduce((total, entry) => total + entry.size, 0);
    assert.equal(
      unpackedBytes,
      preflight.unpackedBytes,
      "extracted size total changed after preflight",
    );
    assert.ok(
      unpackedBytes <= MAX_UNPACKED_BYTES,
      `unpacked package is ${unpackedBytes} bytes; maximum is ${MAX_UNPACKED_BYTES}`,
    );

    const packageJsonPath = join(extractedRoot, "package", "package.json");
    const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
    assert.equal(packageJson.name, "neo-decompiler-web", "unexpected npm package name");
    assert.match(
      packageJson.version,
      /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/,
      "npm package version is not valid semver",
    );

    const expectedFilename = `neo-decompiler-web-${packageJson.version}.tgz`;
    assert.equal(basename(archivePath), expectedFilename, "archive filename/version mismatch");

    return {
      filename: basename(archivePath),
      sha256: createHash("sha256").update(archiveBytes).digest("hex"),
      archive_bytes: archiveMetadata.size,
      unpacked_bytes: unpackedBytes,
      files,
    };
  } finally {
    rmSync(temporaryRoot, { recursive: true, force: true });
  }
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === fileURLToPath(import.meta.url)) {
  assert.equal(process.argv.length, 3, "usage: node scripts/check-pack.mjs <package.tgz>");
  const report = verifyPackedTarball(process.argv[2]);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
}
