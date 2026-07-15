import {
  findBracketClose,
  findCallClose,
  isInsideQuotedString,
  nextOutsideMatch,
} from "./csharp-expression-scanner.js";

// Neo VM truthiness permits numeric and bitwise values in a NOT operation,
// while C# requires `!` to receive a bool. Rewrite only compound operands that
// are visibly value-producing; simple identifiers stay untouched because
// their parameter/local type may be an already-boolean value.
export function rewriteNumericUnaryNot(line) {
  let output = "";
  let cursor = 0;
  let quote = null;
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }
    if (character === "/" && line[index + 1] === "/") break;
    if (character !== "!" || line[index + 1] === "=") continue;

    let operandStart = index + 1;
    while (/\s/.test(line[operandStart] ?? "")) operandStart += 1;
    if (operandStart >= line.length) continue;

    let operandEnd = operandStart;
    let operand;
    if (line[operandStart] === "(") {
      const close = findCallClose(line, operandStart);
      if (close < 0) continue;
      operand = line.slice(operandStart + 1, close).trim();
      operandEnd = close + 1;
    } else {
      while (/[A-Za-z0-9_@.]/.test(line[operandEnd] ?? "")) operandEnd += 1;
      if (line[operandEnd] === "(") {
        const close = findCallClose(line, operandEnd);
        if (close < 0) continue;
        operandEnd = close + 1;
      }
      operand = line.slice(operandStart, operandEnd).trim();
    }
    if (!operand || isLikelyBooleanExpression(operand)) continue;

    output += line.slice(cursor, index);
    output += `!((bool)(dynamic)(${operand}))`;
    cursor = operandEnd;
    index = operandEnd - 1;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

// C# requires an integral `int` shift count even when the Neo VM value is a
// BigInteger. Cast simple or parenthesized counts while leaving compound
// expressions intact for the caller's normal precedence handling.
export function rewriteShiftCounts(line) {
  let output = "";
  let cursor = 0;
  for (let index = 0; index < line.length - 1; index += 1) {
    if (
      (line[index] !== "<" && line[index] !== ">")
      || line[index + 1] !== line[index]
      || isInsideQuotedString(line, index)
    ) {
      continue;
    }
    if (line[index] === "/" && line[index + 1] === "/") break;
    let operandStart = index + 2;
    while (/\s/.test(line[operandStart] ?? "")) operandStart += 1;
    const operandEnd = findSimpleShiftOperandEnd(line, operandStart);
    if (operandEnd <= operandStart) continue;
    const operand = line.slice(operandStart, operandEnd).trim();
    if (!operand || /^\(\s*int\s*\)\s*\(/.test(operand)) continue;
    output += line.slice(cursor, index + 2);
    output += ` (int)(${operand})`;
    cursor = operandEnd;
    index = operandEnd - 1;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

// VM indices are BigInteger values, whereas C# arrays, strings, and
// ByteString indexers require `int`. Preserve the full index expression while
// making that boundary explicit.
export function rewriteIndexOperands(line, types = null) {
  let output = "";
  let cursor = 0;
  for (let index = 0; index < line.length; index += 1) {
    if (line[index] !== "[" || isInsideQuotedString(line, index)) continue;
    const close = findBracketClose(line, index);
    if (close < 0) continue;
    const operand = line.slice(index + 1, close).trim();
    if (!operand || /^\s*\(\s*int\s*\)/.test(operand)) {
      index = close;
      continue;
    }
    // Type suffixes (`object[]`, `byte[]`) and collection literals have no
    // index operand and must remain unchanged.
    const prefix = line.slice(0, index).trimEnd();
    const previous = prefix.slice(-1);
    if (previous === "{" || previous === ",") {
      const mapOpen = prefix.lastIndexOf("new Map<object, object> {");
      const mapClose = prefix.lastIndexOf("}");
      if (mapOpen > mapClose) {
        index = close;
        continue;
      }
    }
    if (/[A-Za-z0-9_>]/.test(previous) && /^(?:object|byte|bool|BigInteger|ByteString)\s*$/i.test(operand)) {
      index = close;
      continue;
    }
    const base = indexBase(line, index);
    if (base && needsDynamicIndexBase(base.expression, types)) {
      output += line.slice(cursor, base.start);
      output += `((dynamic)(${base.expression}))[`;
    } else {
      output += line.slice(cursor, index + 1);
    }
    output += `(int)(${operand})`;
    output += line.slice(close, close + 1);
    cursor = close + 1;
    index = close;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function indexBase(line, index) {
  const prefix = line.slice(0, index);
  const match = prefix.match(/((?:@?[A-Za-z_][A-Za-z0-9_]*)(?:\s*\[[^\]]+\])+|@?[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)?)\s*$/);
  if (!match) return null;
  return {
    expression: match[1].trim(),
    start: match.index,
  };
}

function needsDynamicIndexBase(expression, types) {
  if (!types) return false;
  const root = expression.match(/^@?([A-Za-z_][A-Za-z0-9_]*)/)?.[1];
  if (!root) return false;
  let type = types.get(root) ?? null;
  const indexes = expression.match(/\[/g)?.length ?? 0;
  for (let index = 0; index < indexes; index += 1) {
    if (type === "dynamic") return false;
    if (/\[\]$/.test(type ?? "")) type = type.slice(0, -2);
    else if (type === "ByteString" || type === "string") type = "value";
    else type = "object";
  }
  return type === "object" || type === "Block" || type === "BigInteger";
}

// VM equality and arithmetic accept mixed stack item types. When inference
// knows that a C# operand is object-like (or a VM boolean/ByteString), route
// the operation through `dynamic` so the generated contract remains valid
// instead of binding an impossible static C# operator pair.
export function rewriteDynamicOperators(line, types = null) {
  const operand = "(?:\\(\\s*)?@?[A-Za-z_][A-Za-z0-9_]*(?:\\s*\\[[^\\]]+\\])?(?:\\s*\\))?";
  const rightOperand = "(?:-?(?:0x[0-9A-Fa-f]+|\\d+)|@?[A-Za-z_][A-Za-z0-9_]*|\\([^()]+\\))";
  const pattern = new RegExp(`(${operand})\\s*(===|!==|==|!=|<=|>=|(?<!<)<(?!<)|(?<!>)>(?!>)|[+*/%&|^-])\\s*(${rightOperand})`, "g");
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const left = match[1].trim();
    const type = expressionType(left, types);
    if (
      !isDynamicOperatorType(type)
      || isGenericTypeOperator(left, match[2], match[3])
      || /^\(\s*dynamic\s*\)/.test(left)
    ) continue;
    output += line.slice(cursor, match.index);
    output += `((dynamic)(${left})) ${match[2]} ${match[3]}`;
    cursor = match.index + match[0].length;
    pattern.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function expressionType(expression, types) {
  if (!types) return null;
  let source = expression.replace(/^@/, "").trim();
  while (source.startsWith("(") && source.endsWith(")")) {
    source = source.slice(1, -1).trim();
  }
  const indexed = source.match(/^([A-Za-z_][A-Za-z0-9_]*)\s*\[/)?.[1];
  if (indexed) {
    const baseType = types.get(indexed) ?? "";
    if (/\[\]$/.test(baseType)) return baseType.slice(0, -2);
    if (baseType === "ByteString") return "byte";
    if (baseType === "string") return "char";
  }
  return types.get(source) ?? null;
}

function isDynamicOperatorType(type) {
  return ["object", "dynamic", "bool", "ByteString", "UInt160", "UInt256", "ECPoint"].includes(type);
}

function isGenericTypeOperator(left, operator, right) {
  if (operator !== "<" || !/^(?:object|bool|byte|BigInteger|ByteString|string)$/.test(right)) {
    return false;
  }
  return /(?:^|\.)(?:Map|List|Dictionary)$/.test(left.replace(/[()\s]/g, ""));
}

function findSimpleShiftOperandEnd(line, start) {
  if (line[start] === "(") {
    const close = findCallClose(line, start);
    return close < 0 ? start : close + 1;
  }
  let end = start;
  while (/[A-Za-z0-9_@.]/.test(line[end] ?? "")) end += 1;
  if (line[end] === "(") {
    const close = findCallClose(line, end);
    if (close >= 0) return close + 1;
  }
  return end;
}

function isLikelyBooleanExpression(expression) {
  const source = expression.trim();
  if (/^\(?\s*\(bool\)\s*\(dynamic\)/.test(source)) return true;
  if (/(?:===?|!==?|<=|>=|<|>|&&|\|\||\bis\s+null\b)/.test(source)) return true;
  if (/^(?:(?:is_null|is_type_[A-Za-z0-9_]+|within|equals|not_equals|not|is_valid)|Helper\.(?:Within|NumEqual))\s*\(/.test(source)) {
    return true;
  }
  if (/^@?[A-Za-z_][A-Za-z0-9_]*$/.test(source)) {
    return true;
  }
  return false;
}

export function rewriteConcatenation(line) {
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
