import { SYSCALLS } from "./generated/syscalls.js";
import { inferMethodContracts } from "./method-contracts.js";
import { describeMethodToken } from "./native-contracts.js";

// Build the shared context consumed by high-level control-flow lifting and
// C# rendering. Keeping this separate from the public API entry points makes
// the analysis contract explicit and keeps decompile orchestration small.
export function buildHighLevelContext(
  methodGroups,
  contractGroups,
  nef,
  options = {},
  callGraph = null,
) {
  const entryOffset = methodGroups[0]?.start ?? 0;
  const context = {
    methodLabelsByOffset: new Map(methodGroups.map((group) => [group.start, group.name])),
    methodArgCountsByOffset: new Map(
      methodGroups.map((group) => [group.start, inferMethodArgCount(group, entryOffset)]),
    ),
    callaTargetsByOffset: new Map(
      (callGraph?.edges ?? [])
        .filter((edge) => edge.opcode === "CALLA" && edge.target.kind === "Internal")
        .map((edge) => [edge.callOffset, edge.target.method.offset]),
    ),
    // Resolve token-call labels through the native-contract describe table so
    // known calls render as `GasToken::Transfer(...)` instead of only the
    // raw method name. Unknown or restricted tokens retain their raw label.
    calltLabels: nef.methodTokens.map((token) => {
      const hint = token.callFlags === 0x0F
        ? describeMethodToken(token.hash, token.method)
        : null;
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
      // `clean` is a readability shorthand. Keep the individual option so
      // future postprocess passes can compose without changing this API.
      inlineSingleUseTemps:
        !!options.inlineSingleUseTemps || !!options.clean,
      clean: !!options.clean,
    },
  };
  return {
    ...context,
    ...inferMethodContracts(contractGroups, context, callGraph),
  };
}

function inferMethodArgCount(group, entryOffset) {
  if (group.source?.parameters) {
    return group.source.parameters.length;
  }
  const first = group.instructions[0];
  if (
    first?.opcode?.mnemonic === "INITSLOT" &&
    first.operand?.kind === "Bytes" &&
    first.operand.value.length >= 2
  ) {
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
  if (mnemonic === "POPITEM") return { pops: 1, pushes: 1 };
  if (mnemonic === "DUP") return { pops: 1, pushes: 2 };
  if (mnemonic === "OVER") return { pops: 2, pushes: 3 };
  if (mnemonic === "SWAP") return { pops: 2, pushes: 2 };
  if (mnemonic === "ROT") return { pops: 3, pushes: 3 };
  if (mnemonic === "TUCK") return { pops: 2, pushes: 3 };
  if (mnemonic === "NIP") return { pops: 2, pushes: 1 };
  if (["REVERSE3", "REVERSE4"].includes(mnemonic)) {
    const width = mnemonic === "REVERSE3" ? 3 : 4;
    return { pops: width, pushes: width };
  }
  if (["PICK", "ROLL", "REVERSEN"].includes(mnemonic)) {
    return { pops: 1, pushes: 1 };
  }
  if (mnemonic === "XDROP") return { pops: 1, pushes: 0 };
  if (mnemonic === "SETITEM") return { pops: 3, pushes: 0 };
  if (["APPEND", "REMOVE", "CLEARITEMS", "REVERSEITEMS"].includes(mnemonic)) {
    return { pops: mnemonic === "APPEND" || mnemonic === "REMOVE" ? 2 : 1, pushes: 0 };
  }
  if (
    [
      "ISNULL",
      "NOT",
      "NEGATE",
      "ABS",
      "SIGN",
      "INVERT",
      "INC",
      "DEC",
      "SQRT",
      "CONVERT",
      "ISTYPE",
      "SIZE",
      "DEPTH",
    ].includes(mnemonic)
  ) {
    return { pops: mnemonic === "DEPTH" ? 0 : 1, pushes: 1 };
  }
  if (["WITHIN", "MODMUL", "MODPOW"].includes(mnemonic)) {
    return { pops: 3, pushes: 1 };
  }
  if (["SHL", "SHR"].includes(mnemonic)) return { pops: 2, pushes: 1 };
  return null;
}
