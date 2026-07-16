import { liftMethodBody } from "./high-level.js";

export function inferMethodContracts(methodGroups, context, callGraph) {
  const orderedGroups = [...methodGroups].sort(
    (left, right) => left.start - right.start,
  );
  const groupsByOffset = new Map(orderedGroups.map((group) => [group.start, group]));
  const byOffset = new Map();
  for (const group of orderedGroups) {
    const declaredReturnType = group.source?.returnType;
    const returnBehavior =
      typeof declaredReturnType === "string"
        ? declaredReturnType.toLowerCase() === "void"
          ? "void"
          : "value"
        : "unknown";
    byOffset.set(group.start, {
      method: { offset: group.start, name: group.name },
      argumentCount: context.methodArgCountsByOffset.get(group.start) ?? 0,
      returnBehavior,
    });
  }

  const methodReturnsValueByOffset = new Map(
    [...byOffset].map(([offset, contract]) => [offset, contract.returnBehavior !== "void"]),
  );
  const methodNeverReturnsByOffset = inferNeverReturningMethods(orderedGroups);
  const candidates = new Set(
    (callGraph?.edges ?? [])
      .filter(
        (edge) =>
          edge.target.kind === "Internal" &&
          byOffset.get(edge.target.method.offset)?.returnBehavior === "unknown",
      )
      .map((edge) => edge.target.method.offset),
  );
  const evolvingContext = {
    ...context,
    methodContractsByOffset: byOffset,
    methodReturnsValueByOffset,
    methodNeverReturnsByOffset,
  };

  while (true) {
    const newlyVoid = [];
    for (const offset of candidates) {
      const contract = byOffset.get(offset);
      if (contract?.returnBehavior !== "unknown") {
        continue;
      }
      const group = groupsByOffset.get(offset);
      if (!group) {
        continue;
      }
      // Exception regions can restore an operand-stack value at ENDFINALLY
      // even when the linear high-level pass cannot see an explicit producer
      // immediately before RET. Do not infer `void` from that incomplete
      // view; keeping the contract unknown is conservative and lets callers
      // preserve the helper result instead of emitting a stack-underflow `???`.
      if (group.instructions.some((instruction) =>
        ["TRY", "TRY_L", "ENDTRY", "ENDTRY_L", "ENDFINALLY"].includes(
          instruction.opcode.mnemonic,
        ))) {
        continue;
      }
      const result = liftMethodBody(group.instructions, null, evolvingContext, group.start);
      const returns = result.statements
        .flatMap((statement) => statement.split("\n"))
        .map((line) => line.trim())
        .filter((line) => /^return(?:\s+.+)?;$/.test(line));
      if (returns.length > 0 && returns.every((line) => line === "return;")) {
        newlyVoid.push(offset);
      }
    }

    if (newlyVoid.length === 0) {
      break;
    }
    for (const offset of newlyVoid) {
      byOffset.get(offset).returnBehavior = "void";
      methodReturnsValueByOffset.set(offset, false);
    }
  }

  return {
    methodContracts: { methods: [...byOffset.values()] },
    methodContractsByOffset: byOffset,
    methodReturnsValueByOffset,
    methodNeverReturnsByOffset,
  };
}

// A private helper whose decoded method slice ends in a VM terminator without
// any RET cannot produce a normal value. Keep this fact separate from the
// public tri-state return contract: a manifest can still declare an integer
// return type even though every execution path aborts or throws.
function inferNeverReturningMethods(groups) {
  const neverReturns = new Map();
  for (const group of groups) {
    const instructions = group.instructions ?? [];
    const lastMnemonic = instructions.at(-1)?.opcode?.mnemonic;
    if (
      instructions.length > 0 &&
      !instructions.some((instruction) => instruction.opcode.mnemonic === "RET") &&
      !instructions.some((instruction) => NON_RETURNING_BRANCHES.has(instruction.opcode.mnemonic)) &&
      ["THROW", "ABORT", "ABORTMSG"].includes(lastMnemonic)
    ) {
      neverReturns.set(group.start, true);
    }
  }
  return neverReturns;
}

// Only classify straight-line terminal helpers here. A branch or exception
// context could reach a normal continuation that this small summary cannot
// prove, so those methods remain conservative.
const NON_RETURNING_BRANCHES = new Set([
  "JMP", "JMP_L", "JMPIF", "JMPIF_L", "JMPIFNOT", "JMPIFNOT_L",
  "JMPEQ", "JMPEQ_L", "JMPNE", "JMPNE_L", "JMPGT", "JMPGT_L",
  "JMPGE", "JMPGE_L", "JMPLT", "JMPLT_L", "JMPLE", "JMPLE_L",
  "TRY", "TRY_L", "ENDTRY", "ENDTRY_L", "ENDFINALLY",
]);
