const TYPE_MAP = new Map([
  ["void", "void"],
  ["bool", "bool"],
  ["int", "BigInteger"],
  ["string", "string"],
  ["hash160", "UInt160"],
  ["hash256", "UInt256"],
  ["publickey", "ECPoint"],
  ["bytes", "ByteString"],
  ["bytestring", "ByteString"],
  ["signature", "ByteString"],
  ["array", "object[]"],
  ["map", "Map<object, object>"],
  ["interop", "object"],
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
  "volatile", "while",
]);

function csharpIdentifier(name) {
  return CSHARP_KEYWORDS.has(name) ? `@${name}` : name;
}

function csharpType(type) {
  return TYPE_MAP.get(String(type ?? "any").trim().toLowerCase()) ?? "object";
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

function renderParameters(parameters) {
  return splitParameters(parameters)
    .filter(Boolean)
    .map((parameter) => {
      const separator = parameter.indexOf(":");
      if (separator < 0) return `object ${csharpIdentifier(parameter)}`;
      const name = parameter.slice(0, separator).trim();
      const type = parameter.slice(separator + 1).trim();
      return `${csharpType(type)} ${csharpIdentifier(name)}`;
    })
    .join(", ");
}

function renderSignature(line) {
  const match = line.match(/^(\s*)fn\s+([A-Za-z_][A-Za-z0-9_]*)\((.*?)\)(?:\s*->\s*([^\s{]+))?\s*\{$/);
  if (!match) return null;
  const [, indentation, name, parameters, returnType] = match;
  return `${indentation}public static ${csharpType(returnType ?? "any")} ${csharpIdentifier(name)}(${renderParameters(parameters)}) {`;
}

function renderBodyLine(line) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  const declaration = trimmed.match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*.*)?;$/);
  if (declaration) {
    return `${indentation}var ${csharpIdentifier(declaration[1])}${declaration[2] ?? ""};`;
  }
  const throwExpression = trimmed.match(/^throw\((.*)\);$/);
  if (throwExpression) {
    return `${indentation}throw new Exception(Convert.ToString(${throwExpression[1]}));`;
  }
  if (trimmed === "abort();" || trimmed === "abort") {
    return `${indentation}throw new Exception("ABORT");`;
  }
  return line.replace(/\bunknown\b/g, "default");
}

function renderMetadataLine(line) {
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

/**
 * Render the JS high-level surface as readable C#-style source.
 *
 * VM-specific expressions are intentionally retained verbatim so a caller
 * can still inspect the recovered operation when it is not directly
 * representable as a C# expression. This is a source-oriented view, not a
 * claim that every generated body is compilable against the Neo framework.
 */
export function renderCSharpContract(highLevel, _manifest = null) {
  if (typeof highLevel !== "string") {
    throw new TypeError("highLevel must be a string");
  }

  const output = [
    "using System;",
    "using System.Numerics;",
    "using Neo.SmartContract.Framework;",
    "using Neo.SmartContract.Framework.Services;",
    "",
  ];
  let classSeen = false;
  for (const line of highLevel.split(/\r?\n/)) {
    const contract = line.match(/^contract\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{$/);
    if (contract) {
      output.push(`public class ${csharpIdentifier(contract[1])} : SmartContract {`);
      classSeen = true;
      continue;
    }
    if (/^\s*fn\s+.*;(?:\s*\/\/.*)?$/.test(line)) {
      output.push(`${line.match(/^\s*/)?.[0] ?? ""}// ${line.trim()}`);
      continue;
    }
    if (/^\s*event\s+/.test(line)) {
      output.push(`${line.match(/^\s*/)?.[0] ?? ""}// ${line.trim()}`);
      continue;
    }
    const signature = renderSignature(line);
    if (signature) {
      output.push(signature);
      continue;
    }
    const metadata = renderMetadataLine(line);
    output.push(metadata ?? renderBodyLine(line));
  }
  if (!classSeen) {
    output.push("public class NeoContract : SmartContract {");
    output.push("    // high-level contract body was unavailable");
  }
  return output.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}
