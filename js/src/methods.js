import { makeUniqueIdentifier, sanitizeIdentifier } from "./manifest.js";
import { jumpTarget } from "./high-level-utils.js";
import { hexOffset } from "./util.js";

const TERMINATOR_MNEMONICS = new Set(["RET", "THROW", "ABORT", "ABORTMSG"]);
// Mirrors Rust's `collect_control_flow_edges`: the conditional/unconditional
// jump family plus ENDTRY/ENDTRY_L (TRY/TRY_L targets are decoded from their
// Bytes operand below). CALL/CALL_L are deliberately NOT in this set — Rust
// treats call targets as baseline method starts (`collect_call_targets`),
// not as intra-method control flow, so a CALL into a post-terminator tail
// must not suppress (or force) a split here.
const BRANCH_MNEMONICS = new Set([
  "JMP", "JMP_L", "JMPIF", "JMPIF_L", "JMPIFNOT", "JMPIFNOT_L",
  "JMPEQ", "JMPEQ_L", "JMPNE", "JMPNE_L",
  "JMPGT", "JMPGT_L", "JMPGE", "JMPGE_L",
  "JMPLT", "JMPLT_L", "JMPLE", "JMPLE_L",
  "ENDTRY", "ENDTRY_L",
]);

// Collect (sourceOffset, targetOffset) control-flow edges, mirroring Rust's
// `collect_control_flow_edges` + `relative_target_with_delta`: a target is
// only recorded when it is non-negative and lands on a decoded instruction
// boundary (`knownOffsets`).
function collectControlFlowEdges(instructions, knownOffsets) {
  const edges = [];
  const pushEdge = (source, delta) => {
    const target = source + delta;
    if (target >= 0 && knownOffsets.has(target)) {
      edges.push([source, target]);
    }
  };
  for (const instruction of instructions) {
    const mnemonic = instruction.opcode.mnemonic;
    if (BRANCH_MNEMONICS.has(mnemonic)) {
      const operand = instruction.operand;
      if (operand?.kind === "Jump" || operand?.kind === "Jump32") {
        pushEdge(instruction.offset, operand.value);
      }
    } else if (
      mnemonic === "TRY" &&
      instruction.operand?.kind === "Bytes" &&
      instruction.operand.value.length === 2
    ) {
      for (const byte of instruction.operand.value) {
        pushEdge(instruction.offset, byte > 127 ? byte - 256 : byte);
      }
    } else if (
      mnemonic === "TRY_L" &&
      instruction.operand?.kind === "Bytes" &&
      instruction.operand.value.length === 8
    ) {
      const v = instruction.operand.value;
      for (const base of [0, 4]) {
        const delta =
          v[base] | (v[base + 1] << 8) | (v[base + 2] << 16) | (v[base + 3] << 24);
        pushEdge(instruction.offset, delta);
      }
    }
  }
  return edges;
}

// Mirror of Rust's `collect_post_ret_method_offsets`. For each instruction
// `next` that follows a terminator, find the baseline method range
// [methodStart, methodEnd) containing the terminator and classify the
// incoming edges of `next` by source:
//   - an edge from ANOTHER baseline method is positive evidence FOR a
//     split (only foreign code reaches the tail, so it cannot be part of
//     the current method's straight-line body);
//   - an edge from the SAME baseline method keeps the tail attached;
//   - with no incoming edges at all, the tail splits unless something in
//     the current method branches forward past `next` but still inside the
//     range (e.g. a catch handler reached only via the TRY operand).
function collectPostTerminatorStarts(instructions, starts) {
  const knownOffsets = new Set(instructions.map((instruction) => instruction.offset));
  const edges = collectControlFlowEdges(instructions, knownOffsets);

  // target offset -> [source offsets] (Rust's `edges_by_target`).
  const edgesByTarget = new Map();
  for (const [source, target] of edges) {
    const sources = edgesByTarget.get(target);
    if (sources) sources.push(source);
    else edgesByTarget.set(target, [source]);
  }

  // Snapshot the baseline starts before any additions: Rust passes the
  // pre-extension `baseline_starts` clone, so offsets promoted by this
  // very loop must not shrink the ranges of later candidates.
  const baselineStartsSorted = [...starts].sort((a, b) => a - b);
  const lastStartLE = (offset) => {
    let lo = 0, hi = baselineStartsSorted.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (baselineStartsSorted[mid] <= offset) lo = mid + 1;
      else hi = mid;
    }
    return lo > 0 ? baselineStartsSorted[lo - 1] : null;
  };
  const firstStartGT = (offset) => {
    for (const s of baselineStartsSorted) if (s > offset) return s;
    return Number.POSITIVE_INFINITY;
  };

  for (let i = 0; i + 1 < instructions.length; i++) {
    const current = instructions[i];
    const next = instructions[i + 1];
    if (!TERMINATOR_MNEMONICS.has(current.opcode.mnemonic)) continue;

    // Rust: `baseline_starts.range(..=current.offset).next_back()
    //        .unwrap_or(current.offset)` and the first start after it.
    const methodStart = lastStartLE(current.offset) ?? current.offset;
    const methodEnd = firstStartGT(methodStart);

    const incoming = edgesByTarget.get(next.offset) ?? [];
    const hasIncomingFromSameBaselineMethod = incoming.some(
      (source) => source >= methodStart && source < methodEnd,
    );
    const hasIncomingFromOtherBaselineMethod = incoming.some(
      (source) => source < methodStart || source >= methodEnd,
    );
    const hasSameBaselineIncomingLaterInRange = edges.some(
      ([source, target]) =>
        source >= methodStart && source < methodEnd &&
        target > next.offset && target < methodEnd,
    );

    const detachedTailAfterTerminator =
      hasIncomingFromOtherBaselineMethod ||
      (!hasIncomingFromSameBaselineMethod && !hasSameBaselineIncomingLaterInRange);
    if (detachedTailAfterTerminator) {
      starts.add(next.offset);
    }
  }
}

export function buildMethodGroups(instructions, manifest, options = {}) {
  // `includePostTerminatorTails` defaults to true (presentation grouping). The
  // analysis path passes false to match the Rust port's `analysis::MethodTable`,
  // which excludes detached post-terminator tails that are only useful for
  // presentation-time rendering.
  const includePostTerminatorTails = options.includePostTerminatorTails ?? true;
  const entryOffset = instructions[0]?.offset ?? 0;
  const used = new Set();
  const methods = manifest?.abi?.methods ?? [];
  const manifestOffsets = methods
    .map((method, index) => ({ index, offset: method.offset }))
    .filter((entry) => Number.isInteger(entry.offset) && entry.offset >= 0)
    .sort((left, right) => left.offset - right.offset);
  const knownOffsets = new Set(instructions.map((instruction) => instruction.offset));
  const starts = new Set([entryOffset]);

  for (const { offset } of manifestOffsets) {
    starts.add(offset);
  }
  for (const instruction of instructions) {
    if (instruction.opcode.mnemonic === "INITSLOT") {
      starts.add(instruction.offset);
    }
    if (instruction.opcode.mnemonic === "CALL" || instruction.opcode.mnemonic === "CALL_L") {
      const target = jumpTarget(instruction);
      if (target !== null && knownOffsets.has(target)) {
        starts.add(target);
      }
    }
  }

  // Promote post-terminator detached tails to method starts, matching the
  // Rust port's `collect_post_ret_method_offsets`. A terminator is RET,
  // THROW, ABORT, or ABORTMSG; the next instruction after one is treated
  // as a fresh method entry when nothing branches into it from the
  // surrounding method body. Without this, contracts whose bytecode lays
  // out helpers as straight-line tails after the entry method's RET
  // collapse into a single function with multiple `return`s.
  //
  // This is a presentation-time heuristic: the Rust analysis grouping
  // (`analysis::MethodTable`) deliberately omits these tails, so callers that
  // want analysis-parity grouping pass `includePostTerminatorTails: false`.
  if (includePostTerminatorTails) {
    collectPostTerminatorStarts(instructions, starts);
  }

  const groups = [];
  const offsets = [...starts].sort((left, right) => left - right);
  const manifestOffsetMap = new Map(manifestOffsets.map(({ offset, index }) => [offset, index]));
  const allManifestOffsetsMissing = manifestOffsets.length === 0;

  for (const offset of offsets) {
    if (offset === entryOffset && allManifestOffsetsMissing && methods[0]) {
      groups.push({
        start: offset,
        name: makeUniqueIdentifier(sanitizeIdentifier(methods[0].name), used),
        source: methods[0],
      });
      continue;
    }

    const manifestIndex = manifestOffsetMap.get(offset);
    if (manifestIndex !== undefined) {
      groups.push({
        start: offset,
        name: makeUniqueIdentifier(sanitizeIdentifier(methods[manifestIndex].name), used),
        source: methods[manifestIndex],
      });
      continue;
    }

    groups.push({
      start: offset,
      name: makeUniqueIdentifier(
        // Use uppercase hex (`0xABCD`) so the inferred-helper label
        // matches Rust's `format!("sub_0x{start:04X}")`. Earlier this
        // used `.toString(16).padStart(4, "0")` which lowercases A-F
        // and silently diverged from Rust whenever the offset
        // contained a hex letter (e.g. `sub_0x000a` vs Rust's
        // `sub_0x000A`).
        offset === entryOffset ? "script_entry" : `sub_0x${hexOffset(offset)}`,
        used,
      ),
      source: null,
    });
  }

  groups.sort((left, right) => left.start - right.start);

  // Single-pass partition: instructions are sorted by offset, so we walk once.
  let instrIdx = 0;
  return groups.map((group, index) => {
    const nextStart = groups[index + 1]?.start ?? Number.POSITIVE_INFINITY;
    const end = groups[index + 1]?.start ?? (instructions.at(-1)?.offset ?? group.start) + 1;
    const groupInstructions = [];
    while (instrIdx < instructions.length && instructions[instrIdx].offset < group.start) {
      instrIdx++;
    }
    while (instrIdx < instructions.length && instructions[instrIdx].offset < nextStart) {
      groupInstructions.push(instructions[instrIdx++]);
    }
    return { ...group, end, instructions: groupInstructions };
  });
}
