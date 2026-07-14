import { renderCSharpSyscall } from "./csharp-syscalls.js";

const CSHARP_COLLECTION_HELPERS = new Map([
  ["new_array", (args) => `new object[(int)(${args[0] ?? "???"})]`],
  ["new_buffer", (args) => `new byte[(int)(${args[0] ?? "???"})]`],
  ["new_array_t", (args) => {
    const type = String(args[1] ?? "").replace(/^"|"$/g, "").toLowerCase();
    const element = {
      bool: "bool",
      boolean: "bool",
      integer: "BigInteger",
      int: "BigInteger",
      bytes: "ByteString",
      bytestring: "ByteString",
      buffer: "byte",
    }[type] ?? "object";
    return `new ${element}[(int)(${args[0] ?? "???"})]`;
  }],
  ["new_struct", (args, types) => args.length === 1
    ? `new object[] { ${rewriteCSharpExpression(args[0], types)} }`
    : null],
  ["Map", (args, types) => {
    if (args.length === 0) return "new Map<object, object>()";
    const entries = args.map((entry) => {
      const colon = splitTopLevelColon(entry);
      if (colon < 0) return null;
      const key = rewriteCSharpExpression(entry.slice(0, colon).trim(), types);
      const value = rewriteCSharpExpression(entry.slice(colon + 1).trim(), types);
      return `[${key}] = ${value}`;
    });
    return entries.every(Boolean)
      ? `new Map<object, object> { ${entries.join(", ")} }`
      : null;
  }],
  ["Struct", (args, types) => args.length === 0
    ? "new object[] { }"
    : `new object[] { ${args.map((arg) => rewriteCSharpExpression(arg, types)).join(", ")} }`],
  ["is_null", (args) => args.length === 1 ? `(${args[0]} is null)` : null],
  ["clear_items", (args, types) => {
    if (args.length !== 1) return null;
    const kind = collectionKind(args[0], types);
    return kind === "map"
      ? `${args[0]}.Clear()`
      : kind === "list"
        ? `((Neo.SmartContract.Framework.List<${listElementType(args[0], types)}>)${args[0]}).Clear()`
        : `((dynamic)${args[0]}).Clear()`;
  }],
  ["keys", (args, types) => args.length === 1
    ? collectionKind(args[0], types) === "map" ? `${args[0]}.Keys` : `((dynamic)${args[0]}).Keys`
    : null],
  ["values", (args, types) => args.length === 1
    ? collectionKind(args[0], types) === "map" ? `${args[0]}.Values` : `((dynamic)${args[0]}).Values`
    : null],
  ["remove_item", (args, types) => {
    if (args.length !== 2) return null;
    const kind = collectionKind(args[0], types);
    return kind === "map"
      ? `${args[0]}.Remove(${args[1]})`
      : kind === "list"
        ? `((Neo.SmartContract.Framework.List<${listElementType(args[0], types)}>)${args[0]}).RemoveAt((int)(${args[1]}))`
        : `((dynamic)${args[0]}).Remove(${args[1]})`;
  }],
  ["append", (args, types) => {
    if (args.length !== 2) return null;
    return collectionKind(args[0], types) === "list"
      ? `((Neo.SmartContract.Framework.List<${listElementType(args[0], types)}>)${args[0]}).Add(${args[1]})`
      : `((dynamic)${args[0]}).Add(${args[1]})`;
  }],
  ["has_key", (args, types) => args.length === 2
    ? collectionKind(args[0], types) === "map"
      ? `${args[0]}.HasKey(${args[1]})`
      : `((dynamic)${args[0]}).HasKey(${args[1]})`
    : null],
  ["convert_to_integer", (args) => args.length === 1 ? `(BigInteger)(${args[0]})` : null],
  ["convert_to_bool", (args) => args.length === 1 ? `(bool)(${args[0]})` : null],
  ["convert_to_bytestring", (args) => args.length === 1 ? `(ByteString)(${args[0]})` : null],
  ["convert", (args) => args.length === 1 ? `(object)(${args[0]})` : null],
  ["len", (args, types) => args.length === 1 ? collectionLength(args[0], types) : null],
  ["size", (args, types) => args.length === 1 ? collectionLength(args[0], types) : null],
  ["memcpy", (args) => args.length === 5
    ? `Array.Copy(${args[2]}, (int)(${args[3]}), ${args[0]}, (int)(${args[1]}), (int)(${args[4]}))`
    : null],
  ["pack", (args) => `new object[] { ${args.join(", ")} }`],
  ["abs", (args) => args.length === 1 ? `BigInteger.Abs(${args[0]})` : null],
  ["sign", (args) => args.length === 1 ? `(${args[0]}).Sign` : null],
  ["min", (args) => args.length === 2 ? `BigInteger.Min(${args[0]}, ${args[1]})` : null],
  ["max", (args) => args.length === 2 ? `BigInteger.Max(${args[0]}, ${args[1]})` : null],
  ["sqrt", (args) => args.length === 1 ? `Helper.Sqrt(${args[0]})` : null],
  ["modmul", (args) => args.length === 3 ? `Helper.ModMultiply(${args.join(", ")})` : null],
  ["modpow", (args) => args.length === 3 ? `BigInteger.ModPow(${args.join(", ")})` : null],
  ["pow", (args) => args.length === 2
    ? `BigInteger.Pow(${args[0]}, (int)(${args[1]}))`
    : null],
  ["within", (args) => args.length === 3 ? `Helper.Within(${args.join(", ")})` : null],
  ["substr", (args) => args.length === 3
    ? `Helper.Range(${args[0]}, (int)(${args[1]}), (int)(${args[2]}))`
    : null],
  ["left", (args) => args.length === 2
    ? `Helper.Take(${args[0]}, (int)(${args[1]}))`
    : null],
  ["right", (args) => args.length === 2
    ? `Helper.Last(${args[0]}, (int)(${args[1]}))`
    : null],
  ["pop_item", (args, types) => {
    if (args.length !== 1) return null;
    return collectionKind(args[0], types) === "list"
      ? `((Neo.SmartContract.Framework.List<${listElementType(args[0], types)}>)${args[0]}).PopItem()`
      : `((dynamic)${args[0]}).PopItem()`;
  }],
  ["reverse_items", (args) => args.length === 1
    ? `Helper.Reverse(${args[0]})`
    : null],
]);

export function rewriteCSharpExpression(line, types = null) {
  return rewriteEmptyArrayLiterals(
    rewriteConcatenation(
      rewriteQualifiedCalls(rewriteKnownSyscalls(rewriteKnownHelpers(line, types))),
    ),
  );
}

function rewriteEmptyArrayLiterals(line) {
  const pattern = /\[\]/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    let previous = match.index - 1;
    while (previous >= 0 && /\s/.test(line[previous])) previous -= 1;
    const isTypeSuffix = previous >= 0 && /[A-Za-z0-9_>\]]/.test(line[previous]);
    output += line.slice(cursor, match.index);
    output += isTypeSuffix ? "[]" : "new object[] { }";
    cursor = match.index + 2;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function rewriteConcatenation(line) {
  const pattern = /\bcat\b/g;
  let output = "";
  let cursor = 0;
  while (true) {
    const match = nextOutsideMatch(line, pattern);
    if (!match) break;
    output += line.slice(cursor, match.index).replace(/\s+$/, "") + " + ";
    cursor = pattern.lastIndex;
    while (/\s/.test(line[cursor] ?? "")) cursor += 1;
    pattern.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function rewriteQualifiedCalls(line) {
  const pattern = /\b([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)\s*\(/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    output += line.slice(cursor, match.index);
    output += `${match[1]}.${match[2]}(`;
    cursor = pattern.lastIndex;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function rewriteKnownHelpers(line, types) {
  let output = line;
  for (let pass = 0; pass < 32; pass += 1) {
    const match = nextOutsideMatch(
      output,
      /\b(new_array_t|new_array|new_buffer|new_struct|is_null|clear_items|remove_item|append|has_key|convert_to_integer|convert_to_bool|convert_to_bytestring|convert|len|size|memcpy|keys|values|pack|Map|Struct|abs|sign|min|max|sqrt|modmul|modpow|pow|within|substr|left|right|pop_item|reverse_items|is_type_[A-Za-z0-9_]+)\s*\(/g,
    );
    if (!match) break;
    const open = output.indexOf("(", match.index);
    const close = findCallClose(output, open);
    if (close < 0) break;
    const args = splitCallArguments(output.slice(open + 1, close));
    const renderer = CSHARP_COLLECTION_HELPERS.get(match[1]);
    const replacement = match[1].startsWith("is_type_")
      ? renderCSharpTypeTest(match[1], args)
      : renderer?.(args, types);
    if (!replacement) break;
    output = `${output.slice(0, match.index)}${replacement}${output.slice(close + 1)}`;
  }
  return output;
}

function collectionKind(expression, types) {
  if (!types) return "unknown";
  const name = expression.trim().replace(/^@/, "");
  const type = types.get(name) ?? types.get(expression.trim()) ?? "";
  if (/^Map<|\bMap\b/.test(type)) return "map";
  if (/\[\]$/.test(type) || /\bList</.test(type)) return "list";
  return "unknown";
}

function collectionLength(expression, types) {
  const source = expression.trim();
  const type = inferredType(source, types);
  if (/^Map(?:<|\b)/.test(type)) return `${source}.Count`;
  if (/\bList</.test(type)) return `${source}.Count`;
  if (/\[\]$/.test(type) || /^(?:ByteString|string)$/.test(type)) {
    return `${source}.Length`;
  }
  if (/^\"(?:[^\"\\]|\\.)*\"$/.test(source)) return `${source}.Length`;
  return `((dynamic)${source}).Count`;
}

function inferredType(expression, types) {
  if (!types) return "";
  const name = expression.trim().replace(/^@/, "");
  return types.get(name) ?? types.get(expression.trim()) ?? "";
}

function renderCSharpTypeTest(name, args) {
  if (args.length !== 1) return null;
  const kind = name.slice("is_type_".length).toLowerCase();
  if (kind === "any") return "true";
  const type = {
    bool: "bool",
    boolean: "bool",
    integer: "BigInteger",
    bytestring: "ByteString",
    buffer: "byte[]",
    array: "object[]",
    struct: "object[]",
    map: "Map<object, object>",
    interopinterface: "object",
    pointer: "System.IntPtr",
  }[kind] ?? null;
  return type ? `((object)(${args[0]})) is ${type}` : null;
}

function listElementType(expression, types) {
  if (!types) return "object";
  const name = expression.trim().replace(/^@/, "");
  const type = types.get(name) ?? types.get(expression.trim()) ?? "";
  return type.match(/^(.+)\[\]$/)?.[1] ?? "object";
}

function splitTopLevelColon(text) {
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
    else if (")]}>".includes(character)) depth -= 1;
    else if (character === ":" && depth === 0) return index;
  }
  return -1;
}

function rewriteKnownSyscalls(line) {
  const marker = /syscall\("([^"]+)"/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, marker)) !== null) {
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (open < 0 || close < 0) continue;
    const argsText = line
      .slice(open + 1, close)
      .replace(/^\s*"[^"]*"\s*(?:,\s*)?/, "")
      .trim();
    const args = splitCallArguments(argsText);
    const replacement = renderCSharpSyscall(match[1], args);
    if (!replacement) continue;
    output += line.slice(cursor, match.index) + replacement;
    cursor = close + 1;
    marker.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function nextOutsideMatch(text, pattern) {
  let match;
  while ((match = pattern.exec(text)) !== null) {
    if (!isInsideQuotedString(text, match.index)) return match;
  }
  return null;
}

function isInsideQuotedString(text, end) {
  let quote = null;
  for (let index = 0; index < end; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
    } else if (character === '"' || character === "'") {
      quote = character;
    }
  }
  return quote !== null;
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

export function splitCallArguments(text) {
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
