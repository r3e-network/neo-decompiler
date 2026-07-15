import {
  findBracketClose,
  findCallClose,
  isInsideQuotedString,
  nextOutsideMatch,
} from "./csharp-expression-scanner.js";
import {
  findMatchingOpen,
  scanIdentifierPathStart,
} from "./csharp-expression-lexical.js";

export {
  rewriteDynamicCompoundAssignments,
  rewriteDynamicOperators,
} from "./csharp-expression-dynamic.js";

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
    const hasIntCast = /^\s*\(\s*int\s*\)/.test(operand);
    if (!operand) {
      index = close;
      continue;
    }
    // Type suffixes (`object[]`, `byte[]`) and collection literals have no
    // index operand and must remain unchanged.
    const prefix = line.slice(0, index).trimEnd();
    const previous = prefix.slice(-1);
    if (/\bnew\s+[A-Za-z_][A-Za-z0-9_.]*(?:<[^>]*>)?\s*$/.test(prefix)) {
      index = close;
      continue;
    }
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
    const mapBase = base && isMapIndexBase(base.expression, types);
    const dynamicBase = base && (
      needsDynamicIndexBase(base.expression, types)
      || isDynamicIndexBase(base.expression)
      || isCallIndexBase(base.expression)
    );
    if (dynamicBase) {
      if (base.start < cursor) {
        const innerBase = stripBalancedOuterParens(base.expression);
        const rewrittenInnerBase = rewriteIndexOperands(innerBase, types);
        const renderedBase = output.lastIndexOf(innerBase) >= 0 ? innerBase : rewrittenInnerBase;
        const marker = output.lastIndexOf(renderedBase);
        if (marker < 0) {
          output += line.slice(cursor, index + 1);
        } else {
          output = `${output.slice(0, marker)}((dynamic)(${renderedBase}))${output.slice(marker + renderedBase.length)}`;
          output += line.slice(cursor, index + 1);
        }
      } else {
        output += line.slice(cursor, base.start);
        output += `((dynamic)(${base.expression}))[`;
      }
    } else {
      output += line.slice(cursor, index + 1);
    }
    output += dynamicBase && isDynamicIndexBase(base?.expression)
      ? stripIntIndexCast(operand)
      : mapBase
      ? stripIntIndexCast(operand)
      : hasIntCast ? operand : `(int)(${operand})`;
    output += line.slice(close, close + 1);
    cursor = close + 1;
    index = close;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function indexBase(line, index) {
  let end = index - 1;
  while (end >= 0 && /\s/.test(line[end])) end -= 1;
  if (end < 0) return null;
  let start;
  if (line[end] === ")") {
    const open = findMatchingOpen(line, end, "(", ")");
    if (open < 0) return null;
    start = open;
    let prefix = open - 1;
    while (prefix >= 0 && /\s/.test(line[prefix])) prefix -= 1;
    if (prefix >= 0 && /[A-Za-z0-9_.]/.test(line[prefix])) {
      const candidate = scanIdentifierPathStart(line, prefix);
      const name = line.slice(candidate, open).trim();
      if (!/^(?:if|while|for|switch|catch|return|throw)$/.test(name)) {
        start = candidate;
      }
    }
  } else if (line[end] === "]") {
    start = findMatchingOpen(line, end, "[", "]");
  } else if (/[A-Za-z0-9_@]/.test(line[end])) {
    start = scanIdentifierPathStart(line, end);
  }
  if (start === undefined || start < 0) return null;
  return {
    expression: line.slice(start, index).trim(),
    start,
  };
}

function needsDynamicIndexBase(expression, types) {
  if (!types) return false;
  let source = expression.trim();
  while (source.startsWith("(") && source.endsWith(")") && hasBalancedOuterParens(source)) {
    source = source.slice(1, -1).trim();
  }
  const root = source.match(/^@?([A-Za-z_][A-Za-z0-9_]*)/)?.[1];
  if (!root) return false;
  let type = types.get(root) ?? null;
  // `expression` is the base before the bracket currently being rendered;
  // include that bracket when walking the inferred element type.
  const indexes = (expression.match(/\[/g)?.length ?? 0) + 1;
  for (let index = 0; index < indexes; index += 1) {
    if (type === "dynamic") return false;
    if (/^Map(?:<|$)/.test(type ?? "")) return false;
    if (/\[\]$/.test(type ?? "")) type = type.slice(0, -2);
    else if (type === "ByteString" || type === "string") type = "value";
    else type = "object";
  }
  return type === "object" || type === "Block" || type === "BigInteger";
}

function isMapIndexBase(expression, types) {
  if (!types) return false;
  const source = stripBalancedOuterParens(expression).replace(/^@/, "");
  const root = source.match(/^([A-Za-z_][A-Za-z0-9_]*)/)?.[1];
  return /^Map(?:<|$)/.test(types.get(root) ?? "");
}

function isDynamicIndexBase(expression) {
  return /^\(?\s*default\s*\(\s*dynamic\s*\)\s*\)?$/.test(expression.trim());
}

function isCallIndexBase(expression) {
  const source = stripBalancedOuterParens(expression);
  return !/^new\s+/.test(source) && /^[A-Za-z_][A-Za-z0-9_.]*\s*\(/.test(source);
}

function stripIntIndexCast(operand) {
  return operand.replace(/^\(\s*int\s*\)\s*\((.*)\)$/s, "$1").trim();
}

function hasBalancedOuterParens(source) {
  let depth = 0;
  for (let index = 0; index < source.length; index += 1) {
    if (source[index] === "(") depth += 1;
    else if (source[index] === ")") depth -= 1;
    if (depth === 0 && index < source.length - 1) return false;
  }
  return depth === 0;
}

function stripBalancedOuterParens(source) {
  let value = source.trim();
  while (value.startsWith("(") && value.endsWith(")") && hasBalancedOuterParens(value)) {
    value = value.slice(1, -1).trim();
  }
  return value;
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
