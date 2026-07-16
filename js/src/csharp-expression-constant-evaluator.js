// Evaluate only complete expressions made from literals and pure operators.
// Unknown or fault-prone forms return null so callers can keep the normal
// dynamic C# lowering path.

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

const MAX_SHIFT = 1024;

/** Evaluate a complete constant expression, returning null when unsupported. */
export function foldConstantExpression(source) {
  const tokens = tokenize(source);
  if (tokens.length === 0) return null;
  const parser = new ConstantParser(tokens);
  const value = parser.parseExpression(0);
  if (value === null || !parser.atEnd()) return null;
  return renderConstant(value);
}

/**
 * Parse the longest literal-only expression prefix beginning at `start`.
 * Unlike `foldConstantExpression`, trailing source is allowed so a caller can
 * enforce statement and delimiter boundaries itself.
 */
export function parseConstantPrefix(source, start) {
  const tokens = tokenize(source.slice(start));
  if (tokens.length === 0) return null;
  const parser = new ConstantParser(tokens);
  const value = parser.parseExpression(0);
  if (value === null || parser.position === 0) return null;
  const token = tokens[parser.position - 1];
  return {
    text: renderConstant(value),
    end: start + token.end,
  };
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
