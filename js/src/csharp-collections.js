function renderCSharpBufferConversion(value) {
  const source = value.trim();
  if (/^\(ByteString\)new byte\[\]/.test(source) || /^new byte\[\]/.test(source)) {
    return `(byte[])(${source})`;
  }
  return `((BigInteger)(${source})).ToByteArray()`;
}

export function createCSharpCollectionHelpers(rewriteExpression) {
  return new Map([
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
      ? `new object[] { ${rewriteExpression(args[0], types)} }`
      : null],
    ["Map", (args, types) => {
      if (args.length === 0) return "new Map<object, object>()";
      const entries = args.map((entry) => {
        const colon = splitTopLevelColon(entry);
        if (colon < 0) return null;
        const key = rewriteExpression(entry.slice(0, colon).trim(), types);
        const value = rewriteExpression(entry.slice(colon + 1).trim(), types);
        return `[${key}] = ${value}`;
      });
      return entries.every(Boolean)
        ? `new Map<object, object> { ${entries.join(", ")} }`
        : null;
    }],
    ["Struct", (args, types) => args.length === 0
      ? "new object[] { }"
      : `new object[] { ${args.map((arg) => rewriteExpression(arg, types)).join(", ")} }`],
    // Box the operand before the null test so literals and value-like Neo
    // types remain valid C# expressions (`long is null` is not legal C#).
    ["is_null", (args) => args.length === 1 ? `(((object)(${args[0]})) is null)` : null],
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
    ["convert_to_bytestring", (args) => args.length === 1
      ? renderByteStringConversion(args[0])
      : null],
    ["convert_to_buffer", (args) => args.length === 1 ? renderCSharpBufferConversion(args[0]) : null],
    ["convert", (args) => args.length === 1 ? `(object)(${args[0]})` : null],
    ["len", (args, types) => args.length === 1 ? collectionLength(args[0], types) : null],
    ["size", (args, types) => args.length === 1 ? collectionLength(args[0], types) : null],
    ["memcpy", (args) => args.length === 5
      ? `Array.Copy(${args[2]}, (int)(${args[3]}), ${args[0]}, (int)(${args[1]}), (int)(${args[4]}))`
      : null],
    ["unpack", (args) => args.length === 1 ? `((dynamic)${args[0]})` : null],
    ["unpack_item", (args) => args.length === 2
      ? `((dynamic)${args[0]})[(int)(${args[1]})]`
      : null],
    ["pack", (args) => `new object[] { ${args.join(", ")} }`],
    // The dynamic-count high-level form has no statically recoverable element
    // list. Preserve its runtime-sized collection shape without leaving a
    // pseudo helper in generated C#.
    ["pack_dynamic", (args) => args.length === 1
      ? `new object[(int)(${args[0]})]`
      : null],
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
    ["substr", (args, types) => args.length === 3
      ? renderSubstr(args, types)
      : null],
    ["left", (args, types) => args.length === 2
      ? renderLeft(args, types)
      : null],
    ["right", (args, types) => args.length === 2
      ? renderRight(args, types)
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

function renderByteStringConversion(expression) {
  const source = expression.trim();
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) {
    return `(ByteString)(BigInteger)(${source})`;
  }
  return `(ByteString)(${source})`;
}

function renderSubstr(args, types) {
  const source = args[0].trim();
  const type = inferredType(source, types);
  const receiver = type === "string" ? `(byte[])(ByteString)(${source})` : source;
  return `Helper.Range(${receiver}, (int)(${args[1]}), (int)(${args[2]}))`;
}

function renderLeft(args, types) {
  const source = args[0].trim();
  if (inferredType(source, types) === "string") {
    return `${source}.Substring(0, (int)(${args[1]}))`;
  }
  return `Helper.Take(${source}, (int)(${args[1]}))`;
}

function renderRight(args, types) {
  const source = args[0].trim();
  if (inferredType(source, types) === "string") {
    return `${source}.Substring(${source}.Length - (int)(${args[1]}), (int)(${args[1]}))`;
  }
  return `Helper.Last(${source}, (int)(${args[1]}))`;
}

export function renderCSharpTypeTest(name, args) {
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
