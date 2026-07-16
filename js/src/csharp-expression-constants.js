// Fold only expressions whose complete value can be evaluated without VM
// objects, calls, indexing, or side effects. Unknown or fault-prone forms are
// deliberately left for the normal dynamic C# lowering path.

const PRECEDENCE = new Map([
  ["||", 1],
  ["&&", 2],
  ["|", 3],
  ["^", 4],
  ["&", 5],
  ["==", 6],
  ["!=", 6],
  ["===", 6],
  ["!==", 6],
  ["<", 7],
  ["<=", 7],
  [">", 7],
  [">=", 7],
  ["<<", 8],
  [">>", 8],
  ["+", 9],
  ["-", 9],
  ["*", 10],
  ["/", 10],
  ["%", 10],
]);

const OPERATORS = [
  "===", "!==", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||",
  "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "!", "~",
  "(", ")",
];

const CONTROL_PREFIXES = new Set([
  "abort",
  "assert",
  "case",
  "if",
  "return",
  "throw",
  "while",
]);

const MAX_SHIFT = 1024;

/**
 * Fold complete constant subexpressions embedded in a high-level source line.
 * The scanner preserves quoted strings and comments, and only accepts a
 * candidate at a statement/expression boundary so `value + 1 + 2` is not
 * partially regrouped.
 */
export function rewriteConstantExpressions(line) {
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
    if (line[index] === '"' || line[index] === "'") {
      index = quotedStringEnd(line, index);
      continue;
    }
    if (!isCandidateStart(line, index)) continue;

    const parsed = parseConstantPrefix(line, index);
    if (!parsed || parsed.end <= index || !hasOperator(line.slice(index, parsed.end))) continue;
    if (!validCandidateStart(line, index) || !validCandidateEnd(line, parsed.end)) continue;
    replacements.push({
      start: index,
      end: parsed.end,
      text: renderConstant(parsed.value),
    });
    index = parsed.end - 1;
  }

  if (replacements.length === 0) return line;
  let output = "";
  let cursor = 0;
  for (const replacement of replacements) {
    if (replacement.start < cursor) continue;
    output += line.slice(cursor, replacement.start) + replacement.text;
    cursor = replacement.end;
  }
  return output + line.slice(cursor);
}

/** Evaluate a complete constant expression, returning null when unsupported. */
export function foldConstantExpression(source) {
  const tokens = tokenize(source);
  if (tokens.length === 0) return null;
  const parser = new ConstantParser(tokens);
  const value = parser.parseExpression(0);
  if (value === null || !parser.atEnd()) return null;
  return renderConstant(value);
}

function parseConstantPrefix(source, start) {
  const tokens = tokenize(source.slice(start));
  if (tokens.length === 0) return null;
  const parser = new ConstantParser(tokens);
  const value = parser.parseExpression(0);
  if (value === null || parser.position === 0) return null;
  const token = tokens[parser.position - 1];
  return { value, end: start + token.end };
}

function tokenize(source) {
  const tokens = [];
  for (let index = 0; index < source.length;) {
    if (/\s/.test(source[index])) {
      index += 1;
      continue;
    }
    const start = index;
    const operator = OPERATORS.find((candidate) => source.startsWith(candidate, index));
    if (operator) {
      tokens.push({ type: "operator", value: operator, start, end: index + operator.length });
      index += operator.length;
      continue;
    }
    const number = source.slice(index).match(/^(?:0[xX][0-9a-fA-F]+|[0-9]+)/);
    if (number) {
      tokens.push({ type: "number", value: number[0], start, end: index + number[0].length });
      index += number[0].length;
      continue;
    }
    const word = source.slice(index).match(/^[A-Za-z_][A-Za-z0-9_]*/);
    if (word && (word[0] === "true" || word[0] === "false")) {
      tokens.push({ type: "boolean", value: word[0], start, end: index + word[0].length });
      index += word[0].length;
      continue;
    }
    break;
  }
  return tokens;
}

class ConstantParser {
  constructor(tokens) {
    this.tokens = tokens;
    this.position = 0;
  }

  atEnd() {
    return this.position === this.tokens.length;
  }

  peek() {
    return this.tokens[this.position] ?? null;
  }

  take() {
    const token = this.peek();
    if (token) this.position += 1;
    return token;
  }

  parseExpression(minPrecedence) {
    let left = this.parseUnary();
    if (left === null) return null;
    while (true) {
      const token = this.peek();
      const precedence = token?.type === "operator" ? PRECEDENCE.get(token.value) : undefined;
      if (precedence === undefined || precedence < minPrecedence) break;
      this.take();
      const right = this.parseExpression(precedence + 1);
      if (right === null) return null;
      left = applyBinary(token.value, left, right);
      if (left === null) return null;
    }
    return left;
  }

  parseUnary() {
    const token = this.peek();
    if (token?.type === "operator" && ["+", "-", "~", "!"].includes(token.value)) {
      this.take();
      const operand = this.parseUnary();
      return operand === null ? null : applyUnary(token.value, operand);
    }
    return this.parsePrimary();
  }

  parsePrimary() {
    const token = this.take();
    if (!token) return null;
    if (token.type === "number") {
      try {
        return { kind: "int", value: BigInt(token.value) };
      } catch {
        return null;
      }
    }
    if (token.type === "boolean") return { kind: "bool", value: token.value === "true" };
    if (token.value !== "(") return null;
    const value = this.parseExpression(0);
    const close = this.take();
    return close?.value === ")" ? value : null;
  }
}

function applyUnary(operator, operand) {
  if (operator === "!" && operand.kind === "bool") {
    return { kind: "bool", value: !operand.value };
  }
  if (operand.kind !== "int") return null;
  if (operator === "+") return operand;
  if (operator === "-") return { kind: "int", value: -operand.value };
  if (operator === "~") return { kind: "int", value: ~operand.value };
  return null;
}

function applyBinary(operator, left, right) {
  if (["==", "!=", "===", "!=="].includes(operator)) {
    if (left.kind !== right.kind) return null;
    const equal = left.value === right.value;
    return { kind: "bool", value: operator === "===" || operator === "==" ? equal : !equal };
  }
  if (["&&", "||"].includes(operator)) {
    if (left.kind !== "bool" || right.kind !== "bool") return null;
    return {
      kind: "bool",
      value: operator === "&&" ? left.value && right.value : left.value || right.value,
    };
  }
  if (left.kind !== "int" || right.kind !== "int") return null;
  switch (operator) {
    case "+": return { kind: "int", value: left.value + right.value };
    case "-": return { kind: "int", value: left.value - right.value };
    case "*": return { kind: "int", value: left.value * right.value };
    case "/": return right.value === 0n ? null : { kind: "int", value: left.value / right.value };
    case "%": return right.value === 0n ? null : { kind: "int", value: left.value % right.value };
    case "&": return { kind: "int", value: left.value & right.value };
    case "|": return { kind: "int", value: left.value | right.value };
    case "^": return { kind: "int", value: left.value ^ right.value };
    case "<": return { kind: "bool", value: left.value < right.value };
    case "<=": return { kind: "bool", value: left.value <= right.value };
    case ">": return { kind: "bool", value: left.value > right.value };
    case ">=": return { kind: "bool", value: left.value >= right.value };
    case "<<": return applyShift(left.value, right.value, false);
    case ">>": return applyShift(left.value, right.value, true);
    default: return null;
  }
}

function applyShift(left, right, signed) {
  if (right < 0n || right > BigInt(MAX_SHIFT)) return null;
  const count = Number(right);
  return { kind: "int", value: signed ? left >> BigInt(count) : left << BigInt(count) };
}

function renderConstant(value) {
  return value.kind === "bool" ? String(value.value) : value.value.toString();
}

function isCandidateStart(line, index) {
  const character = line[index];
  if (/[0-9]/.test(character) || character === "(") return true;
  if (character === "-" || character === "+" || character === "!" || character === "~") {
    return /[\s(0-9]/.test(line[index + 1] ?? "");
  }
  return line.startsWith("true", index) || line.startsWith("false", index);
}

function validCandidateStart(line, index) {
  let cursor = index - 1;
  while (cursor >= 0 && /\s/.test(line[cursor])) cursor -= 1;
  if (cursor < 0) return true;
  if ("=(:,[{};".includes(line[cursor])) return true;
  if (line[cursor] === "?") return true;
  if (/[+*/%&|^<>!-]/.test(line[cursor])) return false;
  let end = cursor + 1;
  while (cursor >= 0 && /[A-Za-z_]/.test(line[cursor])) cursor -= 1;
  const word = line.slice(cursor + 1, end);
  return CONTROL_PREFIXES.has(word);
}

function validCandidateEnd(line, end) {
  let cursor = end;
  while (cursor < line.length && /\s/.test(line[cursor])) cursor += 1;
  if (cursor >= line.length || line.startsWith("//", cursor) || line.startsWith("/*", cursor)) return true;
  return ";,)]}:".includes(line[cursor]) || line[cursor] === "{";
}

function hasOperator(source) {
  return /[()+\-*/%<>=!&|^]/.test(source);
}

function quotedStringEnd(line, start) {
  const quote = line[start];
  for (let index = start + 1; index < line.length; index += 1) {
    if (line[index] === "\\") {
      index += 1;
    } else if (line[index] === quote) {
      return index;
    }
  }
  return line.length - 1;
}
