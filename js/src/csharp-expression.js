import { renderCSharpSyscall } from "./csharp-syscalls.js";
import {
  csharpIdentifier,
  isCSharpContextualKeyword,
} from "./csharp-identifiers.js";
import {
  createCSharpCollectionHelpers,
  renderCSharpTypeTest,
} from "./csharp-collections.js";
import {
  findBracketClose,
  findCallClose,
  findQuotedLiteralClose,
  isInsideQuotedString,
  nextOutsideMatch,
  splitCallArguments,
} from "./csharp-expression-scanner.js";

export { splitCallArguments } from "./csharp-expression-scanner.js";

const CSHARP_NATIVE_PROPERTIES = new Map([
  ["GasToken::Symbol", "GasToken.Symbol"],
  ["GasToken::Decimals", "GasToken.Decimals"],
  ["NeoToken::Symbol", "NeoToken.Symbol"],
  ["NeoToken::Decimals", "NeoToken.Decimals"],
  ["LedgerContract::CurrentHash", "LedgerContract.CurrentHash"],
  ["LedgerContract::CurrentIndex", "LedgerContract.CurrentIndex"],
]);

const CSHARP_COLLECTION_HELPERS = createCSharpCollectionHelpers(
  (expression, types) => rewriteCSharpExpression(expression, types),
);

export function rewriteCSharpExpression(line, types = null) {
  return rewriteCSharpIdentifiers(rewriteUnknownPlaceholders(rewriteCollectionLiterals(rewriteEmptyArrayLiterals(
    rewriteConcatenation(
      rewriteQualifiedCalls(rewriteKnownSyscalls(rewriteKnownHelpers(
        rewriteOversizedDecimalLiterals(rewriteOversizedHexLiterals(line)),
        types,
      ))),
    ),
  ))));
}

function rewriteOversizedHexLiterals(line) {
  const pattern = /\b0x([0-9a-fA-F]{17,})\b/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const paddedLength = match[1].length % 2 === 0 ? match[1].length : match[1].length + 1;
    const hex = match[1].padStart(paddedLength, "0");
    const bytes = hex.match(/../g)?.map((value) => `0x${value.toUpperCase()}`) ?? [];
    output += line.slice(cursor, match.index);
    output += `(ByteString)new byte[] { ${bytes.join(", ")} }`;
    cursor = match.index + match[0].length;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function rewriteOversizedDecimalLiterals(line) {
  const pattern = /(?<![A-Za-z0-9_])-?\d{19,}(?![A-Za-z0-9_])/g;
  const min = -(1n << 63n);
  const max = (1n << 63n) - 1n;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const value = BigInt(match[0]);
    output += line.slice(cursor, match.index);
    output += value < min || value > max
      ? `BigInteger.Parse("${match[0]}")`
      : match[0];
    cursor = match.index + match[0].length;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function rewriteUnknownPlaceholders(line) {
  const marker = /\?\?\?/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, marker)) !== null) {
    output += line.slice(cursor, match.index) + "default(dynamic)";
    cursor = match.index + match[0].length;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
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

function rewriteCollectionLiterals(line) {
  let output = "";
  let cursor = 0;
  for (let index = 0; index < line.length; index += 1) {
    if (line[index] !== "[" || isInsideQuotedString(line, index) || !isCollectionLiteralStart(line, index)) {
      continue;
    }
    const close = findBracketClose(line, index);
    if (close < 0) continue;
    const elements = splitCallArguments(line.slice(index + 1, close))
      .map((element) => rewriteCSharpExpression(element));
    output += line.slice(cursor, index);
    output += `new object[] { ${elements.join(", ")} }`;
    cursor = close + 1;
    index = close;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function isCollectionLiteralStart(line, index) {
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous -= 1;
  if (previous < 0) return true;
  if (line[previous] === "[") return true;
  if (line[previous] === "{") {
    const prefix = line.slice(0, previous).trimEnd();
    return !prefix.endsWith("new Map<object, object>");
  }
  if (line[previous] === ",") {
    const prefix = line.slice(0, previous).trimEnd();
    const mapOpen = prefix.lastIndexOf("new Map<object, object> {");
    const mapClose = prefix.lastIndexOf("}");
    if (mapOpen > mapClose) return false;
  }
  if ("=,(\:{;".includes(line[previous])) return true;
  if (/[+\-*/%&|!?<>]/.test(line[previous])) return true;
  const prefix = line.slice(0, previous + 1).match(/[A-Za-z_][A-Za-z0-9_]*$/)?.[0];
  return prefix === "return" || prefix === "throw";
}

function rewriteCSharpIdentifiers(line) {
  let output = "";
  for (let index = 0; index < line.length;) {
    const character = line[index];
    if (character === '"' || character === "'") {
      const close = findQuotedLiteralClose(line, index);
      output += line.slice(index, close < 0 ? line.length : close + 1);
      if (close < 0) break;
      index = close + 1;
      continue;
    }
    if (line.startsWith("//", index)) {
      output += line.slice(index);
      break;
    }
    if (line.startsWith("/*", index)) {
      const close = line.indexOf("*/", index + 2);
      output += line.slice(index, close < 0 ? line.length : close + 2);
      if (close < 0) break;
      index = close + 2;
      continue;
    }
    if (!/[A-Za-z_]/.test(character)) {
      output += character;
      index += 1;
      continue;
    }
    let end = index + 1;
    while (end < line.length && /[A-Za-z0-9_]/.test(line[end])) end += 1;
    const name = line.slice(index, end);
    const escaped = shouldEscapeCSharpIdentifier(line, index, name)
      ? csharpIdentifier(name)
      : name;
    output += escaped;
    index = end;
  }
  return output;
}

function shouldEscapeCSharpIdentifier(line, index, name) {
  if (index > 0 && line[index - 1] === "@") return false;
  if (name === "throw") {
    const tail = line.slice(index + name.length).trimStart();
    return !tail.startsWith("new");
  }
  if (!isCSharpContextualKeyword(name)) return false;
  if (name === "dynamic") return false;
  if (name === "global" && line.slice(index + name.length).startsWith("::")) return false;
  if (name === "let" && isForHeaderLet(line, index)) return false;
  return true;
}

function isForHeaderLet(line, index) {
  const prefix = line.slice(0, index);
  return /\bfor\s*\(\s*$/.test(prefix);
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
    const property = CSHARP_NATIVE_PROPERTIES.get(`${match[1]}::${match[2]}`);
    if (property) {
      const open = line.indexOf("(", match.index);
      const close = findCallClose(line, open);
      if (close >= 0 && !line.slice(open + 1, close).trim()) {
        output += line.slice(cursor, match.index) + property;
        cursor = close + 1;
        pattern.lastIndex = cursor;
        continue;
      }
    }
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
      /\b(new_array_t|new_array|new_buffer|new_struct|is_null|clear_items|remove_item|append|has_key|convert_to_integer|convert_to_bool|convert_to_bytestring|convert_to_buffer|convert|len|size|memcpy|unpack|unpack_item|keys|values|pack|pack_dynamic|pick|roll|Map|Struct|abs|sign|min|max|sqrt|modmul|modpow|pow|within|substr|left|right|pop_item|reverse_items|is_type_[A-Za-z0-9_]+)\s*\(/g,
    );
    if (!match) break;
    const open = output.indexOf("(", match.index);
    const close = findCallClose(output, open);
    if (close < 0) break;
    const args = splitCallArguments(output.slice(open + 1, close));
    const renderer = CSHARP_COLLECTION_HELPERS.get(match[1]);
    const replacement = match[1].startsWith("is_type_")
      ? renderCSharpTypeTest(match[1], args)
      : renderer?.(args, types) ?? renderUnresolvedStackHelper(match[1], args);
    if (!replacement) break;
    output = `${output.slice(0, match.index)}${replacement}${output.slice(close + 1)}`;
  }
  return output;
}

// A runtime-variable PICK/ROLL cannot be represented as an ordinary C#
// expression once the VM stack position has been lost. Keep the generated
// contract valid and make the loss explicit instead of leaking pseudo-code
// helpers that do not exist in the Neo framework.
function renderUnresolvedStackHelper(name, args) {
  if (name !== "pick" && name !== "roll") return null;
  return `default(dynamic) /* unresolved VM ${name.toUpperCase()}(${args.join(", ")}) */`;
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
    // Rewrite nested syscall expressions before rendering the enclosing call.
    // A syscall argument can itself be a syscall (for example storage calls
    // receiving `GetReadOnlyContext`), and a single left-to-right scan would
    // otherwise skip the nested marker inside the replaced outer call.
    const args = splitCallArguments(argsText).map((arg) => rewriteKnownSyscalls(arg));
    const replacement = renderCSharpSyscall(match[1], args);
    if (!replacement) continue;
    output += line.slice(cursor, match.index) + replacement;
    cursor = close + 1;
    marker.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}
