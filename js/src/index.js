import { buildCallGraph } from "./call-graph.js";
import { renderCSharpContract } from "./csharp.js";
import { NeoDecompilerError, NefParseError, DisassemblyError, ManifestParseError } from "./errors.js";
import { parseNef } from "./nef.js";
import { disassembleScript } from "./disassembler.js";
import { renderGroupedPseudocode } from "./grouped-pseudocode.js";
import { renderHighLevelMethodGroups } from "./high-level.js";
import { buildHighLevelContext } from "./high-level-context.js";
import { classifyPermissionContract, parseManifest } from "./manifest.js";
import { identifyPatterns } from "./patterns.js";
import { buildMethodGroups } from "./methods.js";
import { renderPseudocode } from "./pseudocode.js";
import { inferTypes } from "./types.js";
import { buildXrefs } from "./xrefs.js";

export {
  buildCallGraph,
  buildMethodGroups,
  buildXrefs,
  classifyPermissionContract,
  inferTypes,
  identifyPatterns,
  parseManifest,
  parseNef,
  disassembleScript,
  renderGroupedPseudocode,
  renderCSharpContract,
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
    patterns: identifyPatterns(nef, disassembly.instructions, null),
  };
}

/**
 * Run the full analysis pipeline (CFG, xrefs, type inference) against
 * a NEF and an optional manifest. Returns the same fields as
 * `decompileBytes` plus method groups, call graph, method contracts,
 * xrefs, and inferred types.
 */
export function analyzeBytes(bytes, manifestInput = null, options = {}) {
  const manifest = manifestInput ? parseManifest(manifestInput) : null;
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, manifest);
  // Analysis (call graph / xrefs / types) groups on the same baseline as the
  // Rust port's `analysis::MethodTable`, which excludes post-terminator
  // detached tails (a presentation-only heuristic). Using the tail-included
  // `methodGroups` here would attribute padding/tail chunks to spurious method
  // entries that the Rust analysis never reports, diverging the two ports.
  const analysisGroups = buildMethodGroups(result.instructions, manifest, {
    includePostTerminatorTails: false,
  });
  const callGraph = buildCallGraph(result.nef, result.instructions, analysisGroups);
  const context = buildHighLevelContext(
    analysisGroups,
    analysisGroups,
    result.nef,
    options,
    callGraph,
  );
  return {
    ...result,
    manifest,
    methodGroups,
    callGraph,
    methodContracts: context.methodContracts,
    patterns: identifyPatterns(result.nef, result.instructions, manifest),
    xrefs: buildXrefs(result.instructions, analysisGroups),
    types: inferTypes(result.instructions, analysisGroups, manifest),
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
    patterns: identifyPatterns(result.nef, result.instructions, manifest),
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
 * @param {Object} [options] - See `DecompileOptions` in index.d.ts.
 * @param {boolean} [options.clean] - Opt-in maximum-readability
 *   shorthand: enables `inlineSingleUseTemps` and strips informational
 *   slot-declaration comments. Off by default.
 * @param {boolean} [options.inlineSingleUseTemps] - Inline single-use
 *   `tN` temporaries into their use site. Off by default; implied by
 *   `clean: true`.
 * @param {boolean} [options.failOnUnknownOpcodes] - Throw instead of
 *   emitting `UNKNOWN_0xNN` when an unknown opcode is encountered.
 */
export function decompileHighLevelBytes(bytes, options = {}) {
  const result = decompileBytes(bytes, options);
  const methodGroups = buildMethodGroups(result.instructions, null);
  const analysisGroups = buildMethodGroups(result.instructions, null, {
    includePostTerminatorTails: false,
  });
  const callGraph = buildCallGraph(result.nef, result.instructions, analysisGroups);
  const context = buildHighLevelContext(
    methodGroups,
    analysisGroups,
    result.nef,
    options,
    callGraph,
  );
  const highLevel = renderHighLevelMethodGroups(methodGroups, null, context);
  const patterns = result.patterns;
  return {
    ...result,
    warnings: [...result.warnings, ...context.highLevelWarnings],
    methodGroups,
    methodContracts: context.methodContracts,
    patterns,
    highLevel,
    csharp: renderCSharpContract(
      highLevel,
      null,
      { ...options, methodTokens: result.nef.methodTokens },
      patterns,
    ),
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
  const analysisGroups = buildMethodGroups(result.instructions, manifest, {
    includePostTerminatorTails: false,
  });
  const callGraph = buildCallGraph(result.nef, result.instructions, analysisGroups);
  const context = buildHighLevelContext(
    methodGroups,
    analysisGroups,
    result.nef,
    options,
    callGraph,
  );
  const highLevel = renderHighLevelMethodGroups(methodGroups, manifest, context);
  const patterns = identifyPatterns(result.nef, result.instructions, manifest);
  return {
    ...result,
    warnings: [...result.warnings, ...context.highLevelWarnings],
    manifest,
    methodGroups,
    methodContracts: context.methodContracts,
    patterns,
    highLevel,
    csharp: renderCSharpContract(
      highLevel,
      manifest,
      { ...options, methodTokens: result.nef.methodTokens },
      patterns,
    ),
    groupedPseudocode: renderGroupedPseudocode(methodGroups, manifest),
  };
}
