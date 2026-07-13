import { sanitizeIdentifier } from "./manifest.js";

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

function escapeCSharpString(value) {
  return String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function renderManifestAttributes(manifest) {
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

function isSafeManifestMethod(name, manifest) {
  return (manifest?.abi?.methods ?? []).some(
    (method) => method.safe === true && sanitizeIdentifier(method.name) === name,
  );
}

function manifestMethodForName(name, manifest) {
  return (manifest?.abi?.methods ?? []).find(
    (method) => sanitizeIdentifier(method.name) === name,
  ) ?? null;
}

function renderBodyLine(line) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  const declaration = trimmed.match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*.*)?;$/);
  if (declaration) {
    return rewriteKnownSyscalls(
      rewriteKnownHelpers(
        `${indentation}var ${csharpIdentifier(declaration[1])}${declaration[2] ?? ""};`,
      ),
    );
  }
  const throwExpression = trimmed.match(/^throw\((.*)\);$/);
  if (throwExpression) {
    return `${indentation}throw new Exception(Convert.ToString(${throwExpression[1]}));`;
  }
  if (trimmed === "abort();" || trimmed === "abort") {
    return `${indentation}throw new Exception("ABORT");`;
  }
  return rewriteKnownSyscalls(rewriteKnownHelpers(line)).replace(/\bunknown\b/g, "default");
}

const CSHARP_COLLECTION_HELPERS = new Map([
  ["new_array", (args) => `new object[(int)(${args[0] ?? "???"})]`],
  ["new_buffer", (args) => `new byte[(int)(${args[0] ?? "???"})]`],
  ["new_array_t", (args) => {
    const type = String(args[1] ?? "").replace(/^"|"$/g, "").toLowerCase();
    const element = type === "buffer" ? "byte" : "object";
    return `new ${element}[(int)(${args[0] ?? "???"})]`;
  }],
  ["Map", (args) => args.length === 0 ? "new Map<object, object>()" : null],
  ["Struct", (args) => args.length === 0 ? "new Struct()" : null],
  ["is_null", (args) => args.length === 1 ? `(${args[0]} is null)` : null],
  ["clear_items", (args) => args.length === 1 ? `${args[0]}.Clear()` : null],
  ["keys", (args) => args.length === 1 ? `${args[0]}.Keys` : null],
  ["values", (args) => args.length === 1 ? `${args[0]}.Values` : null],
  ["remove_item", (args) => args.length === 2 ? `${args[0]}.Remove(${args[1]})` : null],
  ["append", (args) => args.length === 2 ? `${args[0]}.Add(${args[1]})` : null],
  ["has_key", (args) => args.length === 2 ? `${args[0]}.ContainsKey(${args[1]})` : null],
  ["convert_to_integer", (args) => args.length === 1 ? `(BigInteger)(${args[0]})` : null],
  ["convert_to_bool", (args) => args.length === 1 ? `(bool)(${args[0]})` : null],
  ["convert_to_bytestring", (args) => args.length === 1 ? `(ByteString)(${args[0]})` : null],
  ["pack", (args) => `new object[] { ${args.join(", ")} }`],
]);

function rewriteKnownHelpers(line) {
  let output = line;
  for (let pass = 0; pass < 32; pass += 1) {
    const match = output.match(/\b(new_array_t|new_array|new_buffer|is_null|clear_items|remove_item|append|has_key|convert_to_integer|convert_to_bool|convert_to_bytestring|keys|values|pack|Map|Struct)\s*\(/);
    if (!match) break;
    const open = output.indexOf("(", match.index);
    const close = findCallClose(output, open);
    if (close < 0) break;
    const args = splitCallArguments(output.slice(open + 1, close));
    const renderer = CSHARP_COLLECTION_HELPERS.get(match[1]);
    const replacement = renderer?.(args);
    if (!replacement) break;
    output = `${output.slice(0, match.index)}${replacement}${output.slice(close + 1)}`;
  }
  return output;
}

const CSHARP_SYSCALLS = new Map([
  ["System.Storage.GetContext", "Storage.CurrentContext"],
  ["System.Storage.GetReadOnlyContext", "Storage.CurrentReadOnlyContext"],
  ["System.Runtime.GetTime", "Runtime.Time"],
  ["System.Runtime.GetCallingScriptHash", "Runtime.CallingScriptHash"],
  ["System.Runtime.GetEntryScriptHash", "Runtime.EntryScriptHash"],
  ["System.Runtime.GetExecutingScriptHash", "Runtime.ExecutingScriptHash"],
  ["System.Runtime.GetInvocationCounter", "Runtime.InvocationCounter"],
  ["System.Runtime.GetNetwork", "Runtime.GetNetwork"],
  ["System.Runtime.GetTrigger", "Runtime.Trigger"],
  ["System.Storage.Get", "Storage.Get"],
  ["System.Storage.Put", "Storage.Put"],
  ["System.Storage.Delete", "Storage.Delete"],
  ["System.Storage.Find", "Storage.Find"],
  ["System.Runtime.Notify", "Runtime.Notify"],
  ["System.Runtime.Log", "Runtime.Log"],
  ["System.Runtime.CheckWitness", "Runtime.CheckWitness"],
  ["System.Runtime.GetNotifications", "Runtime.GetNotifications"],
  ["System.Runtime.BurnGas", "Runtime.BurnGas"],
  ["System.Contract.Call", "Contract.Call"],
  ["System.Contract.CallNative", "Contract.CallNative"],
  ["System.Contract.CallLegacy", "Contract.CallLegacy"],
]);

function rewriteKnownSyscalls(line) {
  const marker = /syscall\("([^"]+)"/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = marker.exec(line)) !== null) {
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (open < 0 || close < 0) continue;
    const api = CSHARP_SYSCALLS.get(match[1]);
    if (!api) continue;
    const argsText = line
      .slice(open + 1, close)
      .replace(/^\s*"[^"]*"\s*(?:,\s*)?/, "")
      .trim();
    const args = splitCallArguments(argsText);
    const replacement = api.includes(".") && args.length === 0 && isStaticSyscall(match[1])
      ? api
      : `${api}(${args.join(", ")})`;
    output += line.slice(cursor, match.index) + replacement;
    cursor = close + 1;
    marker.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function isStaticSyscall(name) {
  return name === "System.Storage.GetContext" ||
    name === "System.Storage.GetReadOnlyContext" ||
    name === "System.Runtime.GetTime" ||
    name === "System.Runtime.GetCallingScriptHash" ||
    name === "System.Runtime.GetEntryScriptHash" ||
    name === "System.Runtime.GetExecutingScriptHash" ||
    name === "System.Runtime.GetInvocationCounter" ||
    name === "System.Runtime.GetTrigger";
}

function findCallClose(text, open) {
  if (open < 0) return -1;
  let depth = 0;
  let quote = null;
  for (let index = open; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "(") {
      depth += 1;
    } else if (character === ")" && --depth === 0) {
      return index;
    }
  }
  return -1;
}

function splitCallArguments(text) {
  if (!text) return [];
  const result = [];
  let start = 0;
  let depth = 0;
  let quote = null;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") quote = character;
    else if ("([{<".includes(character)) depth += 1;
    else if (")]} >".replace(" ", "").includes(character)) depth -= 1;
    else if (character === "," && depth === 0) {
      result.push(text.slice(start, index).trim());
      start = index + 1;
    }
  }
  const tail = text.slice(start).trim();
  if (tail) result.push(tail);
  return result;
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

function renderEventDeclaration(line) {
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
export function renderCSharpContract(highLevel, manifest = null) {
  if (typeof highLevel !== "string") {
    throw new TypeError("highLevel must be a string");
  }

  const output = [
    "using System;",
    "using System.Numerics;",
    "using Neo.SmartContract.Framework;",
    "using Neo.SmartContract.Framework.Attributes;",
    "using Neo.SmartContract.Framework.Services;",
    "",
  ];
  let classSeen = false;
  for (const line of highLevel.split(/\r?\n/)) {
    const contract = line.match(/^contract\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{$/);
    if (contract) {
      for (const attribute of renderManifestAttributes(manifest)) output.push(attribute);
      output.push(`public class ${csharpIdentifier(contract[1])} : SmartContract {`);
      classSeen = true;
      continue;
    }
    if (/^\s*fn\s+.*;(?:\s*\/\/.*)?$/.test(line)) {
      output.push(`${line.match(/^\s*/)?.[0] ?? ""}// ${line.trim()}`);
      continue;
    }
    if (/^\s*event\s+/.test(line)) {
      const event = renderEventDeclaration(line);
      output.push(event ?? `${line.match(/^\s*/)?.[0] ?? ""}// ${line.trim()}`);
      continue;
    }
    const signature = renderSignature(line);
    if (signature) {
      const name = line.match(/^\s*fn\s+([A-Za-z_][A-Za-z0-9_]*)/)?.[1];
      const method = name ? manifestMethodForName(name, manifest) : null;
      const indentation = line.match(/^\s*/)?.[0] ?? "";
      if (method && sanitizeIdentifier(method.name) !== method.name) {
        output.push(`${indentation}[DisplayName("${escapeCSharpString(method.name)}")]`);
      }
      if (name && isSafeManifestMethod(name, manifest)) {
        output.push(`${indentation}[Safe]`);
      }
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
