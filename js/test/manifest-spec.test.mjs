// Spec-fidelity tests for manifest permission descriptors, the N3
// `features` contract, and NEF varint/limit behavior. Each case mirrors
// a Rust test (src/manifest/tests.rs, src/nef/tests/) so the two ports
// provably accept/reject the same inputs.
import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";

import {
  classifyPermissionContract,
  parseManifest,
  parseNef,
  NefParseError,
} from "../src/index.js";

// ─── Permission descriptor classification (mirrors Rust
//     classifies_official_string_permission_descriptors) ────────────────────

test("manifest-spec: classifies official string permission descriptors", () => {
  const manifest = parseManifest(
    JSON.stringify({
      name: "DescriptorShapes",
      abi: { methods: [], events: [] },
      permissions: [
        { contract: "*", methods: "*" },
        { contract: "0x0123456789abcdef0123456789abcdef01234567", methods: "*" },
        {
          contract: "03b209fd4f53a7170ea4444e0cb0a6bb6a53c2bd016926989cf85f9b0fba17a70c",
          methods: "*",
        },
        { contract: "not-a-descriptor", methods: "*" },
      ],
    }),
  );

  assert.deepEqual(classifyPermissionContract(manifest.permissions[0].contract), {
    kind: "wildcard",
    value: "*",
  });
  assert.deepEqual(classifyPermissionContract(manifest.permissions[1].contract), {
    kind: "hash",
    hash: "0x0123456789abcdef0123456789abcdef01234567",
  });
  assert.deepEqual(classifyPermissionContract(manifest.permissions[2].contract), {
    kind: "group",
    group: "03b209fd4f53a7170ea4444e0cb0a6bb6a53c2bd016926989cf85f9b0fba17a70c",
  });
  assert.equal(classifyPermissionContract(manifest.permissions[3].contract).kind, "other");
});

test("manifest-spec: descriptor classification edge shapes", () => {
  // 0X prefix (uppercase) counts as a hash descriptor, like Rust.
  assert.equal(
    classifyPermissionContract("0X0123456789ABCDEF0123456789ABCDEF01234567").kind,
    "hash",
  );
  // Wrong lengths fall through to other.
  assert.equal(classifyPermissionContract("0x0123").kind, "other");
  assert.equal(classifyPermissionContract("ab".repeat(32)).kind, "other"); // 64 hex chars
  // Non-hex content of the right length is not a group key.
  assert.equal(classifyPermissionContract("zz".repeat(33)).kind, "other");
  // Object descriptors (the non-official encoding) are malformed.
  assert.equal(classifyPermissionContract({ hash: "0x" + "12".repeat(20) }).kind, "other");
});

// ─── Strict-mode validation (mirrors Rust
//     strict_manifest_parsing_rejects_malformed_permission_descriptor /
//     ..._rejects_non_empty_features) ──────────────────────────────────────

test("manifest-spec: strict mode rejects malformed permission descriptors", () => {
  const json = JSON.stringify({
    name: "InvalidDescriptor",
    abi: { methods: [], events: [] },
    permissions: [
      { contract: { hash: "0x0123456789abcdef0123456789abcdef01234567" }, methods: "*" },
    ],
  });
  assert.throws(
    () => parseManifest(json, { strict: true }),
    (err) => err.details.code === "Validation",
  );
  assert.doesNotThrow(() => parseManifest(json));
});

test("manifest-spec: strict mode rejects non-empty features; tolerant keeps raw object", () => {
  const json = JSON.stringify({
    name: "LegacyFeatures",
    abi: { methods: [], events: [] },
    features: { storage: true },
  });
  assert.throws(
    () => parseManifest(json, { strict: true }),
    (err) => err.details.code === "Validation" && err.details.path === "features",
  );

  const manifest = parseManifest(json);
  assert.deepEqual(manifest.features, { storage: true });

  const empty = parseManifest(
    JSON.stringify({ name: "C", abi: { methods: [], events: [] }, features: {} }),
  );
  assert.deepEqual(empty.features, {});
  assert.doesNotThrow(() =>
    parseManifest(
      JSON.stringify({ name: "C", abi: { methods: [], events: [] }, features: {} }),
      { strict: true },
    ),
  );
});

// ─── NEF spec limits and varint behavior ───────────────────────────────────

function computeChecksum(payload) {
  const bytes = Uint8Array.from(payload);
  const first = createHash("sha256").update(bytes).digest();
  const second = createHash("sha256").update(first).digest();
  return second.subarray(0, 4);
}

function buildRawNef({ script = [0x40], sourceVarInt = [0x00] } = {}) {
  const data = [];
  data.push(...Buffer.from("NEF3"));
  data.push(...new Uint8Array(64)); // compiler
  data.push(...sourceVarInt); // source length varint (+ bytes if any)
  data.push(0); // reserved byte
  data.push(0); // token count
  data.push(0, 0); // reserved word
  data.push(script.length, ...script); // script varbytes (1-byte prefix)
  data.push(...computeChecksum(data));
  return new Uint8Array(data);
}

test("manifest-spec: NEF non-canonical varint (FD 05 00) parses like the reference", () => {
  // Source length 5 encoded with an overlong 2-byte prefix; the
  // reference MemoryReader.ReadVarInt accepts it, so we must too.
  const sourceBytes = Buffer.from("s.com");
  const nef = buildRawNef({ sourceVarInt: [0xfd, 0x05, 0x00, ...sourceBytes] });
  const parsed = parseNef(nef);
  assert.equal(parsed.header.source, "s.com");
});

test("manifest-spec: NEF invalid magic surfaces InvalidMagic even for invalid UTF-8", () => {
  // 0xFF leading byte is not valid UTF-8 — this used to escape as a
  // raw TypeError from TextDecoder instead of a NefParseError.
  const data = buildRawNef();
  data[0] = 0xff;
  assert.throws(() => parseNef(data), (error) => {
    assert.ok(error instanceof NefParseError);
    assert.equal(error.details.code, "InvalidMagic");
    return true;
  });
});

// ─── permissions[].methods type validation (mirrors Rust's untagged enum
//     ManifestPermissionMethods: Wildcard(String) | Methods(Vec<String>)) ────

test("manifest-spec: rejects non-string/non-array permission methods (parity with Rust)", () => {
  const base = { name: "C", abi: { methods: [], events: [] } };
  for (const bad of [5, {}, [1, 2], [true]]) {
    const json = JSON.stringify({
      ...base,
      permissions: [{ contract: "*", methods: bad }],
    });
    assert.throws(
      () => parseManifest(json),
      /methods must be a wildcard string or an array of strings/,
      `methods=${JSON.stringify(bad)} should be rejected`,
    );
  }
  // Valid shapes still parse.
  assert.doesNotThrow(() =>
    parseManifest(
      JSON.stringify({ ...base, permissions: [{ contract: "*", methods: "*" }] }),
    ),
  );
  assert.doesNotThrow(() =>
    parseManifest(
      JSON.stringify({
        ...base,
        permissions: [{ contract: "*", methods: ["transfer"] }],
      }),
    ),
  );
  // Absent methods stays valid (serde default).
  assert.doesNotThrow(() =>
    parseManifest(
      JSON.stringify({ ...base, permissions: [{ contract: "*" }] }),
    ),
  );
});
