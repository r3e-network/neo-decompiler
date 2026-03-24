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
  return {
    stack:
      manifestMethod?.parameters?.length || inferredArgCount === 0
        ? []
        : [...parameterNames],
    statements: [],
    warnings: [],
    initializedLocals: new Set(),
    initializedStatics: new Set(),
    parameterNames,
    returnsVoid:
      manifestMethod?.returnType === "Void" || manifestMethod?.returnType === "void",
    context,
    nextTempId: 0,
    pointerTargetsByExpression: new Map(),
    pointerTargetsBySlot: new Map(),
    packedValuesByExpression: new Map(),
    packedValuesBySlot: new Map(),
    previousInstruction: null,
    previousStoreInfo: null,
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
    previousInstruction: state.previousInstruction,
    previousStoreInfo: state.previousStoreInfo,
    labelTargets: new Set(state.labelTargets),
    emittedLabels: new Set(state.emittedLabels),
    program: state.program,
  };
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
