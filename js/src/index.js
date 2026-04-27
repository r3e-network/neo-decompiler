import { buildCallGraph } from "./call-graph.js";
import { NeoDecompilerError, NefParseError, DisassemblyError, ManifestParseError } from "./errors.js";
import { parseNef } from "./nef.js";
import { disassembleScript } from "./disassembler.js";
import { SYSCALLS } from "./generated/syscalls.js";
import { renderGroupedPseudocode } from "./grouped-pseudocode.js";
import { renderHighLevelMethodGroups } from "./high-level.js";
import { parseManifest } from "./manifest.js";
import { buildMethodGroups } from "./methods.js";
import { describeMethodToken } from "./native-contracts.js";
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
  NeoDecompilerError,
  NefParseError,
  DisassemblyError,
  ManifestParseError,
};

/**
 * Parse a NEF blob and return its instruction stream plus rendered
 * pseudocode (no manifest correlation, no high-level lifting).
 *
 * @param {Uint8Array | ArrayBuffer | number[]} bytes - Raw NEF bytes.
 * @param {Object} [options] - Disassembly options.
 * @param {boolean} [options.failOnUnknownOpcodes] - Throw instead of
 *   emitting `UNKNOWN_0xNN` when an unknown opcode is encountered.
 * @returns {{
 *   nef: import('./index').NefFile,
 *   instructions: import('./index').Instruction[],
 *   warnings: string[],
 *   pseudocode: string,
 * }}
 */
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

/**
 * Run the full analysis pipeline (CFG, xrefs, type inference) against
 * a NEF and an optional manifest. Returns the same fields as
 * `decompileBytes` plus method groups, call graph, xrefs, and
 * inferred types.
 */
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

/**
 * Like `decompileBytes` but with a manifest, so methods can be
 * grouped by their declared offsets and the grouped pseudocode
 * surface (`groupedPseudocode`) becomes available.
 *
 * @param {Uint8Array | ArrayBuffer | number[]} bytes
 * @param {string | object} manifestInput - Manifest JSON string or
 *   parsed object.
 * @param {Object} [options]
 */
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

/**
 * Decompile a NEF directly to high-level pseudocode (no manifest).
 * The resulting `highLevel` string is the human-readable surface
 * that mirrors the Rust CLI's `--format high-level` default.
 *
 * @param {Uint8Array | ArrayBuffer | number[]} bytes
 * @param {Object} [options] - See `DecompileOptions` in index.d.ts;
 *   the most useful are `clean: true` (the default-equivalent
 *   shorthand for inlined temps and no trace comments) and
 *   `emitTraceComments: true` (re-enable per-instruction trace).
 */
export function decompileHighLevelBytes(bytes, options = {}) {
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, null);
  const context = buildHighLevelContext(methodGroups, result.nef, options);
  const highLevel = renderHighLevelMethodGroups(methodGroups, null, context);
  return {
    ...result,
    warnings: [...result.warnings, ...context.highLevelWarnings],
    methodGroups,
    highLevel,
  };
}

/**
 * High-level decompile + manifest correlation in one call. Same
 * `highLevel` surface as `decompileHighLevelBytes`, plus method
 * groups and grouped pseudocode for callers that want both views.
 *
 * @param {Uint8Array | ArrayBuffer | number[]} bytes
 * @param {string | object} manifestInput
 * @param {Object} [options]
 */
export function decompileHighLevelBytesWithManifest(bytes, manifestInput, options = {}) {
  const manifest = parseManifest(manifestInput);
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, manifest);
  const context = buildHighLevelContext(methodGroups, result.nef, options);
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

function buildHighLevelContext(methodGroups, nef, options = {}) {
  const entryOffset = methodGroups[0]?.start ?? 0;
  return {
    methodLabelsByOffset: new Map(methodGroups.map((group) => [group.start, group.name])),
    methodArgCountsByOffset: new Map(
      methodGroups.map((group) => [group.start, inferMethodArgCount(group, entryOffset)]),
    ),
    // Resolve token-call labels through the native-contract describe
    // table so calls into known contracts render as
    // `GasToken::Transfer(...)` rather than just `Transfer(...)`. The
    // qualified form mirrors Rust's `callt_labels` (which already runs
    // through `native_contracts::describe_method_token` →
    // `formatted_label`). Falls back to the raw method name when the
    // hash isn't in the native-contract table.
    calltLabels: nef.methodTokens.map((token) => {
      const hint = describeMethodToken(token.hash, token.method);
      return hint ? hint.formattedLabel(token.method) : token.method;
    }),
    calltParamCounts: nef.methodTokens.map((token) => token.parametersCount),
    calltReturnsValue: nef.methodTokens.map((token) => token.hasReturnValue),
    methodTokens: nef.methodTokens,
    scriptHash: nef.scriptHash,
    scriptHashLE: nef.scriptHashLE,
    compiler: nef.header?.compiler,
    source: nef.header?.source,
    highLevelWarnings: [],
    postprocessOptions: {
      // `clean: true` is a convenience shorthand that enables every
      // readability-focused postprocess option. Today that's
      // `inlineSingleUseTemps` plus stripping informational slot-declaration
      // comments, but new options will compose under the same shorthand
      // without callers needing to update.
      inlineSingleUseTemps:
        !!options.inlineSingleUseTemps || !!options.clean,
      clean: !!options.clean,
    },
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
