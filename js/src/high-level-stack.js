import { convertTargetName, resolvePackedValue, stripOuterParens, wrapExpression } from "./high-level-utils.js";

// Use `==` / `!=` (rather than JavaScript's `===` / `!==`) for parity
// with the Rust port's high-level emitter — those forms also lower
// cleanly to both Rust and C# without further rewriting. The `cat`
// pseudo-operator stays as-is in the high-level surface; the C#
// emitter is responsible for translating it to `+`.
// `arg0`, `loc12`, `static3`, `t7` and bare integer literals are safe
// to duplicate by string copy. Anything else (a call, a freshly-built
// arithmetic expression, an indexing access) carries either side
// effects or a precedence hazard, so DUP/OVER must materialise it
// into a temp before pushing the second copy.
const SIMPLE_IDENT_OR_LITERAL_RE = /^(?:-?\d+|0x[0-9A-Fa-f]+|true|false|null|"(?:[^"\\]|\\.)*"|[A-Za-z_][A-Za-z0-9_]*)$/u;

/**
 * If `value` (currently sitting on the stack at index `slot`, defaulting
 * to the top) is a complex expression, hoist it into a `let tN = ...;`
 * statement and return the temp identifier — also patching the existing
 * stack slot so the original consumer stays in sync. Returns `value`
 * unchanged for simple identifiers and literals.
 */
function materialiseStackTopForDup(state, value, slot = state.stack.length - 1) {
  if (SIMPLE_IDENT_OR_LITERAL_RE.test(value)) {
    return value;
  }
  const temp = `t${state.nextTempId}`;
  state.nextTempId += 1;
  state.statements.push(`let ${temp} = ${value};`);
  state.stack[slot] = temp;
  return temp;
}

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

export function tryControlStatement(state, instruction) {
  switch (instruction.opcode.mnemonic) {
    case "ASSERT": {
      const condition = stripOuterParens(state.stack.pop() ?? "???");
      state.statements.push(`assert(${condition});`);
      return true;
    }
    case "ASSERTMSG": {
      const message = stripOuterParens(state.stack.pop() ?? "???");
      const condition = stripOuterParens(state.stack.pop() ?? "???");
      state.statements.push(`assert(${condition}, ${message});`);
      return true;
    }
    case "THROW": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.statements.push(`throw(${value});`);
      state.stack.length = 0;
      return true;
    }
    case "ABORT":
      state.statements.push("abort();");
      state.stack.length = 0;
      return true;
    case "ABORTMSG": {
      const msg = stripOuterParens(state.stack.pop() ?? "???");
      state.statements.push(`abort(${msg});`);
      state.stack.length = 0;
      return true;
    }
    default:
      return false;
  }
}

export function tryStackShapeOperation(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  switch (mnemonic) {
    case "DEPTH":
      state.stack.push(`${state.stack.length}`);
      return true;
    case "DROP":
      state.stack.pop();
      return true;
    case "CLEAR":
      state.stack.length = 0;
      state.statements.push("// clear stack");
      return true;
    case "DUP": {
      const top = state.stack.at(-1);
      if (top === undefined) {
        state.stack.push("???");
      } else {
        // If the top-of-stack is anything other than a plain
        // identifier — typically a CALL/SYSCALL/CALLT/CALLA result
        // expression with side effects, or a freshly-built
        // arithmetic expression — materialise it into a temp before
        // duplicating. Otherwise the lifted output ends up with two
        // independent copies of the same call (`syscall(...)`,
        // `sub_0xNNNN(...)`) where the bytecode evaluated it once.
        const materialised = materialiseStackTopForDup(state, top);
        state.stack.push(materialised);
      }
      return true;
    }
    case "OVER": {
      if (state.stack.length < 2) {
        state.stack.push("???");
        return true;
      }
      // Same hazard as DUP: copying the second-from-top expression
      // by string yields two evaluations of the underlying call.
      const idx = state.stack.length - 2;
      const materialised = materialiseStackTopForDup(state, state.stack[idx], idx);
      state.stack.push(materialised);
      return true;
    }
    case "SWAP": {
      if (state.stack.length >= 2) {
        const last = state.stack.length - 1;
        [state.stack[last - 1], state.stack[last]] = [state.stack[last], state.stack[last - 1]];
      }
      return true;
    }
    case "NIP":
      if (state.stack.length >= 2) {
        state.stack.splice(state.stack.length - 2, 1);
      }
      return true;
    case "PICK": {
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
    case "ROT":
      if (state.stack.length >= 3) {
        const [a, b, c] = state.stack.splice(state.stack.length - 3, 3);
        state.stack.push(b, c, a);
      }
      state.statements.push("// rotate top three stack values");
      return true;
    case "TUCK":
      if (state.stack.length >= 2) {
        const top = state.stack[state.stack.length - 1];
        state.stack.splice(state.stack.length - 2, 0, top);
      }
      state.statements.push("// tuck top of stack");
      return true;
    case "ROLL": {
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
    case "REVERSE3":
      if (state.stack.length >= 3) {
        const stack = state.stack;
        const last = stack.length - 1;
        const tmp = stack[last - 2];
        stack[last - 2] = stack[last];
        stack[last] = tmp;
      }
      state.statements.push("// reverse top 3 stack values");
      return true;
    case "REVERSE4":
      if (state.stack.length >= 4) {
        const stack = state.stack;
        const last = stack.length - 1;
        let tmp = stack[last - 3];
        stack[last - 3] = stack[last];
        stack[last] = tmp;
        tmp = stack[last - 2];
        stack[last - 2] = stack[last - 1];
        stack[last - 1] = tmp;
      }
      state.statements.push("// reverse top 4 stack values");
      return true;
    case "REVERSEN": {
      const countText = state.stack.pop();
      const count = countText !== undefined ? Number.parseInt(countText, 10) : Number.NaN;
      if (Number.isFinite(count) && count >= 0 && count <= state.stack.length) {
        const stack = state.stack;
        let i = stack.length - count;
        let j = stack.length - 1;
        while (i < j) {
          const tmp = stack[i];
          stack[i] = stack[j];
          stack[j] = tmp;
          i++;
          j--;
        }
        state.statements.push(`// reverse top ${count} stack values`);
      } else {
        state.statements.push(`// reverse top ${countText ?? "???"} stack values`);
      }
      return true;
    }
    case "XDROP": {
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
    default:
      return false;
  }
}

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
      state.stack.push(`!${value}`);
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
    // NEWBUFFER / NEWARRAY / NEWARRAY_T allocate a fresh container that
    // bytecode commonly mutates via DUP + SETITEM / APPEND. Pushing the
    // call expression onto the operand stack means DUP duplicates the
    // string, and the lifted output ends up with two independent
    // allocations (`new_array(3)[0] = 8; return new_array(3);`).
    // Materialise into a temp so all downstream stack references
    // resolve to the same identifier — mirrors the NEWMAP / NEWARRAY0 /
    // NEWSTRUCT0 fix in `high-level-collections.js`.
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
    // NEWSTRUCT handled by tryCollectionExpression in high-level-collections.js
    case "NEGATE": {
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.stack.push(`-${value}`);
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
      state.stack.push(`~${value}`);
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
