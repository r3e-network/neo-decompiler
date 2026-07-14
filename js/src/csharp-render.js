import { sanitizeIdentifier } from "./manifest.js";
import { splitCallArguments } from "./csharp-expression.js";
export { inferDeclarationTypes, renderBodyLine } from "./csharp-body.js";
import { inferDeclarationTypes, renderBodyLine } from "./csharp-body.js";
export { csharpIdentifier } from "./csharp-identifiers.js";
import { csharpIdentifier } from "./csharp-identifiers.js";

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

export function csharpType(type) {
  return TYPE_MAP.get(String(type ?? "any").trim().toLowerCase()) ?? "object";
}

export function escapeCSharpString(value) {
  let escaped = "";
  for (const character of String(value)) {
    switch (character) {
      case "\0": escaped += "\\0"; break;
      case "\u0007": escaped += "\\a"; break;
      case "\u0008": escaped += "\\b"; break;
      case "\u000C": escaped += "\\f"; break;
      case "\n": escaped += "\\n"; break;
      case "\r": escaped += "\\r"; break;
      case "\t": escaped += "\\t"; break;
      case "\u000B": escaped += "\\v"; break;
      case '"': escaped += '\\"'; break;
      case "\\": escaped += "\\\\"; break;
      case "\u2028": escaped += "\\u2028"; break;
      case "\u2029": escaped += "\\u2029"; break;
      default:
        if (character.charCodeAt(0) < 0x20 || character.charCodeAt(0) === 0x7f) {
          escaped += `\\u${character.charCodeAt(0).toString(16).padStart(4, "0").toUpperCase()}`;
        } else {
          escaped += character;
        }
    }
  }
  return escaped;
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
