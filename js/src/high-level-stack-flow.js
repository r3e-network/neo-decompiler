/**
 * Stack snapshots used by the linear control-flow fallback.
 *
 * The fallback walks instructions in source order even when a jump skips
 * ahead. These helpers preserve values that reach forward labels and merge
 * only generated temporaries; ambiguous joins remain fail-closed.
 */

export function recordStackSnapshot(state, target) {
  if (!state.labelTargets.has(target)) {
    return;
  }
  const snapshot = [...state.stack];
  if (!state.stackSnapshotsByLabel.has(target)) {
    state.stackSnapshotsByLabel.set(target, snapshot);
    return;
  }
  const previous = state.stackSnapshotsByLabel.get(target);
  if (previous === null) {
    return;
  }
  if (
    previous.length !== snapshot.length ||
    previous.some((value, index) => value !== snapshot[index])
  ) {
    if (previous.length !== snapshot.length) {
      state.stackSnapshotsByLabel.set(target, null);
      return;
    }
    const assignments = [];
    for (let index = 0; index < previous.length; index += 1) {
      const canonical = previous[index];
      const incoming = snapshot[index];
      if (canonical === incoming) {
        continue;
      }
      if (!isMergeTemporary(canonical) || incoming === "???") {
        state.stackSnapshotsByLabel.set(target, null);
        return;
      }
      assignments.push(`${canonical} = ${incoming};`);
    }
    // The assignment runs on this incoming edge before its jump. The
    // fall-through edge is reconciled by restoreStackAtLabel.
    state.statements.push(...assignments);
  }
}

export function restoreStackAtLabel(state, offset) {
  if (!state.stackSnapshotsByLabel.has(offset)) {
    return;
  }
  const snapshot = state.stackSnapshotsByLabel.get(offset);
  if (snapshot === null) {
    // Conflicting incoming stacks cannot be reconciled without inventing a
    // value. Clear the simulated stack so subsequent consumers surface their
    // normal underflow placeholder instead.
    state.stack.length = 0;
    return;
  }

  if (state.stack.length === 0) {
    state.stack.push(...snapshot);
    return;
  }
  if (state.stack.length !== snapshot.length) {
    state.stack.length = 0;
    return;
  }

  // When both paths reach the label with the same depth but different
  // expressions, assign the fall-through value to the saved identifier before
  // the label. A jump skips that assignment, while the fall-through path runs
  // it, yielding one stable value for consumers after the merge.
  const assignments = [];
  for (let index = 0; index < snapshot.length; index += 1) {
    const saved = snapshot[index];
    const current = state.stack[index];
    if (saved === current) {
      continue;
    }
    if (isMergeTemporary(saved) && current !== "???") {
      assignments.push(`${saved} = ${current};`);
    } else {
      state.stack.length = 0;
      return;
    }
  }
  state.statements.push(...assignments);
  state.stack.length = 0;
  state.stack.push(...snapshot);
}

function isMergeTemporary(value) {
  return /^t\d+$/u.test(value);
}
