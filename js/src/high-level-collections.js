import {
  convertTargetName,
  resolvePackedValue,
  stripOuterParens,
  wrapExpression,
} from "./high-level-utils.js";

export function tryCollectionExpression(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;

  // NEWARRAY0 / NEWMAP / NEWSTRUCT0 each create a fresh container that
  // the bytecode then mutates (via DUP + SETITEM, APPEND, etc.). Pushing
  // the bare literal `[]` / `{}` onto the operand stack means every DUP
  // produces an *independent* literal in the rendered output, so a
  // `NEWMAP DUP "k" "v" SETITEM RET` lift comes out as
  // `{}["k"] = "v"; return {};` (two separate empty maps). Materialise
  // the value into a temp so all stack references resolve to the same
  // identifier.
  if (mnemonic === "NEWARRAY0") {
    const temp = `t${state.nextTempId}`;
    state.nextTempId += 1;
    state.statements.push(`let ${temp} = [];`);
    state.stack.push(temp);
    return true;
  }
  if (mnemonic === "NEWARRAY_T") {
    const size = stripOuterParens(state.stack.pop() ?? "???");
    const targetName = convertTargetName(instruction.operand);
    // Unrecognized StackItemType bytes surface as the raw byte (0xNN),
    // matching the ISTYPE fallback and the Rust port, so the reader can
    // see which type the bytecode actually requested.
    const typeText =
      targetName !== null ? `"${targetName}"` : formatTypeByte(instruction.operand);
    const temp = `t${state.nextTempId}`;
    state.nextTempId += 1;
    state.statements.push(`let ${temp} = new_array_t(${size}, ${typeText});`);
    state.stack.push(temp);
    return true;
  }
  if (mnemonic === "NEWMAP") {
    // `Map()` (not `{}`) — matches the Rust emitter and keeps the
    // map/struct distinction visible in the lifted source.
    const temp = `t${state.nextTempId}`;
    state.nextTempId += 1;
    state.statements.push(`let ${temp} = Map();`);
    state.stack.push(temp);
    return true;
  }
  if (mnemonic === "NEWSTRUCT0") {
    // `Struct()` (not `{}`) — matches the Rust emitter.
    const temp = `t${state.nextTempId}`;
    state.nextTempId += 1;
    state.statements.push(`let ${temp} = Struct();`);
    state.stack.push(temp);
    return true;
  }
  if (mnemonic === "NEWSTRUCT") {
    const value = stripOuterParens(state.stack.pop() ?? "???");
    const temp = `t${state.nextTempId}`;
    state.nextTempId += 1;
    state.statements.push(`let ${temp} = new_struct(${value});`);
    state.stack.push(temp);
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
    } else if (instruction.operand !== null) {
      // Keep the raw type byte (0xNN) so the reader can tell which type
      // is being tested — mirrors the Rust port's fallback.
      state.stack.push(`is_type(${value}, ${formatTypeByte(instruction.operand)})`);
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

// Render an unrecognized StackItemType operand byte as uppercase hex
// (`0xNN`), byte-identical to Rust's `format_type_operand` fallback.
function formatTypeByte(operand) {
  const byte =
    operand.kind === "U8" ? operand.value : operand.kind === "I8" ? operand.value & 0xff : null;
  if (byte === null) {
    return "unknown";
  }
  return `0x${byte.toString(16).padStart(2, "0").toUpperCase()}`;
}

// Hard cap on the number of elements rendered inline for PACK/PACKMAP/
// PACKSTRUCT. Hand-written contracts almost never PACK more than a few
// dozen items at once, while pathological or malformed inputs can drive
// the count into the thousands and produce KB-sized expressions packed
// with `???` underflow markers. When the requested count exceeds this
// cap, render as `pack_n(value0, value1, ..., /* N more elements */)`
// or fall back to a `pack_dynamic(N)` placeholder.
const PACK_MAX_INLINE = 64;

function emitPackExpression(state, mnemonic, stripOuterParens) {
  const countText = state.stack.pop();
  const count = countText !== undefined ? Number.parseInt(countText, 10) : Number.NaN;
  if (!Number.isFinite(count) || count < 0) {
    state.stack.push(`pack_dynamic(${countText ?? "???"})`);
    return true;
  }

  // PACKMAP pops a key/value PAIR per entry — `Pop: 2n+1 item(s)` per
  // OpCode.cs — with the key popped before its value; PACK/PACKSTRUCT
  // pop one item per entry (`Pop: n+1`).
  const isMap = mnemonic === "PACKMAP";
  const unit = isMap ? 2 : 1;

  // Drain only as many concrete stack entries as the stack actually has
  // (capped at PACK_MAX_INLINE entries). When the caller asked for more
  // than the stack can supply, render the remainder as a single
  // underflow note rather than emitting N copies of `???` per missing
  // slot.
  const drainCount = Math.min(count, Math.floor(state.stack.length / unit), PACK_MAX_INLINE);
  const rendered = [];
  for (let index = 0; index < drainCount; index += 1) {
    if (isMap) {
      const key = stripOuterParens(state.stack.pop() ?? "???");
      const value = stripOuterParens(state.stack.pop() ?? "???");
      rendered.push(`${key}: ${value}`);
    } else {
      rendered.push(stripOuterParens(state.stack.pop() ?? "???"));
    }
  }
  const remainder = count - drainCount;
  if (remainder > 0) {
    const noun = isMap
      ? remainder === 1 ? "entry" : "entries"
      : remainder === 1 ? "element" : "elements";
    rendered.push(`/* ${remainder} more ${noun} */`);
    // The cap above bounds only how many entries are RENDERED inline; the VM
    // still pops every element the count names. Drain the elided units from the
    // simulated stack (bounded by its actual depth) so subsequent instructions
    // bind the right operands — mirrors the Rust port
    // (high_level/emitter/stack/expressions/collections.rs).
    let excess = remainder * unit;
    while (excess > 0 && state.stack.length > 0) {
      state.stack.pop();
      excess -= 1;
    }
  }

  const expression = renderPackedExpression(mnemonic, rendered);
  state.stack.push(expression);
  // UNPACK of a map pushes key/value pairs plus the ENTRY count
  // (`Push: 2n+1`, OpCode.cs) — the flat element replay models
  // arrays/structs only, so skip tracking for maps and let UNPACK take
  // its honest unknown-source path (mirrors the Rust port).
  if (!isMap) {
    state.packedValuesByExpression.set(expression, [...rendered]);
  }
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
