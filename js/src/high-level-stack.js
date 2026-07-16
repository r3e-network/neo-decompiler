import {
  isPureRhs,
  literalIndex,
  resolvePackedValue,
  stripOuterParens,
} from "./high-level-utils.js";
export { tryBinaryExpression, tryUnaryExpression } from "./high-level-expressions.js";

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
      if (
        state.previousInstruction?.opcode?.mnemonic === "DROP" &&
        state.lastDroppedValue !== undefined
      ) {
        state.statements.push("throw();");
        state.stack.length = 0;
        state.terminated = true;
        return true;
      }
      const value = stripOuterParens(state.stack.pop() ?? "???");
      state.statements.push(`throw(${value});`);
      state.stack.length = 0;
      state.terminated = true;
      return true;
    }
    case "ABORT":
      state.statements.push("abort();");
      state.stack.length = 0;
      state.terminated = true;
      return true;
    case "ABORTMSG": {
      const msg = stripOuterParens(state.stack.pop() ?? "???");
      state.statements.push(`abort(${msg});`);
      state.stack.length = 0;
      state.terminated = true;
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
    case "DROP": {
      const top = state.stack.pop();
      state.lastDroppedValue = top;
      // A discarded value-returning CALL/SYSCALL/CALLT (or any fault-capable
      // expression) has an observable effect, so it stays visible even though
      // the result is unused — mirroring Rust, which materialises every such
      // op into a `let tN = …;` that survives the drop. Pure values (literals,
      // identifiers, arithmetic, pure-helper calls) are dropped silently.
      if (top !== undefined && !isPureRhs(top)) {
        const temp = `t${state.nextTempId}`;
        state.nextTempId += 1;
        state.statements.push(`let ${temp} = ${stripOuterParens(top)};`);
      }
      return true;
    }
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
      const index = literalIndex(indexText);
      if (!Number.isFinite(index) || index < 0 || index >= state.stack.length) {
        const temp = `t${state.nextTempId}`;
        state.nextTempId += 1;
        state.statements.push(`let ${temp} = pick(${indexText ?? "???"});`);
        state.stack.push(temp);
        return true;
      }
      // Materialise the picked slot into a temp before duplicating, the
      // same hazard DUP/OVER guard against: a side-effecting expression
      // (CALL/SYSCALL/CALLT result) copied by string would evaluate twice.
      // Simple identifiers/literals are left as-is (skip-on-literal).
      const sourceSlot = state.stack.length - 1 - index;
      const source = materialiseStackTopForDup(state, state.stack[sourceSlot], sourceSlot);
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
        // Materialise the top into a temp before tucking a copy below the
        // second item — same double-evaluation hazard as DUP/OVER/PICK.
        // Simple identifiers/literals are left as-is (skip-on-literal).
        const top = materialiseStackTopForDup(state, state.stack[state.stack.length - 1]);
        state.stack.splice(state.stack.length - 2, 0, top);
      }
      return true;
    case "ROLL": {
      const indexText = state.stack.pop();
      const index = literalIndex(indexText);
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
      const count = literalIndex(countText);
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
      const index = literalIndex(indexText);
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
