import { convertTargetName, resolvePackedValue, stripOuterParens, wrapExpression } from "./high-level-utils.js";

const BINARY_OPERATORS = {
  ADD: "+",
  SUB: "-",
  MUL: "*",
  DIV: "/",
  MOD: "%",
  AND: "&",
  OR: "|",
  XOR: "^",
  EQUAL: "===",
  NOTEQUAL: "!==",
  LT: "<",
  LE: "<=",
  GT: ">",
  GE: ">=",
  BOOLAND: "&&",
  BOOLOR: "||",
  NUMEQUAL: "===",
  NUMNOTEQUAL: "!==",
  CAT: "cat",
};

export function tryControlStatement(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  if (mnemonic === "ASSERT") {
    const condition = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`assert(${condition});`);
    return true;
  }
  if (mnemonic === "ASSERTMSG") {
    const message = stripOuterParens(state.stack.pop() ?? "???");
    const condition = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`assert(${condition}, ${message});`);
    return true;
  }
  if (mnemonic === "THROW") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`throw(${value});`);
    state.stack.length = 0;
    return true;
  }
  if (mnemonic === "ABORT") {
    state.statements.push("abort();");
    state.stack.length = 0;
    return true;
  }
  if (mnemonic === "ABORTMSG") {
    const msg = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`abort(${msg});`);
    state.stack.length = 0;
    return true;
  }
  return false;
}

export function tryStackShapeOperation(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  if (mnemonic === "DEPTH") {
    state.stack.push(`${state.stack.length}`);
    return true;
  }
  if (mnemonic === "DROP") {
    state.stack.pop();
    return true;
  }
  if (mnemonic === "CLEAR") {
    state.stack.length = 0;
    state.statements.push("// clear stack");
    return true;
  }
  if (mnemonic === "DUP") {
    const top = state.stack.at(-1);
    if (top !== undefined) {
      state.stack.push(top);
    } else {
      state.stack.push("/* stack_underflow */");
    }
    return true;
  }
  if (mnemonic === "OVER") {
    const value = state.stack.length >= 2 ? state.stack[state.stack.length - 2] : "/* stack_underflow */";
    state.stack.push(value);
    return true;
  }
  if (mnemonic === "SWAP") {
    if (state.stack.length >= 2) {
      const last = state.stack.length - 1;
      [state.stack[last - 1], state.stack[last]] = [state.stack[last], state.stack[last - 1]];
    }
    return true;
  }
  if (mnemonic === "NIP") {
    if (state.stack.length >= 2) {
      state.stack.splice(state.stack.length - 2, 1);
    }
    return true;
  }
  if (mnemonic === "PICK") {
    const indexText = state.stack.pop();
    const index = indexText !== undefined ? Number.parseInt(indexText, 10) : Number.NaN;
    if (!Number.isFinite(index) || index < 0 || index >= state.stack.length) {
      const temp = `t${state.nextTempId}`;
      state.nextTempId += 1;
      state.statements.push(`let ${temp} = pick(${indexText ?? "???"});`);
      state.stack.push(temp);
      return true;
    }
    const source = state.stack[state.stack.length - 1 - index];
    state.stack.push(source);
    const packed = resolvePackedValue(state, source);
    if (packed) {
      state.packedValuesByExpression.set(source, packed);
    }
    return true;
  }
  if (mnemonic === "ROT") {
    if (state.stack.length >= 3) {
      const [a, b, c] = state.stack.splice(state.stack.length - 3, 3);
      state.stack.push(b, c, a);
    }
    state.statements.push("// rotate top three stack values");
    return true;
  }
  if (mnemonic === "TUCK") {
    if (state.stack.length >= 2) {
      const top = state.stack[state.stack.length - 1];
      state.stack.splice(state.stack.length - 2, 0, top);
    }
    state.statements.push("// tuck top of stack");
    return true;
  }
  if (mnemonic === "ROLL") {
    const indexText = state.stack.pop();
    const index = indexText !== undefined ? Number.parseInt(indexText, 10) : Number.NaN;
    if (Number.isFinite(index) && index >= 0 && index < state.stack.length) {
      const from = state.stack.length - 1 - index;
      const [value] = state.stack.splice(from, 1);
      state.stack.push(value);
    } else {
      const temp = `t${state.nextTempId}`;
      state.nextTempId += 1;
      state.statements.push(`let ${temp} = roll(${indexText ?? "???"}); // dynamic roll`);
      state.stack.push(temp);
    }
    return true;
  }
  if (mnemonic === "REVERSE3") {
    if (state.stack.length >= 3) {
      const start = state.stack.length - 3;
      state.stack.splice(start, 3, ...state.stack.slice(start).reverse());
    }
    state.statements.push("// reverse top 3 stack values");
    return true;
  }
  if (mnemonic === "REVERSE4") {
    if (state.stack.length >= 4) {
      const start = state.stack.length - 4;
      state.stack.splice(start, 4, ...state.stack.slice(start).reverse());
    }
    state.statements.push("// reverse top 4 stack values");
    return true;
  }
  if (mnemonic === "REVERSEN") {
    const countText = state.stack.pop();
    const count = countText !== undefined ? Number.parseInt(countText, 10) : Number.NaN;
    if (Number.isFinite(count) && count >= 0 && count <= state.stack.length) {
      const start = state.stack.length - count;
      state.stack.splice(start, count, ...state.stack.slice(start).reverse());
      state.statements.push(`// reverse top ${count} stack values`);
    } else {
      state.statements.push(`// reverse top ${countText ?? "???"} stack values`);
    }
    return true;
  }
  if (mnemonic === "XDROP") {
    const indexText = state.stack.pop();
    const index = indexText !== undefined ? Number.parseInt(indexText, 10) : Number.NaN;
    if (Number.isFinite(index) && index >= 0 && index < state.stack.length) {
      const removeAt = state.stack.length - 1 - index;
      state.stack.splice(removeAt, 1);
    } else {
      state.statements.push(`// xdrop stack[${indexText ?? "???"}] (dynamic index, stack may be imprecise)`);
    }
    return true;
  }
  return false;
}

export function tryUnaryExpression(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  if (mnemonic === "SQRT") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`sqrt(${value})`);
    return true;
  }
  if (mnemonic === "NOT") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`!${value}`);
    return true;
  }
  if (mnemonic === "INC") {
    const value = stripOuterParens(state.stack.pop() ?? "/* stack_underflow */");
    state.stack.push(`${wrapExpression(value)} + 1`);
    return true;
  }
  if (mnemonic === "DEC") {
    const value = stripOuterParens(state.stack.pop() ?? "/* stack_underflow */");
    state.stack.push(`${wrapExpression(value)} - 1`);
    return true;
  }
  if (mnemonic === "SUBSTR") {
    const count = stripOuterParens(state.stack.pop() ?? "???");
    const index = stripOuterParens(state.stack.pop() ?? "???");
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`substr(${value}, ${index}, ${count})`);
    return true;
  }
  if (mnemonic === "CONVERT") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    const targetName = convertTargetName(instruction.operand);
    if (targetName !== null) {
      state.stack.push(`convert_to_${targetName}(${value})`);
    } else {
      state.stack.push(`convert(${value})`);
    }
    return true;
  }
  if (mnemonic === "NEWBUFFER") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`new_buffer(${value})`);
    return true;
  }
  if (mnemonic === "NEWARRAY") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`new_array(${value})`);
    return true;
  }
  // NEWSTRUCT handled by tryCollectionExpression in high-level-collections.js
  if (mnemonic === "NEGATE") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`-${value}`);
    return true;
  }
  if (mnemonic === "ABS") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`abs(${value})`);
    return true;
  }
  if (mnemonic === "SIGN") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`sign(${value})`);
    return true;
  }
  if (mnemonic === "INVERT") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`~${value}`);
    return true;
  }
  if (mnemonic === "ISNULL") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`is_null(${value})`);
    return true;
  }
  if (mnemonic === "NZ") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`${wrapExpression(value)} != 0`);
    return true;
  }
  return false;
}

export function tryBinaryExpression(stack, mnemonic) {
  const operator = BINARY_OPERATORS[mnemonic];
  if (operator) {
    const right = stack.pop() ?? "/* stack_underflow */";
    const left = stack.pop() ?? "/* stack_underflow */";
    stack.push(`${wrapExpression(left)} ${operator} ${wrapExpression(right)}`);
    return true;
  }
  if (mnemonic === "POW") {
    const exponent = stack.pop() ?? "???";
    const base = stack.pop() ?? "???";
    stack.push(`pow(${base}, ${exponent})`);
    return true;
  }
  if (mnemonic === "MODPOW") {
    const modulus = stack.pop() ?? "???";
    const exponent = stack.pop() ?? "???";
    const base = stack.pop() ?? "???";
    stack.push(`modpow(${base}, ${exponent}, ${modulus})`);
    return true;
  }
  if (mnemonic === "MODMUL") {
    const modulus = stack.pop() ?? "???";
    const right = stack.pop() ?? "???";
    const left = stack.pop() ?? "???";
    stack.push(`modmul(${left}, ${right}, ${modulus})`);
    return true;
  }
  if (mnemonic === "MAX") {
    const right = stack.pop() ?? "???";
    const left = stack.pop() ?? "???";
    stack.push(`max(${left}, ${right})`);
    return true;
  }
  if (mnemonic === "MIN") {
    const right = stack.pop() ?? "???";
    const left = stack.pop() ?? "???";
    stack.push(`min(${left}, ${right})`);
    return true;
  }
  if (mnemonic === "WITHIN") {
    const upper = stack.pop() ?? "???";
    const lower = stack.pop() ?? "???";
    const value = stack.pop() ?? "???";
    stack.push(`within(${value}, ${lower}, ${upper})`);
    return true;
  }
  if (mnemonic === "LEFT") {
    const count = stack.pop() ?? "???";
    const value = stack.pop() ?? "???";
    stack.push(`left(${value}, ${count})`);
    return true;
  }
  if (mnemonic === "RIGHT") {
    const count = stack.pop() ?? "???";
    const value = stack.pop() ?? "???";
    stack.push(`right(${value}, ${count})`);
    return true;
  }
  if (mnemonic === "SHL") {
    const shift = stripOuterParens(stack.pop() ?? "???");
    const value = stripOuterParens(stack.pop() ?? "???");
    stack.push(`${wrapExpression(value)} << ${shift}`);
    return true;
  }
  if (mnemonic === "SHR") {
    const shift = stripOuterParens(stack.pop() ?? "???");
    const value = stripOuterParens(stack.pop() ?? "???");
    stack.push(`${wrapExpression(value)} >> ${shift}`);
    return true;
  }
  return false;
}
