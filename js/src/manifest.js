export function parseManifest(json) {
  const value = typeof json === "string" ? JSON.parse(json) : json;
  return {
    name: value.name,
    groups: Array.isArray(value.groups) ? value.groups : [],
    supportedStandards: Array.isArray(value.supportedstandards)
      ? value.supportedstandards
      : [],
    features: {
      storage: Boolean(value.features?.storage),
      payable: Boolean(value.features?.payable),
    },
    abi: {
      methods: Array.isArray(value.abi?.methods)
        ? value.abi.methods.map((method) => ({
            name: method.name,
            parameters: Array.isArray(method.parameters)
              ? method.parameters.map((parameter) => ({
                  name: parameter.name,
                  kind: parameter.type ?? parameter.kind ?? "Any",
                }))
              : [],
            returnType: method.returntype ?? "Void",
            offset:
              typeof method.offset === "number" && method.offset >= 0
                ? method.offset
                : null,
            safe: Boolean(method.safe),
          }))
        : [],
      events: Array.isArray(value.abi?.events)
        ? value.abi.events.map((event) => ({
            name: event.name,
            parameters: Array.isArray(event.parameters)
              ? event.parameters.map((parameter) => ({
                  name: parameter.name,
                  kind: parameter.type ?? parameter.kind ?? "Any",
                }))
              : [],
          }))
        : [],
    },
    permissions: Array.isArray(value.permissions) ? value.permissions : [],
    trusts:
      value.trusts === undefined
        ? null
        : Array.isArray(value.trusts)
          ? value.trusts
          : value.trusts,
    extra: value.extra ?? null,
  };
}

export function sanitizeIdentifier(input) {
  let ident = "";
  for (const character of input) {
    if (/[A-Za-z0-9]/u.test(character)) {
      ident += character;
    } else if (
      (character === "_" || /\s|-/u.test(character)) &&
      !ident.endsWith("_")
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
