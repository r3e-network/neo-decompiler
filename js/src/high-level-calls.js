import { SYSCALLS } from "./generated/syscalls.js";
import { jumpTarget, stripOuterParens } from "./high-level-utils.js";
import { hex16, hex32, hexOffset } from "./util.js";

const MAX_RENDERED_CALL_ARGUMENTS = 256;

export function tryInternalCall(state, instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  if (mnemonic !== "CALL" && mnemonic !== "CALL_L") {
    return false;
  }
  const target = jumpTarget(instruction);
  if (target === null) {
    return false;
  }

  // Match Rust's `call_0x{target:04X}` fallback when the target isn't
  // resolvable through `methodLabelsByOffset`: distinct prefix
  // (`call_` vs `sub_`) signals "unresolved internal call", and
  // uppercase hex matches both the offset suffix and Rust's format.
  // Earlier this used `sub_0x` with lowercase digits, conflating
  // OOB/unknown calls with regular helper definitions.
  const callee =
    state.context.methodLabelsByOffset.get(target) ?? `call_0x${hexOffset(target)}`;
  const argCount = state.context.methodArgCountsByOffset.get(target) ?? 0;
  const args = popCallArguments(state, instruction, callee, argCount);
  emitInternalCallResult(state, target, `${callee}(${args.join(", ")})`);
  return true;
}

export function tryIndirectCall(state, instruction) {
  if (instruction.opcode.mnemonic !== "CALLA") {
    return false;
  }

  const targetExpr = stripOuterParens(state.stack.pop() ?? "???");
  const resolvedTarget =
    state.pointerTargetsByExpression.get(targetExpr) ??
    state.pointerTargetsBySlot.get(targetExpr) ??
    state.context.callaTargetsByOffset?.get(instruction.offset) ??
    null;

  if (resolvedTarget !== null) {
    const callee =
      state.context.methodLabelsByOffset.get(resolvedTarget) ??
      `sub_0x${hexOffset(resolvedTarget)}`;
    const argCount = state.context.methodArgCountsByOffset.get(resolvedTarget) ?? 0;
    const args = popCallArguments(state, instruction, callee, argCount);
    emitInternalCallResult(state, resolvedTarget, `${callee}(${args.join(", ")})`);
  } else {
    state.stack.push(`calla(${targetExpr})`);
  }
  return true;
}

function emitInternalCallResult(state, target, expression) {
  if (state.context.methodNeverReturnsByOffset?.get(target) === true) {
    // C# cannot infer that an arbitrary helper always aborts/throws. Keep the
    // call visible, then make the proven non-returning edge explicit so a
    // surrounding non-void branch remains compile-valid.
    state.statements.push(`${expression};`);
    state.statements.push("throw();");
    state.stack.length = 0;
    state.terminated = true;
  } else if (state.context.methodReturnsValueByOffset?.get(target) === false) {
    state.statements.push(`${expression};`);
  } else {
    state.stack.push(expression);
  }
}

export function tryTokenCall(state, instruction) {
  if (instruction.opcode.mnemonic !== "CALLT") {
    return false;
  }
  const index = instruction.operand?.kind === "U16" ? instruction.operand.value : null;
  if (index === null) {
    return false;
  }
  const resolved = state.context.calltLabels[index];
  if (resolved === undefined) {
    // Unresolved/out-of-range token: mirror Rust (jumps.rs emit_indirect_call) —
    // bind the bare `callt(0xHEX)` token call to a temp WITHOUT consuming or
    // appending arguments. The fallback label already reads as a call, so the
    // previous `${label}(${args})` produced an invalid double-call
    // `callt(0xHEX)()`.
    const temp = `t${state.nextTempId}`;
    state.nextTempId += 1;
    state.statements.push(`let ${temp} = callt(0x${hex16(index)});`);
    state.stack.push(temp);
    return true;
  }
  const argCount = state.context.calltParamCounts[index] ?? 0;
  const returnsValue = state.context.calltReturnsValue[index] ?? true;
  const args = popCallArguments(state, instruction, resolved, argCount);
  const expression = `${resolved}(${args.join(", ")})`;
  if (returnsValue) {
    state.stack.push(expression);
  } else {
    state.statements.push(`${expression};`);
  }
  return true;
}

// Pop `argCount` values off the stack to use as call arguments. When the
// stack underflows we substitute `???` (matching the syscall path) and
// emit a structured warning + trace-style note so the user sees the
// hazard in both the rendered output and the `warnings` array. The
// previous fallback string `/* stack_underflow */` rendered as a
// C-style comment in argument position, which was awkward and
// inconsistent with the syscall path.
function popCallArguments(state, instruction, calleeLabel, argCount) {
  const args = [];
  let missingArgument = false;
  const renderedCount = Math.min(argCount, MAX_RENDERED_CALL_ARGUMENTS);
  for (let index = 0; index < renderedCount; index += 1) {
    const value = state.stack.pop();
    if (value === undefined) {
      missingArgument = true;
      args.push("???");
    } else {
      args.push(stripOuterParens(value));
    }
  }
  if (argCount > renderedCount) {
    // A malformed or adversarial token can legally advertise a u16-sized
    // argument list. Keep source output bounded while consuming the values
    // that the VM call would remove from the abstract stack.
    const omitted = argCount - renderedCount;
    state.stack.length = Math.max(0, state.stack.length - omitted);
    args.push("unknown");
    const message = `call argument count ${argCount} exceeds render limit `
      + `${MAX_RENDERED_CALL_ARGUMENTS}; omitted ${omitted} values for ${calleeLabel}`;
    state.statements.push(`// warning: ${message}`);
    state.warnings.push(
      `high-level: 0x${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${message}`,
    );
  }
  if (missingArgument) {
    const message = `missing call argument values for ${calleeLabel} (substituted ???)`;
    state.statements.push(`// warning: ${message}`);
    state.warnings.push(
      `high-level: 0x${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${message}`,
    );
  }
  return args;
}

export function trySyscall(state, instruction) {
  if (instruction.opcode.mnemonic !== "SYSCALL") {
    return false;
  }
  const hash =
    instruction.operand?.kind === "Syscall" ? instruction.operand.value : null;
  if (hash === null) {
    return false;
  }
  const info = SYSCALLS.get(hash) ?? null;
  const argCount = info?.param_count ?? 0;
  const returnsValue = info?.returns_value ?? true;
  // Syscall arguments are pushed right-to-left (Cdecl) by the devpack,
  // so parameters[0] sits on top of the stack at SYSCALL and
  // `ApplicationEngine.OnSysCall` pops it first: pop order already
  // equals declaration order — do NOT reverse (matches the
  // internal-call path in popCallArguments).
  const args = [];
  let missingArgument = false;
  for (let index = 0; index < argCount; index += 1) {
    const value = state.stack.pop();
    if (value === undefined) {
      missingArgument = true;
      args.push("???");
    } else {
      args.push(stripOuterParens(value));
    }
  }

  const call = info
    ? args.length > 0
      ? `syscall("${info.name}", ${args.join(", ")})`
      : `syscall("${info.name}")`
    : `syscall(0x${hex32(hash)})`;

  if (missingArgument && info) {
    let message = `missing syscall argument values for ${info.name} (substituted ???)`;
    const context = missingSyscallArgumentContext(state, info.name);
    if (context) {
      message += `; ${context}`;
    }
    state.statements.push(`// warning: ${message}`);
    state.warnings.push(
      `high-level: 0x${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${message}`,
    );
  }
  if (!info) {
    // Surface the fact that the syscall hash isn't in our generated
    // table — without this annotation the user just sees a bare hex
    // call and has to guess. Mirrors the Rust port's
    // `// unknown syscall` trailing comment.
    const message = `unknown syscall 0x${hex32(hash)}`;
    state.statements.push(`// warning: ${message}`);
    state.warnings.push(
      `high-level: 0x${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${message}`,
    );
  }

  if (returnsValue) {
    state.stack.push(call);
  } else {
    state.statements.push(`${call};`);
  }
  return true;
}

function missingSyscallArgumentContext(state, syscallName) {
  const previousInstruction = state.previousInstruction;
  const previousStoreInfo = state.previousStoreInfo;
  if (
    !previousInstruction ||
    !previousStoreInfo ||
    previousInstruction.offset !== previousStoreInfo.offset
  ) {
    return null;
  }

  const storedValue = previousStoreInfo.storedPacked
    ? "a packed value"
    : "the last produced value";
  return `preceding ${previousStoreInfo.opcode} stored ${storedValue} into ${previousStoreInfo.slotLabel}; no value remains on the evaluation stack before ${syscallName}`;
}
