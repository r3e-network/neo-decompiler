import {
  findBracketClose,
  isInsideQuotedString,
  nextOutsideMatch,
  splitCallArguments,
} from "./csharp-expression-scanner.js";

export function rewriteOversizedHexLiterals(line) {
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

export function rewriteOversizedDecimalLiterals(line) {
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

export function rewriteUnknownPlaceholders(line) {
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

// High-level VM notation represents a function pointer as `&method`. C# only
// permits method groups in delegate/function-pointer contexts, while the
// generated contract intentionally keeps VM values dynamic. Lower the marker
// to a compile-safe dynamic value and retain the original target in a comment
// so the recovered control-flow fact is still visible to readers.
export function rewriteFunctionPointers(line) {
  const pattern = /&([A-Za-z_][A-Za-z0-9_]*)/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    if (!isFunctionPointerContext(line, match.index)) continue;
    output += line.slice(cursor, match.index);
    output += `default(dynamic) /* unresolved VM function pointer &${match[1]} */`;
    cursor = match.index + match[0].length;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function isFunctionPointerContext(line, index) {
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous -= 1;
  if (previous < 0 || "([{,=:".includes(line[previous])) return true;
  const prefix = line.slice(0, previous + 1);
  return /\breturn\s*$/.test(prefix);
}

export function rewriteEmptyArrayLiterals(line) {
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

export function rewriteCollectionLiterals(line, rewriteExpression) {
  let output = "";
  let cursor = 0;
  for (let index = 0; index < line.length; index += 1) {
    if (line[index] !== "[" || isInsideQuotedString(line, index) || !isCollectionLiteralStart(line, index)) {
      continue;
    }
    const close = findBracketClose(line, index);
    if (close < 0) continue;
    const elements = splitCallArguments(line.slice(index + 1, close))
      .map((element) => rewriteExpression(element));
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
