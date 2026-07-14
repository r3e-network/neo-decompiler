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
  ["Map", (args) => args.length === 0 ? "new Map<object, object>()" : null],
  ["Struct", (args) => args.length === 0 ? "new object[] { }" : null],
  ["is_null", (args) => args.length === 1 ? `(${args[0]} is null)` : null],
  ["clear_items", (args, types) => {
    if (args.length !== 1) return null;
    const kind = collectionKind(args[0], types);
    return kind === "map"
      ? `${args[0]}.Clear()`
      : kind === "list"
        ? `((Neo.SmartContract.Framework.List<object>)${args[0]}).Clear()`
        : `((dynamic)${args[0]}).Clear()`;
  }],
  ["keys", (args) => args.length === 1 ? `${args[0]}.Keys` : null],
  ["values", (args) => args.length === 1 ? `${args[0]}.Values` : null],
  ["remove_item", (args, types) => {
    if (args.length !== 2) return null;
    const kind = collectionKind(args[0], types);
    return kind === "map"
      ? `${args[0]}.Remove(${args[1]})`
      : kind === "list"
        ? `((Neo.SmartContract.Framework.List<object>)${args[0]}).RemoveAt((int)(${args[1]}))`
        : `((dynamic)${args[0]}).Remove(${args[1]})`;
  }],
  ["append", (args) => args.length === 2
    ? `((Neo.SmartContract.Framework.List<object>)${args[0]}).Add(${args[1]})`
    : null],
  ["has_key", (args) => args.length === 2 ? `${args[0]}.ContainsKey(${args[1]})` : null],
  ["convert_to_integer", (args) => args.length === 1 ? `(BigInteger)(${args[0]})` : null],
  ["convert_to_bool", (args) => args.length === 1 ? `(bool)(${args[0]})` : null],
  ["convert_to_bytestring", (args) => args.length === 1 ? `(ByteString)(${args[0]})` : null],
  ["pack", (args) => `new object[] { ${args.join(", ")} }`],
  ["abs", (args) => args.length === 1 ? `BigInteger.Abs(${args[0]})` : null],
  ["sign", (args) => args.length === 1 ? `(${args[0]}).Sign` : null],
  ["min", (args) => args.length === 2 ? `BigInteger.Min(${args[0]}, ${args[1]})` : null],
  ["max", (args) => args.length === 2 ? `BigInteger.Max(${args[0]}, ${args[1]})` : null],
  ["sqrt", (args) => args.length === 1 ? `Helper.Sqrt(${args[0]})` : null],
  ["modmul", (args) => args.length === 3 ? `Helper.ModMultiply(${args.join(", ")})` : null],
  ["modpow", (args) => args.length === 3 ? `BigInteger.ModPow(${args.join(", ")})` : null],
  ["pow", (args) => args.length === 2 ? `BigInteger.Pow(${args.join(", ")})` : null],
  ["within", (args) => args.length === 3 ? `Helper.Within(${args.join(", ")})` : null],
  ["substr", (args) => args.length === 3 ? `Helper.Range(${args.join(", ")})` : null],
  ["left", (args) => args.length === 2 ? `Helper.Take(${args.join(", ")})` : null],
  ["right", (args) => args.length === 2 ? `Helper.Last(${args.join(", ")})` : null],
  ["pop_item", (args, types) => {
    if (args.length !== 1) return null;
    return collectionKind(args[0], types) === "list"
      ? `((Neo.SmartContract.Framework.List<object>)${args[0]}).PopItem()`
      : `((dynamic)${args[0]}).PopItem()`;
  }],
]);

const CSHARP_SYSCALLS = new Map([
  ["System.Contract.CreateStandardAccount", "Contract.CreateStandardAccount"],
  ["System.Contract.CreateMultisigAccount", "Contract.CreateMultisigAccount"],
  ["System.Storage.GetContext", "Storage.CurrentContext"],
  ["System.Storage.GetReadOnlyContext", "Storage.CurrentReadOnlyContext"],
  ["System.Runtime.GetTime", "Runtime.Time"],
  ["System.Runtime.GetRandom", "Runtime.GetRandom"],
  ["System.Runtime.GetScriptContainer", "Runtime.Transaction"],
  ["System.Runtime.GetCallingScriptHash", "Runtime.CallingScriptHash"],
  ["System.Runtime.GetEntryScriptHash", "Runtime.EntryScriptHash"],
  ["System.Runtime.GetExecutingScriptHash", "Runtime.ExecutingScriptHash"],
  ["System.Runtime.GetInvocationCounter", "Runtime.InvocationCounter"],
  ["System.Contract.GetCallFlags", "Contract.GetCallFlags"],
  ["System.Runtime.GetNetwork", "Runtime.GetNetwork"],
  ["System.Runtime.GetTrigger", "Runtime.Trigger"],
  ["System.Runtime.CurrentSigners", "Runtime.CurrentSigners"],
  ["System.Runtime.GasLeft", "Runtime.GasLeft"],
  ["System.Runtime.GetAddressVersion", "Runtime.AddressVersion"],
  ["System.Runtime.Platform", "Runtime.Platform"],
  ["System.Crypto.CheckSig", "Crypto.CheckSig"],
  ["System.Crypto.CheckMultisig", "Crypto.CheckMultisig"],
  ["System.Storage.Get", "Storage.Get"],
  ["System.Storage.Put", "Storage.Put"],
  ["System.Storage.Delete", "Storage.Delete"],
  ["System.Storage.Find", "Storage.Find"],
  ["System.Runtime.Notify", "Runtime.Notify"],
  ["System.Runtime.Log", "Runtime.Log"],
  ["System.Runtime.CheckWitness", "Runtime.CheckWitness"],
  ["System.Runtime.GetNotifications", "Runtime.GetNotifications"],
  ["System.Runtime.BurnGas", "Runtime.BurnGas"],
  ["System.Runtime.LoadScript", "Runtime.LoadScript"],
  ["System.Contract.Call", "Contract.Call"],
  ["System.Contract.CallNative", "Contract.CallNative"],
  ["System.Contract.CallLegacy", "Contract.CallLegacy"],
]);

export function rewriteCSharpExpression(line, types = null) {
  return rewriteConcatenation(
    rewriteQualifiedCalls(rewriteKnownSyscalls(rewriteKnownHelpers(line, types))),
  );
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
      /\b(new_array_t|new_array|new_buffer|is_null|clear_items|remove_item|append|has_key|convert_to_integer|convert_to_bool|convert_to_bytestring|keys|values|pack|Map|Struct|abs|sign|min|max|sqrt|modmul|modpow|pow|within|substr|left|right|pop_item)\s*\(/g,
    );
    if (!match) break;
    const open = output.indexOf("(", match.index);
    const close = findCallClose(output, open);
    if (close < 0) break;
    const args = splitCallArguments(output.slice(open + 1, close));
    const renderer = CSHARP_COLLECTION_HELPERS.get(match[1]);
    const replacement = renderer?.(args, types);
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

function rewriteKnownSyscalls(line) {
  const marker = /syscall\("([^"]+)"/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, marker)) !== null) {
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

function isStaticSyscall(name) {
  return name === "System.Storage.GetContext" ||
    name === "System.Storage.GetReadOnlyContext" ||
    name === "System.Runtime.GetTime" ||
    name === "System.Runtime.GetCallingScriptHash" ||
    name === "System.Runtime.GetEntryScriptHash" ||
    name === "System.Runtime.GetExecutingScriptHash" ||
    name === "System.Runtime.GetInvocationCounter" ||
    name === "System.Runtime.GetTrigger" ||
    name === "System.Runtime.GetScriptContainer" ||
    name === "System.Runtime.GasLeft" ||
    name === "System.Runtime.GetAddressVersion" ||
    name === "System.Runtime.Platform";
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
