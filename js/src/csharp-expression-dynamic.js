import {
  isInsideQuotedString,
  nextOutsideMatch,
} from "./csharp-expression-scanner.js";
import {
  findMatchingOpen,
  scanIdentifierPathStart,
} from "./csharp-expression-lexical.js";

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
  const rewritten = cursor === 0 ? line : output + line.slice(cursor);
  return rewriteDynamicCompoundOperators(rewritten, types);
}

// Dynamic lowering must not leave a cast expression on the left-hand side of
// a compound assignment (`((dynamic)(value)) += 1` is not assignable in C#).
// Rebind the original slot while keeping the VM arithmetic dynamic.
export function rewriteDynamicCompoundAssignments(line) {
  const match = line.match(
    /^(\s*)(@?[A-Za-z_][A-Za-z0-9_]*)\s*(<<|>>|[+\-*/%&|^])=\s*(.+?);\s*(\/\/.*)?$/,
  );
  if (!match) return line;
  const [, indentation, target, operator, value, comment = ""] = match;
  return `${indentation}${target} = ((dynamic)(${target})) ${operator} ${value};${comment ? ` ${comment}` : ""}`;
}

// The fast path above intentionally handles the common identifier form. VM
// values also arrive through nested calls, indexes, and parenthesized
// comparisons, which cannot be matched safely with one regular expression.
// Find those complete left operands lexically and bind them dynamically when
// their static C# operator pair is not trustworthy.
function rewriteDynamicCompoundOperators(line, types) {
  const replacements = [];
  let blockComment = false;
  for (let index = 0; index < line.length; index += 1) {
    if (blockComment) {
      if (line.startsWith("*/", index)) {
        blockComment = false;
        index += 1;
      }
      continue;
    }
    if (line.startsWith("//", index)) break;
    if (line.startsWith("/*", index)) {
      blockComment = true;
      index += 1;
      continue;
    }
    if (isInsideQuotedString(line, index)) continue;
    const operator = operatorAt(line, index);
    if (!operator) continue;
    const start = findLeftOperandStart(line, index);
    if (start < 0) continue;
    const left = line.slice(start, index).trim();
    if (!left || /^\(*\s*\(?\s*dynamic\s*\)/.test(left)) continue;
    if (!isDynamicCompoundOperand(left, types)) continue;
    replacements.push({
      start,
      end: index,
      text: `((dynamic)(${left})) `,
    });
  }
  if (replacements.length === 0) return line;

  // Inner operators can produce overlapping candidates with an outer
  // parenthesized expression. Keep the widest non-overlapping candidate so
  // `(a > b) - (c > d)` becomes one dynamic subtraction expression.
  replacements.sort((left, right) =>
    left.start - right.start || right.end - left.end,
  );
  const selected = [];
  for (const replacement of replacements) {
    if (selected.some((entry) => replacement.start < entry.end && entry.start < replacement.end)) {
      continue;
    }
    selected.push(replacement);
  }
  selected.sort((left, right) => left.start - right.start);
  let output = "";
  let cursor = 0;
  for (const replacement of selected) {
    output += line.slice(cursor, replacement.start) + replacement.text;
    cursor = replacement.end;
  }
  return output + line.slice(cursor);
}

function operatorAt(line, index) {
  for (const candidate of ["===", "!==", "==", "!=", "<=", ">=", "<<", ">>"]) {
    if (line.startsWith(candidate, index)) return candidate;
  }
  if ("<>+-*/%&|^".includes(line[index])) return line[index];
  return null;
}

function findLeftOperandStart(line, operatorIndex) {
  let end = operatorIndex - 1;
  while (end >= 0 && /\s/.test(line[end])) end -= 1;
  if (end < 0) return -1;
  if (line[end] === ")") {
    const open = findMatchingOpen(line, end, "(", ")");
    if (open < 0) return -1;
    let start = open;
    let prefix = open - 1;
    while (prefix >= 0 && /\s/.test(line[prefix])) prefix -= 1;
    if (prefix >= 0 && /[A-Za-z0-9_.]/.test(line[prefix])) {
      const candidate = scanIdentifierPathStart(line, prefix);
      const name = line.slice(candidate, open).trim();
      if (!/^(?:if|while|for|switch|catch|return|throw)$/.test(name)) start = candidate;
    }
    return start;
  }
  if (line[end] === "]") {
    const open = findMatchingOpen(line, end, "[", "]");
    if (open < 0) return -1;
    let prefix = open - 1;
    while (prefix >= 0 && /\s/.test(line[prefix])) prefix -= 1;
    return prefix >= 0 ? scanIdentifierPathStart(line, prefix) : open;
  }
  if (/[A-Za-z0-9_@]/.test(line[end])) return scanIdentifierPathStart(line, end);
  return -1;
}

function isDynamicCompoundOperand(left, types) {
  const normalized = left.replace(/\s+/g, "");
  const type = expressionType(left, types);
  if (isDynamicOperatorType(type)) return true;
  if (/(?:===|!==|==|!=|<=|>=|(?<!<)<(?!<)|(?<!>)>(?!>))/.test(left)) return true;
  if (!/[([]/.test(left)) return false;
  // Keep ordinary numeric framework calls statically typed where possible.
  if (/^(?:BigInteger|Math)\./.test(normalized)) return false;
  return true;
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
