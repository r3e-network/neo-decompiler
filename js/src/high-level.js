import { executeStraightLine } from "./high-level/straight-line.js";
import { createControlFlowHelpers } from "./high-level-control-flow.js";
import {
  cloneState,
  createState,
  emptyContext,
  forkStateForSlice,
  advanceNextTempIdFromStatements,
} from "./high-level-state.js";
import { stripOuterParens } from "./high-level-utils.js";
import { renderContractHeader } from "./high-level/header.js";
import {
  formatManifestParameters,
  formatManifestType,
  makeUniqueIdentifier,
  sanitizeIdentifier,
} from "./manifest.js";
import { hexOffset } from "./util.js";
import { postprocess } from "./postprocess.js";
import { hasLeadingTry } from "./high-level-control-flow-shared.js";

let CONTROL_FLOW;
const ACTIVE_STRUCTURED_SLICES = new Set();

function liftStructuredSlice(
  instructions,
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = instructions[0]?.offset ?? 0,
  initialState = null,
) {
  if (instructions.length === 0) {
    return { statements: [], warnings: [] };
  }

  const sliceKey = [
    instructions[0]?.offset ?? 0,
    instructions.at(-1)?.offset ?? 0,
    instructions.length,
  ].join(":");
  if (ACTIVE_STRUCTURED_SLICES.has(sliceKey)) {
    return liftStraightLineMethodBody(
      instructions,
      manifestMethod,
      context,
      initialState ? forkStateForSlice(initialState, instructions) : undefined,
      methodOffset,
    );
  }
  ACTIVE_STRUCTURED_SLICES.add(sliceKey);
  try {
    const leadingTry = hasLeadingTry(instructions);
    let result =
      CONTROL_FLOW.tryLiftSimpleSwitch(instructions, manifestMethod, context, methodOffset, initialState) ??
      (leadingTry
        ? CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset, initialState)
        : null) ??
      CONTROL_FLOW.tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset, initialState) ??
      (!leadingTry
        ? CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset, initialState)
        : null) ??
      CONTROL_FLOW.tryLiftSimpleBranch(instructions, manifestMethod, context, methodOffset, initialState) ??
      liftStraightLineMethodBody(
        instructions,
        manifestMethod,
        context,
        initialState ? forkStateForSlice(initialState, instructions) : undefined,
        methodOffset,
      );

  // Loop/branch recognition can claim a broad compiler-generated slice even
  // though a protected region inside it fell through to the linear opcode
  // renderer. Retry the try parser as a narrower alternative and keep it when
  // it removes at least one untranslated TRY warning. This preserves the
  // existing loop preference for clean slices while allowing nested handlers
  // to be structured when the surrounding control-flow shape is imperfect.
  if (
    instructions.length <= 256 &&
    (manifestMethod !== null || (context?.methodTokens?.length ?? 0) > 0) &&
    instructions.some((instruction) =>
      ["TRY", "TRY_L"].includes(instruction.opcode?.mnemonic),
    ) &&
    result.warnings?.some((warning) => /TRY(?:_L)? \(not yet translated\)/u.test(warning))
  ) {
    const tryAlternative = CONTROL_FLOW.tryLiftSimpleTryBlock(
      instructions,
      manifestMethod,
      context,
      methodOffset,
      initialState,
    );
    const primaryTryWarnings = result.warnings.filter((warning) =>
      /TRY(?:_L)? \(not yet translated\)/u.test(warning),
    ).length;
    const alternativeTryWarnings = tryAlternative?.warnings?.filter((warning) =>
      /TRY(?:_L)? \(not yet translated\)/u.test(warning),
    ).length ?? Number.POSITIVE_INFINITY;
    if (tryAlternative && alternativeTryWarnings < primaryTryWarnings) {
      result = tryAlternative;
    }
  }
    if (initialState) {
      advanceNextTempIdFromStatements(initialState, result.statements);
    }
    return result;
  } finally {
    ACTIVE_STRUCTURED_SLICES.delete(sliceKey);
  }
}

CONTROL_FLOW = createControlFlowHelpers({
  createState,
  cloneState,
  forkStateForSlice,
  executeStraightLine,
  liftStructuredSlice,
});

export function renderHighLevelMethodGroups(groups, manifest, context = null) {
  const lines = renderContractHeader(manifest, context);

  const used = new Set();
  let firstEmitted = true;
  for (const group of groups) {
    const body = liftMethodBody(group.instructions, group.source, context, group.start);
    if (context?.highLevelWarnings) {
      context.highLevelWarnings.push(...body.warnings);
    }

    // Inferred helpers (no manifest source) whose body lifts to nothing
    // are usually padding — runs of NOPs the compiler emits between real
    // methods. Skip them entirely; rendering `fn sub_0xNNNN() { // no
    // instructions decoded }` is noise. Manifest-declared methods still
    // get the placeholder so the user sees the ABI is honoured.
    const isInferred = !group.source;
    if (isInferred && body.statements.length === 0) {
      continue;
    }

    // Blank line between method definitions for readability — matches
    // the Rust renderer's `writeln!(output)` before each method body.
    if (!firstEmitted) {
      lines.push("");
    }
    firstEmitted = false;

    const signature = renderMethodSignature(group, used, context);
    lines.push(`    ${signature} {`);

    if (body.statements.length === 0) {
      // A manifest-declared method whose offset resolves but whose body slice
      // is empty (e.g. an offset pointing past the decoded script) gets the
      // offset-bearing placeholder, matching Rust's `write_manifest_methods`.
      // Inferred empty groups were already skipped above, so a source-bearing
      // group here is a manifest method.
      if (group.source && Number.isInteger(group.start)) {
        lines.push(
          `        // no instructions decoded for manifest method at offset 0x${hexOffset(group.start)}`,
        );
      } else {
        lines.push("        // no instructions decoded");
      }
    } else {
      let indentLevel = 0;
      for (const statement of body.statements) {
        const trimmed = statement.trim();
        if (trimmed.startsWith("}")) {
          indentLevel = Math.max(0, indentLevel - 1);
        }
        lines.push(`${" ".repeat(8 + indentLevel * 4)}${trimmed}`);
        if (trimmed.endsWith("{")) {
          indentLevel += 1;
        }
      }
    }

    lines.push("    }");
  }

  lines.push("}");
  return lines.join("\n") + "\n";
}

// Mirrors the Rust `ManifestTrusts::describe()` output:
// - "*" → "*"
// - "" / [] (empty list) → null (caller suppresses the line)
// - ["str", "str"] → `["str", "str"]`
// - {hashes:[...], groups:[...]} → `[hash:0x..., group:02...]`
//   (only when every entry is a string and the object has no
//   unexpected keys; otherwise fall through to JSON.stringify so
//   anomalous shapes are surfaced verbatim instead of silently
//   dropped).
// Stringify a manifest `extra` scalar (string, number, boolean) for
// the high-level `// Key: <value>` comment. Returns `null` for nested
// objects, arrays, or null/undefined: those have no canonical short
// form, so callers drop the entry rather than emit ambiguous output.
// Mirrors Rust's `decompiler::helpers::render_extra_scalar`.

function renderMethodSignature(group, used, context = null) {
  const name = makeUniqueIdentifier(group.name, used);
  const parameters = group.source?.parameters ?? [];
  let args;
  if (parameters.length > 0) {
    args = formatManifestParameters(parameters);
  } else {
    const inferredArgCount = context?.methodArgCountsByOffset?.get(group.start) ?? 0;
    args = Array.from({ length: inferredArgCount }, (_, index) => `arg${index}`).join(", ");
  }
  const returnType = group.source?.returnType;
  const renderedReturnType = returnType ? formatManifestType(returnType) : null;
  if (renderedReturnType && renderedReturnType !== "void") {
    return `fn ${name}(${args}) -> ${renderedReturnType}`;
  }
  if (renderedReturnType === "void") {
    return `fn ${name}(${args})`;
  }
  const inferredReturnBehavior =
    context?.methodContractsByOffset?.get(group.start)?.returnBehavior;
  return inferredReturnBehavior === "void"
    ? `fn ${name}(${args})`
    : `fn ${name}(${args}) -> any`;
}

// Maximum number of instructions a single method body may have before
// high-level lifting is skipped in favour of a fallback note. The stack-lifting
// and postprocess passes are worst-case O(n²) in the per-method statement
// count, so a maliciously crafted in-cap NEF packed with crossing jumps would
// otherwise take a very long time to lift. Real Neo contract methods are
// gas-bounded and far smaller; the raw instruction stream stays available via
// the disassembler. Mirrors the Rust port's MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS.
export const MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS = 16384;

export function liftMethodBody(
  instructions,
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = instructions[0]?.offset ?? 0,
) {
  if (instructions.length > MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS) {
    const offsetHex = (instructions[0]?.offset ?? 0)
      .toString(16)
      .padStart(4, "0")
      .toUpperCase();
    return {
      statements: [
        `// method body too large for high-level lifting: ${instructions.length} instructions ` +
          `exceeds the ${MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS}-instruction limit; use the disassembler for the full listing`,
      ],
      warnings: [
        `high-level: method at 0x${offsetHex} skipped — ${instructions.length} instructions ` +
          `exceeds the high-level lifting limit (${MAX_HIGH_LEVEL_METHOD_INSTRUCTIONS})`,
      ],
    };
  }
  let result;
  const switchLift = CONTROL_FLOW.tryLiftSimpleSwitch(instructions, manifestMethod, context, methodOffset);
  if (switchLift !== null) {
    result = switchLift;
  } else {
    const leadingTry = hasLeadingTry(instructions);
    const leadingTryLift = leadingTry
      ? CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset)
      : null;
    if (leadingTryLift !== null) {
      result = leadingTryLift;
    } else {
      const loopLift = CONTROL_FLOW.tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset);
      if (loopLift !== null) {
        result = loopLift;
      } else {
        const tryLift = leadingTry
          ? null
          : CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset);
        if (tryLift !== null) {
          result = tryLift;
        } else {
          const branchLift = CONTROL_FLOW.tryLiftSimpleBranch(instructions, manifestMethod, context, methodOffset);
          if (branchLift !== null) {
            result = branchLift;
          } else {
            result = liftStraightLineMethodBody(instructions, manifestMethod, context, undefined, methodOffset);
          }
        }
      }
    }
  }

  // The top-level method path has the same ambiguity as nested slices: a
  // broad loop/branch lift can leave a later compiler TRY untranslated. Give
  // the dedicated try parser one opportunity to produce a cleaner result.
  if (
    instructions.length <= 256 &&
    (manifestMethod !== null || (context?.methodTokens?.length ?? 0) > 0) &&
    instructions.some((instruction) =>
      ["TRY", "TRY_L"].includes(instruction.opcode?.mnemonic),
    ) &&
    result.warnings?.some((warning) => /TRY(?:_L)? \(not yet translated\)/u.test(warning))
  ) {
    const tryAlternative = CONTROL_FLOW.tryLiftSimpleTryBlock(
      instructions,
      manifestMethod,
      context,
      methodOffset,
    );
    const primaryTryWarnings = result.warnings.filter((warning) =>
      /TRY(?:_L)? \(not yet translated\)/u.test(warning),
    ).length;
    const alternativeTryWarnings = tryAlternative?.warnings?.filter((warning) =>
      /TRY(?:_L)? \(not yet translated\)/u.test(warning),
    ).length ?? Number.POSITIVE_INFINITY;
    if (tryAlternative && alternativeTryWarnings < primaryTryWarnings) {
      result = tryAlternative;
    }
  }

  postprocess(result.statements, context.postprocessOptions);
  return result;
}

function liftStraightLineMethodBody(
  instructions,
  manifestMethod = null,
  context,
  initialState,
  methodOffset = instructions[0]?.offset ?? 0,
) {
  const state = initialState ?? createState(manifestMethod, context, methodOffset, instructions);
  executeStraightLine(state, instructions);
  if (instructions.at(-1)?.opcode?.mnemonic !== "RET" && state.stack.length > 0) {
    for (const expression of state.stack) {
      state.statements.push(`${stripOuterParens(expression)};`);
    }
    state.stack.length = 0;
  }
  return {
    statements: state.statements,
    warnings: state.warnings,
    stack: [...state.stack],
    nextTempId: state.nextTempId,
  };
}
