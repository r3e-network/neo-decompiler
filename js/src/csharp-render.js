import { sanitizeIdentifier } from "./manifest.js";
import { rewriteCSharpExpression, splitCallArguments } from "./csharp-expression.js";

const TYPE_MAP = new Map([
  ["void", "void"],
  ["bool", "bool"],
  ["boolean", "bool"],
  ["int", "BigInteger"],
  ["integer", "BigInteger"],
  ["string", "string"],
  ["hash160", "UInt160"],
  ["hash256", "UInt256"],
  ["publickey", "ECPoint"],
  ["bytes", "ByteString"],
  ["bytestring", "ByteString"],
  ["bytearray", "ByteString"],
  ["signature", "ByteString"],
  ["array", "object[]"],
  ["map", "Map<object, object>"],
  ["interop", "object"],
  ["interopinterface", "object"],
  ["any", "object"],
]);

const CSHARP_KEYWORDS = new Set([
  "abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char",
  "checked", "class", "const", "continue", "decimal", "default", "delegate", "do",
  "double", "else", "enum", "event", "explicit", "extern", "false", "finally",
  "fixed", "float", "for", "foreach", "goto", "if", "implicit", "in", "int",
  "interface", "internal", "is", "lock", "long", "namespace", "new", "null",
  "object", "operator", "out", "override", "params", "private", "protected", "public",
  "readonly", "ref", "return", "sbyte", "sealed", "short", "sizeof", "stackalloc",
  "static", "string", "struct", "switch", "this", "throw", "true", "try", "typeof",
  "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "virtual", "void",
  "volatile", "while", "add", "alias", "ascending", "async", "await", "by",
  "descending", "dynamic", "equals", "file", "from", "get", "global", "group",
  "init", "into", "join", "let", "nameof", "nint", "notnull", "nuint", "on",
  "orderby", "partial", "record", "remove", "required", "scoped", "select", "set",
  "unmanaged", "value", "when", "where", "with", "yield",
]);

export function csharpIdentifier(name) {
  return CSHARP_KEYWORDS.has(name) ? `@${name}` : name;
}

export function csharpType(type) {
  return TYPE_MAP.get(String(type ?? "any").trim().toLowerCase()) ?? "object";
}

export function escapeCSharpString(value) {
  return String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r");
}

export function renderManifestAttributes(manifest) {
  if (!manifest || typeof manifest !== "object") return [];
  const lines = [];
  const standards = Array.isArray(manifest.supportedStandards)
    ? manifest.supportedStandards.filter((value) => typeof value === "string")
    : [];
  if (standards.length > 0) {
    lines.push(`[SupportedStandards(${standards.map((value) => `"${escapeCSharpString(value)}"`).join(", ")})]`);
  }
  if (manifest.extra && typeof manifest.extra === "object" && !Array.isArray(manifest.extra)) {
    for (const key of Object.keys(manifest.extra).sort()) {
      const value = manifest.extra[key];
      if (value === null || typeof value === "object") continue;
      lines.push(`[ManifestExtra("${escapeCSharpString(key)}", "${escapeCSharpString(value)}")]`);
    }
  }
  return lines;
}

function splitParameters(parameters) {
  const parts = [];
  let start = 0;
  let angleDepth = 0;
  for (let index = 0; index < parameters.length; index += 1) {
    const character = parameters[index];
    if (character === "<") angleDepth += 1;
    if (character === ">") angleDepth = Math.max(0, angleDepth - 1);
    if (character === "," && angleDepth === 0) {
      parts.push(parameters.slice(start, index).trim());
      start = index + 1;
    }
  }
  const tail = parameters.slice(start).trim();
  if (tail) parts.push(tail);
  return parts;
}

function renderParameters(parameters, nullableParameters = new Set()) {
  return splitParameters(parameters)
    .filter(Boolean)
    .map((parameter) => {
      const separator = parameter.indexOf(":");
      if (separator < 0) return `object ${csharpIdentifier(parameter)}`;
      const name = parameter.slice(0, separator).trim();
      const type = parameter.slice(separator + 1).trim();
      return `${nullableParameters.has(name) ? "dynamic" : csharpType(type)} ${csharpIdentifier(name)}`;
    })
    .join(", ");
}

export function renderSignature(line, nullableParameters = new Set()) {
  const match = line.match(/^(\s*)fn\s+([A-Za-z_][A-Za-z0-9_]*)\((.*?)\)(?:\s*->\s*([^\s{]+))?\s*\{$/);
  if (!match) return null;
  const [, indentation, name, parameters, returnType] = match;
  return `${indentation}public static ${csharpType(returnType ?? "any")} ${csharpIdentifier(name)}(${renderParameters(parameters, nullableParameters)}) {`;
}

export function isSafeManifestMethod(name, manifest) {
  return (manifest?.abi?.methods ?? []).some(
    (method) => method.safe === true && sanitizeIdentifier(method.name) === name,
  );
}

export function manifestMethodForName(name, manifest) {
  return (manifest?.abi?.methods ?? []).find(
    (method) => sanitizeIdentifier(method.name) === name,
  ) ?? null;
}

function inferExpressionType(expression) {
  let value = expression.trim();
  while (value.startsWith("(") && value.endsWith(")")) {
    value = value.slice(1, -1).trim();
  }
  if (/^(?:true|false)$/i.test(value)) return "bool";
  if (/^-?\d+$/.test(value)) return "BigInteger";
  if (/^"(?:[^"\\]|\\.)*"$/.test(value)) return "string";
  if (/^new_array_t\s*\(/i.test(value)) {
    const type = value.match(/,\s*"?([a-z]+)"?\s*\)\s*$/i)?.[1]?.toLowerCase();
    return {
      bool: "bool[]",
      boolean: "bool[]",
      int: "BigInteger[]",
      integer: "BigInteger[]",
      buffer: "byte[]",
      bytes: "ByteString[]",
      bytestring: "ByteString[]",
    }[type] ?? "object[]";
  }
  if (/^(?:new_array|pack)\s*\(/i.test(value)) return "object[]";
  if (/^Map\s*\(/i.test(value)) return "Map<object, object>";
  if (/^(?:is_null|is_type|has_key|within|equals|not_equals|not|is_valid)\s*\(/i.test(value)) {
    return "bool";
  }
  if (/^(?:len|size|abs|sqrt|min|max|sign|convert_to_integer)\s*\(/i.test(value)) {
    return "BigInteger";
  }
  if (/^convert_to_bool\s*\(/i.test(value)) return "bool";
  if (/^convert_to_bytestring\s*\(/i.test(value)) return "ByteString";
  if (/[<>=!]=?|&&|\|\|/.test(value)) return "bool";
  if (/[+\-*\/%]|\b(?:and|or|xor|shl|shr)\b/.test(value)) return "BigInteger";
  return "dynamic";
}

export function inferDeclarationTypes(lines) {
  const observed = new Map();
  for (const line of lines) {
    const match = line.trim().match(/^(?:let\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+);$/);
    if (!match) continue;
    const type = inferExpressionType(match[2]);
    if (!observed.has(match[1])) observed.set(match[1], new Set());
    observed.get(match[1]).add(type);
  }
  return new Map(
    [...observed].map(([name, types]) => [
      name,
      types.size === 1 && !types.has("dynamic") ? [...types][0] : "dynamic",
    ]),
  );
}

export function renderBodyLine(line, declarationTypes = null) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  const declaration = trimmed.match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*.*)?;$/);
  if (declaration) {
    const declarationType = declarationTypes
      ? declarationTypes.get(declaration[1]) ?? "dynamic"
      : "var";
    return rewriteCSharpExpression(
      `${indentation}${declarationType} ${csharpIdentifier(declaration[1])}${declaration[2] ?? ""};`,
    );
  }
  const throwExpression = trimmed.match(/^throw\((.*)\);$/);
  if (throwExpression) {
    return `${indentation}throw new Exception(Convert.ToString(${throwExpression[1]}));`;
  }
  const abortExpression = trimmed.match(/^abort\((.*)\);$/);
  if (abortExpression) {
    const payload = abortExpression[1].trim();
    return payload
      ? `${indentation}throw new InvalidOperationException(Convert.ToString(${payload}));`
      : `${indentation}throw new InvalidOperationException();`;
  }
  if (trimmed === "abort" || trimmed === "abort;") {
    return `${indentation}throw new InvalidOperationException();`;
  }
  return rewriteCSharpExpression(line)
    .replace(/\bunknown\b/g, "default");
}

export function renderMetadataLine(line) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  if (
    /^(supported_standards|features|groups|permissions|trusts)\b/.test(trimmed) ||
    trimmed.startsWith("pubkey=")
  ) {
    return `${indentation}// ${trimmed}`;
  }
  return null;
}

export function renderEventDeclaration(line) {
  const match = line.match(/^(\s*)event\s+([A-Za-z_][A-Za-z0-9_]*)\((.*?)\);(?:\s*\/\/\s*manifest\s+(.+))?$/);
  if (!match) return null;
  const [, indentation, name, parameters, originalName] = match;
  const types = splitCallArguments(parameters).map((parameter) => {
    const separator = parameter.indexOf(":");
    return csharpType(separator < 0 ? "any" : parameter.slice(separator + 1).trim());
  });
  const actionType = types.length === 0 ? "Action" : `Action<${types.join(", ")}>`;
  const displayName = originalName ? `${indentation}[DisplayName(${originalName})]\n` : "";
  return `${displayName}${indentation}public static event ${actionType} ${csharpIdentifier(name)};`;
}

/**
 * Render the JS high-level surface as readable C#-style source.
 *
 * VM-specific expressions are intentionally retained verbatim so a caller
 * can still inspect the recovered operation when it is not directly
 * representable as a C# expression. This is a source-oriented view, not a
 * claim that every generated body is compilable against the Neo framework.
 */
