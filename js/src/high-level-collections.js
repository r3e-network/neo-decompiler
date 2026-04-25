import {
  convertTargetName,
  resolvePackedValue,
  stripOuterParens,
  wrapExpression,
} from "./high-level-utils.js";

export function tryCollectionExpression(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;

  if (mnemonic === "NEWARRAY0") {
    state.stack.push("[]");
    return true;
  }
  if (mnemonic === "NEWARRAY_T") {
    const size = stripOuterParens(state.stack.pop() ?? "???");
    const targetName = convertTargetName(instruction.operand) ?? "unknown";
    state.stack.push(`new_array_t(${size}, "${targetName}")`);
    return true;
  }
  if (mnemonic === "NEWMAP") {
    state.stack.push("{}");
    return true;
  }
  if (mnemonic === "NEWSTRUCT0") {
    state.stack.push("{}");
    return true;
  }
  if (mnemonic === "NEWSTRUCT") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`new_struct(${value})`);
    return true;
  }
  if (mnemonic === "PACK" || mnemonic === "PACKMAP" || mnemonic === "PACKSTRUCT") {
    return emitPackExpression(state, mnemonic, stripOuterParens);
  }
  if (mnemonic === "PICKITEM") {
    const index = stripOuterParens(state.stack.pop() ?? "???");
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`${wrapExpression(target)}[${index}]`);
    return true;
  }
  if (mnemonic === "HASKEY") {
    const key = stripOuterParens(state.stack.pop() ?? "???");
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`has_key(${wrapExpression(target)}, ${key})`);
    return true;
  }
  if (mnemonic === "SIZE") {
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`len(${wrapExpression(target)})`);
    return true;
  }
  if (mnemonic === "KEYS") {
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`keys(${wrapExpression(target)})`);
    return true;
  }
  if (mnemonic === "VALUES") {
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`values(${wrapExpression(target)})`);
    return true;
  }
  if (mnemonic === "POPITEM") {
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.stack.push(`pop_item(${wrapExpression(target)})`);
    return true;
  }
  if (mnemonic === "ISTYPE") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    const targetName = convertTargetName(instruction.operand);
    if (targetName !== null) {
      state.stack.push(`is_type_${targetName}(${value})`);
    } else {
      state.stack.push(`is_type(${value})`);
    }
    return true;
  }
  if (mnemonic === "UNPACK") {
    const source = stripOuterParens(state.stack.pop() ?? "???");
    const packed = resolvePackedValue(state, source);
    if (!packed) {
      const elementCount = inferUnpackElementCount(state, instruction);
      const elementsTemp = `unpack(${source})`;
      state.statements.push(`let t${state.nextTempId} = ${elementsTemp};`);
      const unpackTemp = `t${state.nextTempId}`;
      state.nextTempId += 1;
      for (let index = 0; index < elementCount; index += 1) {
        const itemTemp = `t${state.nextTempId}`;
        state.nextTempId += 1;
        state.statements.push(
          `let ${itemTemp} = unpack_item(${unpackTemp}, ${index});`,
        );
        state.stack.push(itemTemp);
      }
      const countTemp = `t${state.nextTempId}`;
      state.nextTempId += 1;
      state.statements.push(`let ${countTemp} = len(${source});`);
      state.stack.push(countTemp);
      return true;
    }
    for (let i = packed.length - 1; i >= 0; i--) {
      state.stack.push(packed[i]);
    }
    state.stack.push(`${packed.length}`);
    return true;
  }
  return false;
}

export function tryCollectionStatement(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;

  if (mnemonic === "MEMCPY") {
    const args = [];
    for (let index = 0; index < 5; index += 1) {
      args.push(stripOuterParens(state.stack.pop() ?? "???"));
    }
    args.reverse();
    state.statements.push(`memcpy(${args.join(", ")});`);
    return true;
  }
  if (mnemonic === "SETITEM") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    const index = stripOuterParens(state.stack.pop() ?? "???");
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`${wrapExpression(target)}[${index}] = ${value};`);
    return true;
  }
  if (mnemonic === "APPEND") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`append(${wrapExpression(target)}, ${value});`);
    return true;
  }
  if (mnemonic === "REMOVE") {
    const key = stripOuterParens(state.stack.pop() ?? "???");
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`remove_item(${wrapExpression(target)}, ${key});`);
    return true;
  }
  if (mnemonic === "CLEARITEMS") {
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`clear_items(${wrapExpression(target)});`);
    return true;
  }
  if (mnemonic === "REVERSEITEMS") {
    const target = stripOuterParens(state.stack.pop() ?? "???");
    state.statements.push(`reverse_items(${wrapExpression(target)});`);
    return true;
  }
  return false;
}

function emitPackExpression(state, mnemonic, stripOuterParens) {
  const countText = state.stack.pop();
  const count = countText !== undefined ? Number.parseInt(countText, 10) : Number.NaN;
  if (!Number.isFinite(count) || count < 0) {
    state.stack.push(`pack_dynamic(${countText ?? "???"})`);
    return true;
  }

  const elements = [];
  for (let index = 0; index < count; index += 1) {
    elements.push(stripOuterParens(state.stack.pop() ?? "???"));
  }

  const expression = renderPackedExpression(mnemonic, elements);
  state.stack.push(expression);
  state.packedValuesByExpression.set(expression, [...elements]);
  return true;
}

function renderPackedExpression(mnemonic, elements) {
  const body = elements.join(", ");
  if (mnemonic === "PACKMAP") {
    return `Map(${body})`;
  }
  if (mnemonic === "PACKSTRUCT") {
    return `Struct(${body})`;
  }
  return `[${body}]`;
}

function inferUnpackElementCount(state, instruction) {
  const DEFAULT_COUNT = 4;
  if (!state.programIndexByOffset) {
    const map = new Map();
    for (let i = 0; i < state.program.length; i++) {
      map.set(state.program[i].offset, i);
    }
    state.programIndexByOffset = map;
  }
  const unpackIndex = state.programIndexByOffset.get(instruction.offset);
  if (unpackIndex === undefined) {
    return DEFAULT_COUNT;
  }

  let cursor = unpackIndex + 1;
  if (cursor >= state.program.length) {
    return DEFAULT_COUNT;
  }
  if (state.program[cursor].opcode.mnemonic !== "DROP") {
    return DEFAULT_COUNT;
  }
  cursor += 1;

  let pops = 0;
  while (
    cursor < state.program.length &&
    isSinglePopMnemonic(state.program[cursor].opcode.mnemonic)
  ) {
    pops += 1;
    cursor += 1;
  }

  if (pops === 0) {
    return DEFAULT_COUNT;
  }

  const hasDupBefore =
    unpackIndex > 0 && state.program[unpackIndex - 1].opcode.mnemonic === "DUP";
  const count = hasDupBefore ? Math.max(0, pops - 1) : pops;
  return count === 0 ? DEFAULT_COUNT : count;
}

function isSinglePopMnemonic(mnemonic) {
  return (
    mnemonic === "DROP" ||
    mnemonic.startsWith("STLOC") ||
    mnemonic.startsWith("STARG") ||
    mnemonic.startsWith("STSFLD")
  );
}
