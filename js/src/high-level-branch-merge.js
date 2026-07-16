/**
 * Reconcile operand-stack values at the merge after a structured branch.
 *
 * Neo bytecode commonly computes a value in one arm and `null` in the other,
 * then stores the merged value immediately after the branch. The high-level
 * renderer must materialize that phi-like value once per arm; emitting each
 * arm's expression as a bare statement loses the value and causes a later
 * consumer to surface `???`.
 */
export function mergeBranchStacks(
  prefixState,
  thenState,
  elseState,
  thenTerminates,
  elseTerminates,
) {
  const liveThen = thenTerminates ? null : thenState.stack;
  const liveElse = elseTerminates ? null : elseState.stack;
  const reference = liveThen ?? liveElse ?? prefixState.stack;
  if (!reference) {
    return null;
  }

  const targetLength = Math.max(
    liveThen?.length ?? 0,
    liveElse?.length ?? 0,
    reference.length,
  );
  if (
    liveThen &&
    liveElse &&
    liveThen.length !== liveElse.length &&
    !isStackPrefix(
      liveThen.length < liveElse.length ? liveThen : liveElse,
      liveThen.length < liveElse.length ? liveElse : liveThen,
    )
  ) {
    return null;
  }
  if (liveThen && !liveElse && !elseTerminates) return null;
  if (liveElse && !liveThen && !thenTerminates) return null;

  if ((liveThen && liveThen.length !== targetLength) ||
      (liveElse && liveElse.length !== targetLength)) {
    const shorter = liveThen?.length === targetLength ? liveElse : liveThen;
    if (shorter && !isStackPrefix(shorter, reference)) {
      return null;
    }
  }

  const mergedStack = [];
  const declarations = [];
  const thenAssignments = [];
  const elseAssignments = [];
  let nextTempId = Math.max(
    prefixState.nextTempId,
    thenState.nextTempId,
    elseState.nextTempId,
  );

  for (let index = 0; index < targetLength; index += 1) {
    const thenValue = branchValueAt(liveThen, index, targetLength);
    const elseValue = branchValueAt(liveElse, index, targetLength);
    const values = [thenValue, elseValue].filter((value) => value !== undefined);
    if (values.some((value) => value === "???")) {
      return null;
    }
    const first = values[0];
    if (values.length === 0 || first === undefined) {
      return null;
    }
    if (values.every((value) => value === first)) {
      mergedStack.push(first);
      continue;
    }

    const temporary = `t${nextTempId}`;
    nextTempId += 1;
    declarations.push(`let ${temporary} = null;`);
    if (thenValue !== undefined) thenAssignments.push(`${temporary} = ${thenValue};`);
    if (elseValue !== undefined) elseAssignments.push(`${temporary} = ${elseValue};`);
    mergedStack.push(temporary);
  }

  return {
    mergedStack,
    declarations,
    thenAssignments,
    elseAssignments,
    nextTempId,
  };
}

function branchValueAt(stack, index, targetLength) {
  if (!stack) return undefined;
  if (index < stack.length) return stack[index];
  if (stack.length > 0 && stack.length < targetLength) return stack.at(-1);
  return undefined;
}

function isStackPrefix(shorter, longer) {
  return shorter.every((value, index) => value === longer[index]);
}
