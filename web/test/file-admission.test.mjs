import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_MANIFEST_FILE_BYTES,
  MAX_NEF_FILE_BYTES,
  readAdmittedFiles,
} from "../file-admission.js";

function fakeFile(size, contents = new Uint8Array([1, 2, 3])) {
  let arrayBufferReads = 0;
  let textReads = 0;
  return {
    file: {
      size,
      async arrayBuffer() {
        arrayBufferReads += 1;
        return contents.buffer;
      },
      async text() {
        textReads += 1;
        return "{}";
      },
    },
    reads() {
      return { arrayBufferReads, textReads };
    },
  };
}

test("oversized NEF is rejected before either file is read", async () => {
  const nef = fakeFile(MAX_NEF_FILE_BYTES + 1);
  const manifest = fakeFile(2);

  await assert.rejects(
    readAdmittedFiles(nef.file, manifest.file),
    /NEF file is 1048577 bytes; maximum is 1048576 bytes/,
  );
  assert.deepEqual(nef.reads(), { arrayBufferReads: 0, textReads: 0 });
  assert.deepEqual(manifest.reads(), { arrayBufferReads: 0, textReads: 0 });
});

test("oversized manifest is rejected before either file is read", async () => {
  const nef = fakeFile(3);
  const manifest = fakeFile(MAX_MANIFEST_FILE_BYTES + 1);

  await assert.rejects(
    readAdmittedFiles(nef.file, manifest.file),
    /Manifest file is 65536 bytes; maximum is 65535 bytes/,
  );
  assert.deepEqual(nef.reads(), { arrayBufferReads: 0, textReads: 0 });
  assert.deepEqual(manifest.reads(), { arrayBufferReads: 0, textReads: 0 });
});

test("admitted files are read exactly once", async () => {
  const nef = fakeFile(3);
  const manifest = fakeFile(2);

  const result = await readAdmittedFiles(nef.file, manifest.file);

  assert.deepEqual([...result.nefBytes], [1, 2, 3]);
  assert.equal(result.manifestJson, "{}");
  assert.deepEqual(nef.reads(), { arrayBufferReads: 1, textReads: 0 });
  assert.deepEqual(manifest.reads(), { arrayBufferReads: 0, textReads: 1 });
});
