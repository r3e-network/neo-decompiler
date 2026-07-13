import {
  convertTargetName,
  stripOuterParens,
  wrapExpression,
} from "./high-level-utils.js";

const BINARY_OPERATORS = {
  ADD: "+",
  SUB: "-",
  MUL: "*",
  DIV: "/",
  MOD: "%",
  AND: "&",
  OR: "|",
  XOR: "^",
  EQUAL: "==",
  NOTEQUAL: "!=",
  LT: "<",
  LE: "<=",
  GT: ">",
  GE: ">=",
  BOOLAND: "&&",
  BOOLOR: "||",
  NUMEQUAL: "==",
  NUMNOTEQUAL: "!=",
  CAT: "cat",
};

export function tryUnaryExpression(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  switch (mnemonic) {
    case "SQRT": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`sqrt(${value})`);
      return true;
    }
    case "NOT": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`!${wrapExpression(value)}`);
      return true;
    }
    case "INC": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`${wrapExpression(value)} + 1`);
      return true;
    }
    case "DEC": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`${wrapExpression(value)} - 1`);
      return true;
    }
    case "SUBSTR": {
      const count = stripOuterParens(state.stack.pop() ?? "???");
      const index = stripOuterParens(state.stack.pop() ?? "???");
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`substr(${value}, ${index}, ${count})`);
      return true;
    }
    case "CONVERT": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      const targetName = convertTargetName(instruction.operand);
      state.stack.push(
        targetName !== null ? `convert_to_${targetName}(${value})` : `convert(${value})`,
      );
      return true;
    }
    case "NEWBUFFER": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      const temp = `t${state.nextTempId}`;
      state.nextTempId += 1;
      state.statements.push(`let ${temp} = new_buffer(${value});`);
      state.stack.push(temp);
      return true;
    }
    case "NEWARRAY": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      const temp = `t${state.nextTempId}`;
      state.nextTempId += 1;
      state.statements.push(`let ${temp} = new_array(${value});`);
      state.stack.push(temp);
      return true;
    }
    case "NEGATE": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`-${wrapExpression(value)}`);
      return true;
    }
    case "ABS": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`abs(${value})`);
      return true;
    }
    case "SIGN": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`sign(${value})`);
      return true;
    }
    case "INVERT": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`~${wrapExpression(value)}`);
      return true;
    }
    case "ISNULL": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`is_null(${value})`);
      return true;
    }
    case "NZ": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`${wrapExpression(value)} != 0`);
      return true;
    }
    default:
      return false;
  }
}

export function tryBinaryExpression(stack, mnemonic) {
  const operator = BINARY_OPERATORS[mnemonic];
  if (operator) {
    const right = stack.pop() ?? "???";
    const left = stack.pop() ?? "???";
    stack.push(`${wrapExpression(left)} ${operator} ${wrapExpression(right)}`);
    return true;
  }
  switch (mnemonic) {
    case "POW": {
      const exponent = stack.pop() ?? "???";
      const base = stack.pop() ?? "???";
      stack.push(`pow(${base}, ${exponent})`);
      return true;
    }
    case "MODPOW": {
      const modulus = stack.pop() ?? "???";
      const exponent = stack.pop() ?? "???";
      const base = stack.pop() ?? "???";
      stack.push(`modpow(${base}, ${exponent}, ${modulus})`);
      return true;
    }
    case "MODMUL": {
      const modulus = stack.pop() ?? "???";
      const right = stack.pop() ?? "???";
      const left = stack.pop() ?? "???";
      stack.push(`modmul(${left}, ${right}, ${modulus})`);
      return true;
    }
    case "MAX": {
      const right = stack.pop() ?? "???";
      const left = stack.pop() ?? "???";
      stack.push(`max(${left}, ${right})`);
      return true;
    }
    case "MIN": {
      const right = stack.pop() ?? "???";
      const left = stack.pop() ?? "???";
      stack.push(`min(${left}, ${right})`);
      return true;
    }
    case "WITHIN": {
      const upper = stack.pop() ?? "???";
      const lower = stack.pop() ?? "???";
      const value = stack.pop() ?? "???";
      stack.push(`within(${value}, ${lower}, ${upper})`);
      return true;
    }
    case "LEFT": {
      const count = stack.pop() ?? "???";
      const value = stack.pop() ?? "???";
      stack.push(`left(${value}, ${count})`);
      return true;
    }
    case "RIGHT": {
      const count = stack.pop() ?? "???";
      const value = stack.pop() ?? "???";
      stack.push(`right(${value}, ${count})`);
      return true;
    }
    case "SHL": {
      const shift = stripOuterParens(stack.pop() ?? "???");
      const value = stripOuterParens(stack.pop() ?? "???");
      stack.push(`${wrapExpression(value)} << ${shift}`);
      return true;
    }
    case "SHR": {
      const shift = stripOuterParens(stack.pop() ?? "???");
      const value = stripOuterParens(stack.pop() ?? "???");
      stack.push(`${wrapExpression(value)} >> ${shift}`);
      return true;
    }
    default:
      return false;
  }
}
