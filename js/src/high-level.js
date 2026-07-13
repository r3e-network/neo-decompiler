import { tryCollectionExpression, tryCollectionStatement } from "./high-level-collections.js";
import { createControlFlowHelpers } from "./high-level-control-flow.js";
import { emitLabelIfNeeded, tryControlTransferFallback } from "./high-level-labels.js";
import { tryInternalCall, tryIndirectCall, trySyscall, tryTokenCall } from "./high-level-calls.js";
import { captureStoreInfo, cloneState, createState, emptyContext, finishInstruction } from "./high-level-state.js";
import {
  tryBinaryExpression,
  tryControlStatement,
  tryStackShapeOperation,
  tryUnaryExpression,
} from "./high-level-stack.js";
import {
  pushImmediate,
  tryLoadLocalOrArg,
  tryLoadStatic,
  trySlotDeclarations,
  tryStoreArgument,
  tryStoreLocal,
  tryStoreStatic,
} from "./high-level-slots.js";
import { renderUntranslatedInstruction, stripOuterParens } from "./high-level-utils.js";
import { renderContractHeader } from "./high-level/header.js";
import {
  formatManifestParameters,
  formatManifestType,
  makeUniqueIdentifier,
  sanitizeIdentifier,
} from "./manifest.js";
import { hexOffset } from "./util.js";
import { postprocess } from "./postprocess.js";

let CONTROL_FLOW;

function liftStructuredSlice(
  instructions,
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = instructions[0]?.offset ?? 0,
) {
  if (instructions.length === 0) {
    return { statements: [], warnings: [] };
  }

  const result =
    CONTROL_FLOW.tryLiftSimpleSwitch(instructions, manifestMethod, context, methodOffset) ??
    CONTROL_FLOW.tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset) ??
    CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset) ??
    CONTROL_FLOW.tryLiftSimpleBranch(instructions, manifestMethod, context, methodOffset) ??
    liftStraightLineMethodBody(instructions, manifestMethod, context, undefined, methodOffset);
  return result;
}

CONTROL_FLOW = createControlFlowHelpers({
  createState,
  cloneState,
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
    const loopLift = CONTROL_FLOW.tryLiftSimpleLoop(instructions, manifestMethod, context, methodOffset);
    if (loopLift !== null) {
      result = loopLift;
    } else {
      const tryLift = CONTROL_FLOW.tryLiftSimpleTryBlock(instructions, manifestMethod, context, methodOffset);
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
  return { statements: state.statements, warnings: state.warnings };
}

function executeStraightLine(state, instructions) {
  // Destructure stable references once. state.stack / state.statements / the
  // four pointer-and-packed Maps are mutated in place but never reassigned,
  // so caching the references avoids per-instruction property loads. The
  // assigned-to state.previousStoreInfo / state.previousInstruction stay on
  // `state` since those are slot writes, not mutations.
  const {
    statements,
    initializedLocals,
    initializedStatics,
    parameterNames,
    returnsVoid,
    stack,
    pointerTargetsByExpression,
    pointerTargetsBySlot,
    packedValuesByExpression,
    packedValuesBySlot,
  } = state;

  for (const instruction of instructions) {
    const mnemonic = instruction.opcode.mnemonic;
    emitLabelIfNeeded(state, instruction.offset);

    if (trySlotDeclarations(statements, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (pushImmediate(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (mnemonic === "NOP") {
      finishInstruction(state, instruction);
      continue;
    }

    // ENDFINALLY without a structured try-block lift wrapping it
    // (i.e. one that already absorbed the matching ENDTRY) is
    // best treated as a pass-through: the JS port has no
    // verbose-mode trace comment to emit and the bytecode-level
    // `endfinally` semantics are subtle enough that flagging it
    // as "not yet translated" misleads. Match the Rust port's
    // clean-mode behaviour (silently consume).
    if (mnemonic === "ENDFINALLY") {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryLoadLocalOrArg(stack, mnemonic, parameterNames, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryLoadStatic(stack, mnemonic, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (
      tryStoreLocal(
        statements,
        stack,
        initializedLocals,
        pointerTargetsByExpression,
        pointerTargetsBySlot,
        packedValuesByExpression,
        packedValuesBySlot,
        mnemonic,
        instruction,
      )
    ) {
      state.previousStoreInfo = captureStoreInfo(instruction, state);
      finishInstruction(state, instruction);
      continue;
    }

    if (tryStoreArgument(statements, stack, parameterNames, mnemonic, instruction)) {
      state.previousStoreInfo = captureStoreInfo(instruction, state);
      finishInstruction(state, instruction);
      continue;
    }

    if (
      tryStoreStatic(
        statements,
        stack,
        initializedStatics,
        pointerTargetsByExpression,
        pointerTargetsBySlot,
        packedValuesByExpression,
        packedValuesBySlot,
        mnemonic,
        instruction,
      )
    ) {
      state.previousStoreInfo = captureStoreInfo(instruction, state);
      finishInstruction(state, instruction);
      continue;
    }

    if (tryBinaryExpression(stack, mnemonic)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryInternalCall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryIndirectCall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryTokenCall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (trySyscall(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryCollectionExpression(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryCollectionStatement(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryStackShapeOperation(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryUnaryExpression(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryControlStatement(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (tryControlTransferFallback(state, instruction)) {
      finishInstruction(state, instruction);
      continue;
    }

    if (mnemonic === "RET") {
      if (returnsVoid || stack.length === 0) {
        statements.push("return;");
      } else {
        statements.push(`return ${stripOuterParens(stack.pop())};`);
      }
      finishInstruction(state, instruction);
      continue;
    }

    statements.push(renderUntranslatedInstruction(instruction));
    // Surface the not-yet-translated opcode through the structured
    // `warnings` array too, so a programmatic caller (CI, IDE
    // integration) sees the hazard without having to grep the
    // rendered source. Mirrors the Rust port's `self.warn(...)` for
    // unhandled opcodes.
    const opcodeName =
      instruction.opcode.mnemonic === "UNKNOWN"
        ? `UNKNOWN_0x${instruction.opcode.byte.toString(16).padStart(2, "0").toUpperCase()}`
        : instruction.opcode.mnemonic;
    state.warnings.push(
      `high-level: 0x${instruction.offset.toString(16).padStart(4, "0").toUpperCase()}: ${opcodeName} (not yet translated)`,
    );
    finishInstruction(state, instruction);
  }
}
