export function sanitizeIdentifier(input) {
  let ident = "";
  for (const character of input) {
    if (/[A-Za-z0-9]/u.test(character)) {
      ident += character;
    } else if (
      character === "_" ||
      (/\s|-/u.test(character) && !ident.endsWith("_"))
    ) {
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
