import { buildCallGraph } from "./call-graph.js";
import { parseNef } from "./nef.js";
import { disassembleScript } from "./disassembler.js";
import { SYSCALLS } from "./generated/syscalls.js";
import { renderGroupedPseudocode } from "./grouped-pseudocode.js";
import { renderHighLevelMethodGroups } from "./high-level.js";
import { parseManifest } from "./manifest.js";
import { buildMethodGroups } from "./methods.js";
import { renderPseudocode } from "./pseudocode.js";
import { inferTypes } from "./types.js";
import { buildXrefs } from "./xrefs.js";

export {
  buildCallGraph,
  buildMethodGroups,
  buildXrefs,
  inferTypes,
  parseManifest,
  parseNef,
  disassembleScript,
  renderGroupedPseudocode,
  renderPseudocode,
  renderHighLevelMethodGroups,
};

export function decompileBytes(bytes, options = {}) {
  const nef = parseNef(bytes);
  const disassembly = disassembleScript(nef.script, options);
  return {
    nef,
    instructions: disassembly.instructions,
    warnings: disassembly.warnings,
    pseudocode: renderPseudocode(disassembly.instructions),
  };
}

export function analyzeBytes(bytes, manifestInput = null, options = {}) {
  const manifest = manifestInput ? parseManifest(manifestInput) : null;
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, manifest);
  return {
    ...result,
    manifest,
    methodGroups,
    callGraph: buildCallGraph(result.nef, result.instructions, methodGroups),
    xrefs: buildXrefs(result.instructions, methodGroups),
    types: inferTypes(result.instructions, methodGroups, manifest),
  };
}

export function decompileBytesWithManifest(bytes, manifestInput, options = {}) {
  const manifest = parseManifest(manifestInput);
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, manifest);
  return {
    ...result,
    manifest,
    methodGroups,
    groupedPseudocode: renderGroupedPseudocode(methodGroups, manifest),
  };
}

export function decompileHighLevelBytes(bytes, options = {}) {
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, null);
  const context = buildHighLevelContext(methodGroups, result.nef);
  const highLevel = renderHighLevelMethodGroups(methodGroups, null, context);
  return {
    ...result,
    warnings: [...result.warnings, ...context.highLevelWarnings],
    methodGroups,
    highLevel,
  };
}

export function decompileHighLevelBytesWithManifest(bytes, manifestInput, options = {}) {
  const manifest = parseManifest(manifestInput);
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, manifest);
  const context = buildHighLevelContext(methodGroups, result.nef);
  const highLevel = renderHighLevelMethodGroups(methodGroups, manifest, context);
  return {
    ...result,
    warnings: [...result.warnings, ...context.highLevelWarnings],
    manifest,
    methodGroups,
    highLevel,
    groupedPseudocode: renderGroupedPseudocode(methodGroups, manifest),
  };
}

function buildHighLevelContext(methodGroups, nef) {
  const entryOffset = methodGroups[0]?.start ?? 0;
  return {
    methodLabelsByOffset: new Map(methodGroups.map((group) => [group.start, group.name])),
    methodArgCountsByOffset: new Map(
      methodGroups.map((group) => [group.start, inferMethodArgCount(group, entryOffset)]),
    ),
    calltLabels: nef.methodTokens.map((token) => token.method),
    calltParamCounts: nef.methodTokens.map((token) => token.parametersCount),
    calltReturnsValue: nef.methodTokens.map((token) => token.hasReturnValue),
    highLevelWarnings: [],
  };
}

function inferMethodArgCount(group, entryOffset) {
  if (group.source?.parameters) {
    return group.source.parameters.length;
  }
  const first = group.instructions[0];
  if (first?.opcode?.mnemonic === "INITSLOT" && first.operand?.kind === "Bytes" && first.operand.value.length >= 2) {
    return first.operand.value[1];
  }
  if (group.start === entryOffset) {
    return 0;
  }
  return inferRequiredEntryStackDepth(group.instructions);
}

function inferRequiredEntryStackDepth(instructions) {
  let required = 0;
  let depth = 0;

  for (const instruction of instructions) {
    if (instruction.opcode.mnemonic === "RET") {
      break;
    }
    const effect = stackEffectForArgInference(instruction);
    if (!effect) {
      break;
    }
    while (depth < effect.pops) {
      depth += 1;
      required += 1;
    }
    depth -= effect.pops;
    depth += effect.pushes;
  }

  return required;
}

function stackEffectForArgInference(instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  if (["NOP", "INITSSLOT", "INITSLOT"].includes(mnemonic)) {
    return { pops: 0, pushes: 0 };
  }
  if (
    mnemonic.startsWith("PUSH") ||
    mnemonic === "NEWARRAY0" ||
    mnemonic === "NEWMAP" ||
    mnemonic === "NEWSTRUCT0" ||
    mnemonic.startsWith("LDLOC") ||
    mnemonic.startsWith("LDARG") ||
    mnemonic.startsWith("LDSFLD") ||
    mnemonic === "DEPTH"
  ) {
    return { pops: 0, pushes: 1 };
  }
  if (
    mnemonic.startsWith("STLOC") ||
    mnemonic.startsWith("STARG") ||
    mnemonic.startsWith("STSFLD") ||
    mnemonic === "DROP"
  ) {
    return { pops: 1, pushes: 0 };
  }
  if (mnemonic === "SYSCALL" && instruction.operand?.kind === "Syscall") {
    const info = SYSCALLS.get(instruction.operand.value) ?? null;
    if (!info) return null;
    return {
      pops: info.param_count ?? 0,
      pushes: (info.returns_value ?? true) ? 1 : 0,
    };
  }
  if (
    [
      "ADD",
      "SUB",
      "MUL",
      "DIV",
      "MOD",
      "EQUAL",
      "NOTEQUAL",
      "LT",
      "LE",
      "GT",
      "GE",
      "BOOLAND",
      "BOOLOR",
      "NUMEQUAL",
      "NUMNOTEQUAL",
      "CAT",
      "HASKEY",
      "PICKITEM",
    ].includes(mnemonic)
  ) {
    return { pops: 2, pushes: 1 };
  }
  if (mnemonic === "POPITEM") {
    return { pops: 1, pushes: 1 };
  }
  return null;
}
