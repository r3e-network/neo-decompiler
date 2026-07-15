import { sanitizeParameterNames } from "./manifest.js";
import { collectLabelTargets } from "./high-level-labels.js";
import { slotIndexFromMnemonic } from "./high-level-slots.js";

export function createState(
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = 0,
  instructions = [],
) {
  const inferredArgCount = context?.methodArgCountsByOffset?.get(methodOffset) ?? 0;
  const parameterNames = manifestMethod?.parameters?.length
    ? sanitizeParameterNames(manifestMethod.parameters)
    : Array.from({ length: inferredArgCount }, (_, index) => `arg${index}`);
  // Pre-populate the operand stack with the parameter names ONLY when
  // the method does not start with `INITSLOT`. INITSLOT pops args off
  // the stack into the arg slots, so pre-populating in that case
  // creates phantom `argN` values that linger past LDARG/RET and
  // surface as spurious bare-expression statements (e.g. `return 1;
  // arg0;`). Mirrors the Rust `set_argument_labels` guard.
  const startsWithInitslot =
    instructions[0]?.opcode?.mnemonic === "INITSLOT";
  const inferredReturnBehavior =
    context?.methodContractsByOffset?.get(methodOffset)?.returnBehavior;
  return {
    stack:
      manifestMethod?.parameters?.length ||
      inferredArgCount === 0 ||
      startsWithInitslot
        ? []
        : [...parameterNames],
    statements: [],
    warnings: [],
    initializedLocals: new Set(),
    initializedStatics: new Set(),
    parameterNames,
    returnsVoid:
      manifestMethod?.returnType?.toLowerCase() === "void" ||
      inferredReturnBehavior === "void",
    context,
    nextTempId: 0,
    pointerTargetsByExpression: new Map(),
    pointerTargetsBySlot: new Map(),
    packedValuesByExpression: new Map(),
    packedValuesBySlot: new Map(),
    // Forward jumps are emitted by the linear fallback while the statement
    // walk continues through the skipped instructions. Keep the stack that
    // reaches each target so the label can restore it when encountered.
    stackSnapshotsByLabel: new Map(),
    previousInstruction: null,
    previousStoreInfo: null,
    lastDroppedValue: undefined,
    labelTargets: collectLabelTargets(instructions),
    emittedLabels: new Set(),
    program: instructions,
  };
}

export function cloneState(state) {
  return {
    stack: [...state.stack],
    statements: [...state.statements],
    warnings: [...state.warnings],
    initializedLocals: new Set(state.initializedLocals),
    initializedStatics: new Set(state.initializedStatics),
    parameterNames: [...state.parameterNames],
    returnsVoid: state.returnsVoid,
    context: state.context,
    nextTempId: state.nextTempId,
    pointerTargetsByExpression: new Map(state.pointerTargetsByExpression),
    pointerTargetsBySlot: new Map(state.pointerTargetsBySlot),
    packedValuesByExpression: new Map(state.packedValuesByExpression),
    packedValuesBySlot: new Map(state.packedValuesBySlot),
    stackSnapshotsByLabel: new Map(
      [...state.stackSnapshotsByLabel].map(([offset, snapshot]) => [
        offset,
        snapshot === null ? null : [...snapshot],
      ]),
    ),
    previousInstruction: state.previousInstruction,
    previousStoreInfo: state.previousStoreInfo,
    lastDroppedValue: state.lastDroppedValue,
    labelTargets: new Set(state.labelTargets),
    emittedLabels: new Set(state.emittedLabels),
    program: state.program,
  };
}

/**
 * Fork a state for a nested structured slice. The parent stack, slot
 * provenance, and temp numbering remain available to the nested body, while
 * emitted statements, labels, and forward-jump snapshots belong only to the
 * new slice.
 */
export function forkStateForSlice(state, instructions) {
  const fork = cloneState(state);
  fork.statements = [];
  fork.warnings = [];
  fork.previousInstruction = null;
  fork.previousStoreInfo = null;
  fork.labelTargets = collectLabelTargets(instructions);
  fork.emittedLabels = new Set();
  fork.stackSnapshotsByLabel = new Map();
  fork.program = instructions;
  return fork;
}

export function advanceNextTempIdFromStatements(state, statements) {
  for (const statement of statements) {
    for (const match of statement.matchAll(/\bt(\d+)\b/gu)) {
      state.nextTempId = Math.max(state.nextTempId, Number(match[1]) + 1);
    }
  }
}

export function finishInstruction(state, instruction) {
  state.previousInstruction = instruction;
}

export function captureStoreInfo(instruction, state) {
  const local = slotIndexForInstruction(instruction, "STLOC");
  if (local !== null) {
    return {
      offset: instruction.offset,
      opcode: storeOpcodeName(instruction, "STLOC", local),
      slotLabel: `loc${local}`,
      storedPacked: state.packedValuesBySlot.has(`loc${local}`),
    };
  }

  const argument = slotIndexForInstruction(instruction, "STARG");
  if (argument !== null) {
    return {
      offset: instruction.offset,
      opcode: storeOpcodeName(instruction, "STARG", argument),
      slotLabel: `arg${argument}`,
      storedPacked: false,
    };
  }

  const stat = slotIndexForInstruction(instruction, "STSFLD");
  if (stat !== null) {
    return {
      offset: instruction.offset,
      opcode: storeOpcodeName(instruction, "STSFLD", stat),
      slotLabel: `static${stat}`,
      storedPacked: state.packedValuesBySlot.has(`static${stat}`),
    };
  }

  return null;
}

export function emptyContext() {
  return {
    methodLabelsByOffset: new Map(),
    methodArgCountsByOffset: new Map(),
    methodReturnsValueByOffset: new Map(),
    methodContractsByOffset: new Map(),
    callaTargetsByOffset: new Map(),
    calltLabels: [],
    calltParamCounts: [],
    calltReturnsValue: [],
  };
}

function slotIndexForInstruction(instruction, prefix) {
  const exact = slotIndexFromMnemonic(instruction.opcode.mnemonic, prefix);
  if (exact !== null) {
    return exact;
  }
  if (instruction.opcode.mnemonic === prefix && typeof instruction.operand?.value === "number") {
    return instruction.operand.value;
  }
  return null;
}

function storeOpcodeName(instruction, prefix, index) {
  if (instruction.opcode.mnemonic === prefix) {
    return `${prefix}${index}`;
  }
  return instruction.opcode.mnemonic;
}
