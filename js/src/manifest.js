import { ManifestParseError } from "./errors.js";

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
    return { storage: false, payable: false };
  }
  if (features === null || typeof features !== "object" || Array.isArray(features)) {
    throw new ManifestParseError("features must be an object", {
      code: "InvalidType",
      path: "features",
    });
  }
  if (features.storage !== undefined && typeof features.storage !== "boolean") {
    throw new ManifestParseError("features.storage must be a boolean", {
      code: "InvalidType",
      path: "features.storage",
    });
  }
  if (features.payable !== undefined && typeof features.payable !== "boolean") {
    throw new ManifestParseError("features.payable must be a boolean", {
      code: "InvalidType",
      path: "features.payable",
    });
  }
  return {
    storage: features.storage === true,
    payable: features.payable === true,
  };
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
  if (
    method.offset !== undefined &&
    method.offset !== null &&
    typeof method.offset !== "number"
  ) {
    throw new ManifestParseError(`${path}.offset must be a number`, {
      code: "InvalidType",
      path: `${path}.offset`,
    });
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
  for (let i = 0; i < manifest.permissions.length; i++) {
    const perm = manifest.permissions[i];
    if (perm && typeof perm === "object" && !Array.isArray(perm)) {
      if (typeof perm.contract === "string" && perm.contract !== "*") {
        throw new ManifestParseError(
          `permissions[${i}].contract wildcard must be "*", got ${JSON.stringify(perm.contract)}`,
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

export function sanitizeIdentifier(input) {
  let ident = "";
  for (const character of input) {
    if (/[A-Za-z0-9]/u.test(character)) {
      ident += character;
    } else if (
      character === "_" ||
      (/\s|-/u.test(character) && !ident.endsWith("_"))
    ) {
      // Mirror Rust precedence: explicit `_` is always preserved
      // (so `__foo` stays `__foo`); whitespace and `-` collapse into
      // a single `_` separator only when the previous char isn't
      // already `_`. Earlier the parens were `(_ || whitespace) &&
      // !ends_with("_")`, which silently collapsed leading double
      // underscores and broke parity with Rust's `sanitize_identifier`.
      ident += "_";
    }
  }

  ident = ident.replace(/_+$/u, "");
  if (ident.length === 0) {
    ident = "param";
  }
  if (/^[0-9]/u.test(ident)) {
    ident = `_${ident}`;
  }
  return ident;
}

/**
 * Extract and sanitise the contract name from a manifest, falling back
 * to `NeoContract` when the manifest is absent or the name trims to
 * empty. Mirrors Rust's `decompiler::helpers::extract_contract_name`
 * so manifest-less and empty-name outputs are byte-identical across
 * ports. Returns the sanitised identifier or `"NeoContract"`.
 */
export function extractContractName(manifest) {
  const trimmed = manifest?.name?.trim();
  if (!trimmed) {
    return "NeoContract";
  }
  const sanitised = sanitizeIdentifier(trimmed);
  return sanitised !== "" ? sanitised : "NeoContract";
}

export function makeUniqueIdentifier(base, used) {
  if (!used.has(base)) {
    used.add(base);
    return base;
  }

  let index = 1;
  while (used.has(`${base}_${index}`)) {
    index += 1;
  }
  const candidate = `${base}_${index}`;
  used.add(candidate);
  return candidate;
}

export function sanitizeParameterNames(parameters) {
  const used = new Set();
  return parameters.map((parameter) =>
    makeUniqueIdentifier(sanitizeIdentifier(parameter.name), used),
  );
}

export function formatManifestType(kind) {
  const normalized = String(kind).toLowerCase();
  switch (normalized) {
    case "void":
      return "void";
    case "boolean":
      return "bool";
    case "integer":
      return "int";
    case "string":
      return "string";
    case "hash160":
      return "hash160";
    case "hash256":
      return "hash256";
    case "publickey":
      return "publickey";
    case "bytearray":
      return "bytes";
    case "signature":
      return "signature";
    case "array":
      return "array";
    case "map":
      return "map";
    case "interopinterface":
      return "interop";
    case "any":
      return "any";
    default:
      return String(kind);
  }
}

export function formatManifestParameters(parameters) {
  const names = sanitizeParameterNames(parameters);
  return parameters
    .map((parameter, index) => `${names[index]}: ${formatManifestType(parameter.kind)}`)
    .join(", ");
}
