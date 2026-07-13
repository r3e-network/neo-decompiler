import { ManifestParseError } from "./errors.js";

export {
  extractContractName,
  formatManifestParameters,
  formatManifestType,
  makeUniqueIdentifier,
  sanitizeIdentifier,
  sanitizeParameterNames,
} from "./manifest-format.js";

const MAX_MANIFEST_SIZE = 0xffff;

export function parseManifest(json, options = {}) {
  let value;
  if (typeof json === "string") {
    const size = Buffer.byteLength(json, "utf8");
    if (size > MAX_MANIFEST_SIZE) {
      throw new ManifestParseError(
        `manifest size ${size} exceeds maximum (${MAX_MANIFEST_SIZE} bytes)`,
        { code: "FileTooLarge", size, max: MAX_MANIFEST_SIZE },
      );
    }
    try {
      value = JSON.parse(json);
    } catch (cause) {
      throw new ManifestParseError(
        `invalid manifest JSON: ${cause.message}`,
        { code: "InvalidJson" },
      );
    }
  } else {
    value = json;
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new ManifestParseError(
      "manifest must be a JSON object",
      { code: "InvalidStructure" },
    );
  }
  requireString(value.name, "name");

  if (
    value.abi === null ||
    value.abi === undefined ||
    typeof value.abi !== "object" ||
    Array.isArray(value.abi)
  ) {
    throw new ManifestParseError("abi is required and must be an object", {
      code: "MissingField",
      path: "abi",
    });
  }

  if (value.supportedstandards !== undefined && !Array.isArray(value.supportedstandards)) {
    throw new ManifestParseError("supportedstandards must be an array", {
      code: "InvalidType",
      path: "supportedstandards",
    });
  }
  // Rust deserializes this field as `Vec<String>`, so serde rejects any
  // non-string element. Mirror that — without this check a number/object
  // element would be carried verbatim into the parsed manifest, diverging
  // from the authoritative Rust parser's reject decision.
  if (
    Array.isArray(value.supportedstandards) &&
    !value.supportedstandards.every((entry) => typeof entry === "string")
  ) {
    throw new ManifestParseError("supportedstandards must be an array of strings", {
      code: "InvalidType",
      path: "supportedstandards",
    });
  }

  const features = parseFeatures(value.features);

  requireArrayIfPresent(value.groups, "groups");
  requireArrayIfPresent(value.abi.methods, "abi.methods");
  requireArrayIfPresent(value.abi.events, "abi.events");
  requireArrayIfPresent(value.permissions, "permissions");

  const manifest = {
    name: value.name,
    groups: Array.isArray(value.groups)
      ? value.groups.map((group, groupIndex) => parseGroup(group, groupIndex))
      : [],
    supportedStandards: Array.isArray(value.supportedstandards)
      ? value.supportedstandards
      : [],
    features,
    abi: {
      methods: Array.isArray(value.abi.methods)
        ? value.abi.methods.map((method, methodIndex) =>
            parseAbiMethod(method, methodIndex),
          )
        : [],
      events: Array.isArray(value.abi.events)
        ? value.abi.events.map((event, eventIndex) =>
            parseAbiEvent(event, eventIndex),
          )
        : [],
    },
    permissions: Array.isArray(value.permissions)
      ? value.permissions.map((perm, permIndex) => parsePermission(perm, permIndex))
      : [],
    trusts:
      value.trusts === undefined
        ? null
        : Array.isArray(value.trusts)
          ? value.trusts
          : value.trusts,
    extra: value.extra ?? null,
  };

  if (options.strict) {
    validateManifestStrict(manifest);
  }

  return manifest;
}

function requireString(value, path) {
  if (typeof value !== "string") {
    throw new ManifestParseError(`${path} is required`, {
      code: "MissingField",
      path,
    });
  }
}

function requireArrayIfPresent(value, path) {
  if (value !== undefined && value !== null && !Array.isArray(value)) {
    throw new ManifestParseError(`${path} must be an array`, {
      code: "InvalidType",
      path,
    });
  }
}

function parseAbiParameter(parameter, path) {
  requireString(parameter?.name, `${path}.name`);
  requireString(parameter?.type, `${path}.type`);
  return { name: parameter.name, kind: parameter.type };
}

function parseFeatures(features) {
  if (features === undefined) {
    return {};
  }
  if (features === null || typeof features !== "object" || Array.isArray(features)) {
    throw new ManifestParseError("features must be an object", {
      code: "InvalidType",
      path: "features",
    });
  }
  // Neo N3's `ContractManifest.FromJson` requires `features` to be an
  // empty object (the legacy 2.x storage/payable flags do not exist in
  // N3). Tolerant parsing keeps the raw object so malformed manifests
  // stay inspectable; strict parsing rejects non-empty content.
  return { ...features };
}

/**
 * Classify a permission `contract` descriptor by shape, mirroring the
 * official `ContractPermissionDescriptor.FromJson()` (and the Rust
 * port's `ManifestPermissionContract::classify`):
 *
 * - `"*"` — wildcard,
 * - 42 characters, `0x` prefix + 40 hex digits — contract hash,
 * - 66 hex characters — group public key,
 * - anything else (including the non-official `{hash}`/`{group}`
 *   object forms) — `other`, a malformed descriptor the official
 *   parser rejects.
 */
export function classifyPermissionContract(value) {
  if (typeof value === "string") {
    if (value === "*") {
      return { kind: "wildcard", value };
    }
    if (isHashDescriptor(value)) {
      return { kind: "hash", hash: value };
    }
    if (isGroupDescriptor(value)) {
      return { kind: "group", group: value };
    }
  }
  return { kind: "other", value };
}

const HEX_DIGITS = /^[0-9a-fA-F]+$/u;

function isHashDescriptor(text) {
  return (
    text.length === 42 &&
    (text.startsWith("0x") || text.startsWith("0X")) &&
    HEX_DIGITS.test(text.slice(2))
  );
}

function isGroupDescriptor(text) {
  return text.length === 66 && HEX_DIGITS.test(text);
}

function parsePermission(perm, permIndex) {
  const path = `permissions[${permIndex}]`;
  if (perm === null || typeof perm !== "object" || Array.isArray(perm)) {
    throw new ManifestParseError(`${path} must be an object`, {
      code: "InvalidType",
      path,
    });
  }
  if (perm.contract === undefined) {
    throw new ManifestParseError(`${path}.contract is required`, {
      code: "MissingField",
      path: `${path}.contract`,
    });
  }
  // Mirror the Rust untagged enum `ManifestPermissionMethods`
  // (`Wildcard(String) | Methods(Vec<String>)`): when present, `methods` must
  // be a string or an array of strings. `undefined` stays valid (serde default).
  if (perm.methods !== undefined) {
    const validMethods =
      typeof perm.methods === "string" ||
      (Array.isArray(perm.methods) &&
        perm.methods.every((method) => typeof method === "string"));
    if (!validMethods) {
      throw new ManifestParseError(
        `${path}.methods must be a wildcard string or an array of strings`,
        { code: "InvalidType", path: `${path}.methods` },
      );
    }
  }
  return perm;
}

function parseGroup(group, groupIndex) {
  const path = `groups[${groupIndex}]`;
  requireString(group?.pubkey, `${path}.pubkey`);
  requireString(group?.signature, `${path}.signature`);
  return { pubkey: group.pubkey, signature: group.signature };
}

function parseAbiMethod(method, methodIndex) {
  const path = `abi.methods[${methodIndex}]`;
  requireString(method?.name, `${path}.name`);
  requireString(method?.returntype, `${path}.returntype`);
  requireArrayIfPresent(method.parameters, `${path}.parameters`);
  if (method.offset !== undefined && method.offset !== null) {
    // Rust deserializes offset as Option<i32>: serde rejects non-integers and
    // out-of-range values. Mirror that here (JSON.parse cannot distinguish the
    // `3.0` float-syntax that Rust also rejects, but every other case matches)
    // so a crafted manifest is rejected consistently across both ports.
    if (
      typeof method.offset !== "number" ||
      !Number.isInteger(method.offset) ||
      method.offset < -2147483648 ||
      method.offset > 2147483647
    ) {
      throw new ManifestParseError(
        `${path}.offset must be an i32 integer`,
        { code: "InvalidType", path: `${path}.offset` },
      );
    }
  }
  if (method.safe !== undefined && typeof method.safe !== "boolean") {
    throw new ManifestParseError(`${path}.safe must be a boolean`, {
      code: "InvalidType",
      path: `${path}.safe`,
    });
  }
  return {
    name: method.name,
    parameters: Array.isArray(method.parameters)
      ? method.parameters.map((parameter, parameterIndex) =>
          parseAbiParameter(parameter, `${path}.parameters[${parameterIndex}]`),
        )
      : [],
    returnType: method.returntype,
    offset:
      typeof method.offset === "number" && method.offset >= 0
        ? method.offset
        : null,
    safe: method.safe === true,
  };
}

function parseAbiEvent(event, eventIndex) {
  const path = `abi.events[${eventIndex}]`;
  requireString(event?.name, `${path}.name`);
  requireArrayIfPresent(event.parameters, `${path}.parameters`);
  return {
    name: event.name,
    parameters: Array.isArray(event.parameters)
      ? event.parameters.map((parameter, parameterIndex) =>
          parseAbiParameter(parameter, `${path}.parameters[${parameterIndex}]`),
        )
      : [],
  };
}

function validateManifestStrict(manifest) {
  if (Object.keys(manifest.features).length > 0) {
    throw new ManifestParseError(
      `features must be an empty object in Neo N3, got keys: ${Object.keys(manifest.features).join(", ")}`,
      { code: "Validation", path: "features", value: manifest.features },
    );
  }

  for (let i = 0; i < manifest.permissions.length; i++) {
    const perm = manifest.permissions[i];
    if (perm && typeof perm === "object" && !Array.isArray(perm)) {
      if (classifyPermissionContract(perm.contract).kind === "other") {
        throw new ManifestParseError(
          `permissions[${i}].contract must be "*", a 0x-prefixed 20-byte contract hash, or a 33-byte group public key, got ${JSON.stringify(perm.contract)}`,
          { code: "Validation", path: `permissions[${i}].contract`, value: perm.contract },
        );
      }
      if (typeof perm.methods === "string" && perm.methods !== "*") {
        throw new ManifestParseError(
          `permissions[${i}].methods wildcard must be "*", got ${JSON.stringify(perm.methods)}`,
          { code: "Validation", path: `permissions[${i}].methods`, value: perm.methods },
        );
      }
    }
  }
  if (typeof manifest.trusts === "string" && manifest.trusts !== "*") {
    throw new ManifestParseError(
      `trusts wildcard must be "*", got ${JSON.stringify(manifest.trusts)}`,
      { code: "Validation", path: "trusts", value: manifest.trusts },
    );
  }
}
