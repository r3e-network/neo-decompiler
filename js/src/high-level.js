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
import {
  extractContractName,
  formatManifestParameters,
  formatManifestType,
  makeUniqueIdentifier,
  sanitizeIdentifier,
} from "./manifest.js";
import { describeCallFlags } from "./nef.js";
import { describeMethodToken } from "./native-contracts.js";
import { upperHex } from "./util.js";
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
  const contractName = extractContractName(manifest);
  const lines = [`contract ${contractName} {`];

  // Contract-level metadata block (matches the Rust renderer):
  // - script hash in both byte orders for cross-explorer lookups
  // - supportedstandards / features / permissions / trusts when set
  // - extra fields like Author/Email surfaced as `// Key: Value`
  // - ABI method signatures listed as forward declarations
  // - ABI events listed as `event Name(params);`
  const scriptHash = context?.scriptHash;
  const scriptHashLE = context?.scriptHashLE;
  if (scriptHash) {
    lines.push(`    // script hash (little-endian): ${scriptHashLE}`);
    lines.push(`    // script hash (big-endian): ${scriptHash}`);
  }
  if (context?.compiler) {
    lines.push(`    // compiler: ${context.compiler}`);
  }
  if (context?.source) {
    lines.push(`    // source: ${context.source}`);
  }
  if (!manifest) {
    // Mirror the Rust header writer so the absence of an ABI surface
    // is explicit rather than silently elided. The trailing blank
    // line (separating header from body) is unconditionally added
    // below so we don't push it here — earlier this branch eagerly
    // pushed a blank line, which compounded with the method-tokens
    // header for `// manifest not provided` + blank + `// method
    // tokens declared in NEF`, while Rust runs them flush.
    lines.push(`    // manifest not provided`);
  }
  if (manifest) {
    if (manifest.supportedStandards?.length) {
      const formatted = manifest.supportedStandards.map((s) => `"${s}"`).join(", ");
      lines.push(`    supported_standards = [${formatted}];`);
    }
    if (manifest.features?.storage || manifest.features?.payable) {
      lines.push(`    features {`);
      if (manifest.features.storage) lines.push(`        storage = true;`);
      if (manifest.features.payable) lines.push(`        payable = true;`);
      lines.push(`    }`);
    }
    if (manifest.groups?.length) {
      // `groups` is the list of pubkey/signature pairs that authorise
      // signed updates of the contract. Show only the pubkey
      // (canonical short form) for a scannable summary; the signature
      // is opaque base64 and adds no human-readable value.
      lines.push(`    groups {`);
      for (const group of manifest.groups) {
        if (group?.pubkey) {
          lines.push(`        pubkey=${group.pubkey}`);
        }
      }
      lines.push(`    }`);
    }
    if (manifest.permissions?.length) {
      lines.push(`    permissions {`);
      for (const perm of manifest.permissions) {
        const contractPart =
          typeof perm.contract === "string"
            ? `contract=${perm.contract}`
            : perm.contract?.hash
              ? `contract=hash:${perm.contract.hash}`
              : perm.contract?.group
                ? `contract=group:${perm.contract.group}`
                : `contract=${JSON.stringify(perm.contract)}`;
        const methodsPart =
          perm.methods === "*"
            ? "methods=*"
            : Array.isArray(perm.methods)
              ? `methods=[${perm.methods.map((m) => `"${m}"`).join(", ")}]`
              : `methods=${JSON.stringify(perm.methods)}`;
        lines.push(`        ${contractPart} ${methodsPart}`);
      }
      lines.push(`    }`);
    }
    if (manifest.trusts !== null && manifest.trusts !== undefined) {
      const formatted = formatManifestTrusts(manifest.trusts);
      if (formatted !== null) {
        lines.push(`    trusts = ${formatted};`);
      }
    }
    if (manifest.extra && typeof manifest.extra === "object" && !Array.isArray(manifest.extra)) {
      for (const [key, value] of Object.entries(manifest.extra)) {
        const rendered = renderExtraScalar(value);
        if (rendered !== null) {
          lines.push(`    // ${key}: ${rendered}`);
        }
      }
    }
    if (manifest.abi?.methods?.length) {
      lines.push(`    // ABI methods`);
      for (const method of manifest.abi.methods) {
        const params = method.parameters
          ?.map((p) => `${sanitizeIdentifier(p.name)}: ${formatManifestType(p.kind)}`)
          .join(", ") ?? "";
        // Always show `-> type`, including `-> void`, in the ABI summary
        // so the manifest contract surface is fully explicit. The lifted
        // method body still omits `-> void` for idiomatic readability.
        const returnType = formatManifestType(method.returnType ?? "Void");
        // Build the trailing meta-comment with the same shape as the
        // Rust port: when the manifest method name has chars that
        // sanitise away (e.g. `-`), surface a `manifest "Original"`
        // entry so the original identifier is recoverable. Then
        // `safe` (if `safe: true`) and `offset N` join with `, `.
        const sanitisedName = sanitizeIdentifier(method.name);
        const meta = [];
        if (sanitisedName !== method.name) {
          meta.push(`manifest ${JSON.stringify(method.name)}`);
        }
        if (method.safe) {
          meta.push("safe");
        }
        if (typeof method.offset === "number") {
          meta.push(`offset ${method.offset}`);
        }
        const metaComment = meta.length > 0 ? ` // ${meta.join(", ")}` : "";
        lines.push(`    fn ${sanitisedName}(${params}) -> ${returnType};${metaComment}`);
      }
    }
    if (manifest.abi?.events?.length) {
      lines.push(`    // ABI events`);
      for (const event of manifest.abi.events) {
        const params = event.parameters
          ?.map((p) => `${sanitizeIdentifier(p.name)}: ${formatManifestType(p.kind)}`)
          .join(", ") ?? "";
        const sanitised = sanitizeIdentifier(event.name);
        // Mirror the Rust manifest summary: when the sanitised
        // identifier differs from the raw manifest name, append a
        // `// manifest "Original"` annotation so the original
        // identifier is recoverable from the lifted source.
        const note = sanitised !== event.name ? ` // manifest ${JSON.stringify(event.name)}` : "";
        lines.push(`    event ${sanitised}(${params});${note}`);
      }
    }
  }
  // Method tokens declared in the NEF — surface them whether or not a
  // manifest was supplied, mirroring the Rust contract header.
  const methodTokens = context?.methodTokens ?? [];
  if (methodTokens.length > 0) {
    lines.push(`    // method tokens declared in NEF`);
    for (const token of methodTokens) {
      const hint = describeMethodToken(token.hash, token.method);
      const contractNote = hint ? ` (${hint.formattedLabel(token.method)})` : "";
      const flagsHex = token.callFlags.toString(16).padStart(2, "0").toUpperCase();
      lines.push(
        `    // ${token.method}${contractNote} hash=${upperHex(token.hash)} ` +
          `params=${token.parametersCount} returns=${token.hasReturnValue} ` +
          `flags=0x${flagsHex} (${describeCallFlags(token.callFlags)})`,
      );
      if (hint && !hint.hasExactMethod()) {
        lines.push(
          `    // warning: native contract ${hint.contract} does not expose method ${token.method}`,
        );
      }
    }
  }
  // Single trailing blank line separating header from method bodies.
  // Mirrors `writeln!(output)` at the end of Rust's
  // `write_contract_header` — emitted unconditionally regardless of
  // whether a manifest or method tokens were rendered.
  lines.push("");

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
      lines.push("        // no instructions decoded");
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
function renderExtraScalar(value) {
  if (typeof value === "string") return value;
  if (typeof value === "boolean") return value.toString();
  if (typeof value === "number" && Number.isFinite(value)) return value.toString();
  if (typeof value === "bigint") return value.toString();
  return null;
}

function formatManifestTrusts(trusts) {
  if (trusts === "*") {
    return "*";
  }
  if (Array.isArray(trusts)) {
    if (trusts.length === 0) {
      return null;
    }
    if (trusts.every((entry) => typeof entry === "string")) {
      return `[${trusts.map((entry) => `"${entry}"`).join(", ")}]`;
    }
    return JSON.stringify(trusts);
  }
  if (trusts && typeof trusts === "object") {
    const structured = formatStructuredTrusts(trusts);
    if (structured !== null) {
      return structured;
    }
  }
  return JSON.stringify(trusts);
}

function formatStructuredTrusts(object) {
  const allowedKeys = new Set(["hashes", "groups"]);
  for (const key of Object.keys(object)) {
    if (!allowedKeys.has(key)) {
      return null;
    }
  }
  const hashes = parseTypedTrustEntries(object.hashes, "hash");
  if (hashes === null) {
    return null;
  }
  const groups = parseTypedTrustEntries(object.groups, "group");
  if (groups === null) {
    return null;
  }
  return `[${[...hashes, ...groups].join(", ")}]`;
}

function parseTypedTrustEntries(value, prefix) {
  if (value === undefined || value === null) {
    return [];
  }
  if (!Array.isArray(value)) {
    return null;
  }
  const entries = [];
  for (const entry of value) {
    if (typeof entry !== "string") {
      return null;
    }
    entries.push(`${prefix}:${entry}`);
  }
  return entries;
}

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
  return `fn ${name}(${args})`;
}

export function liftMethodBody(
  instructions,
  manifestMethod = null,
  context = emptyContext(),
  methodOffset = instructions[0]?.offset ?? 0,
) {
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
