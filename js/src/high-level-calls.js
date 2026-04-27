import { SYSCALLS } from "./generated/syscalls.js";
import { jumpTarget, stripOuterParens } from "./high-level-utils.js";
import { hex16, hex32 } from "./util.js";

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
    state.context.methodLabelsByOffset.get(target) ?? `call_0x${hex16(target)}`;
  const argCount = state.context.methodArgCountsByOffset.get(target) ?? 0;
  const args = popCallArguments(state, instruction, callee, argCount);
  state.stack.push(`${callee}(${args.join(", ")})`);
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
    null;

  if (resolvedTarget !== null) {
    const callee =
      state.context.methodLabelsByOffset.get(resolvedTarget) ??
      `sub_0x${hex16(resolvedTarget)}`;
    const argCount = state.context.methodArgCountsByOffset.get(resolvedTarget) ?? 0;
    const args = popCallArguments(state, instruction, callee, argCount);
    state.stack.push(`${callee}(${args.join(", ")})`);
  } else {
    state.stack.push(`calla(${targetExpr})`);
  }
  return true;
}

export function tryTokenCall(state, instruction) {
  if (instruction.opcode.mnemonic !== "CALLT") {
    return false;
  }
  const index = instruction.operand?.kind === "U16" ? instruction.operand.value : null;
  if (index === null) {
    return false;
  }
  const label =
    state.context.calltLabels[index] ?? `callt(0x${hex16(index)})`;
  const argCount = state.context.calltParamCounts[index] ?? 0;
  const returnsValue = state.context.calltReturnsValue[index] ?? true;
  const args = popCallArguments(state, instruction, label, argCount);
  const expression = `${label}(${args.join(", ")})`;
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
  for (let index = 0; index < argCount; index += 1) {
    const value = state.stack.pop();
    if (value === undefined) {
      missingArgument = true;
      args.push("???");
    } else {
      args.push(stripOuterParens(value));
    }
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
  args.reverse();

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
