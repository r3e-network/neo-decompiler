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
  };
}
