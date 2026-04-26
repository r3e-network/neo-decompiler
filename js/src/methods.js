import { makeUniqueIdentifier, sanitizeIdentifier } from "./manifest.js";
import { jumpTarget } from "./high-level-utils.js";

const TERMINATOR_MNEMONICS = new Set(["RET", "THROW", "ABORT", "ABORTMSG"]);
const BRANCH_MNEMONICS = new Set([
  "JMP", "JMP_L", "JMPIF", "JMPIF_L", "JMPIFNOT", "JMPIFNOT_L",
  "JMPEQ", "JMPEQ_L", "JMPNE", "JMPNE_L",
  "JMPGT", "JMPGT_L", "JMPGE", "JMPGE_L",
  "JMPLT", "JMPLT_L", "JMPLE", "JMPLE_L",
  "CALL", "CALL_L", "ENDTRY", "ENDTRY_L",
]);

function collectPostTerminatorStarts(instructions, starts) {
  // Collect (sourceOffset, targetOffset) edges. The set of targets answers
  // "is X branched into?" and the source list answers "does anything in
  // range [a,b] branch to a target in (b, end)?" — both needed to mirror
  // the Rust port's "detached tail" detection.
  const edges = [];
  for (const instruction of instructions) {
    if (BRANCH_MNEMONICS.has(instruction.opcode.mnemonic)) {
      const target = jumpTarget(instruction);
      if (target !== null) edges.push([instruction.offset, target]);
    } else if (
      instruction.opcode.mnemonic === "TRY" &&
      instruction.operand?.kind === "Bytes" &&
      instruction.operand.value.length === 2
    ) {
      for (const byte of instruction.operand.value) {
        const signed = byte > 127 ? byte - 256 : byte;
        if (signed !== 0) edges.push([instruction.offset, instruction.offset + signed]);
      }
    } else if (
      instruction.opcode.mnemonic === "TRY_L" &&
      instruction.operand?.kind === "Bytes" &&
      instruction.operand.value.length === 8
    ) {
      const v = instruction.operand.value;
      for (const offset of [0, 4]) {
        const u = v[offset] | (v[offset + 1] << 8) | (v[offset + 2] << 16) | (v[offset + 3] << 24);
        if (u !== 0) edges.push([instruction.offset, instruction.offset + u]);
      }
    }
  }
  const branchTargets = new Set(edges.map(([_, t]) => t));

  // The "current method" range for a candidate split at offset X is the
  // span between the most-recent baseline start ≤ X and the next baseline
  // start > X. If any instruction in that range branches forward past X
  // (but within the range), the method body extends past X — bail.
  const baselineStartsSorted = [...starts].sort((a, b) => a - b);
  const lastStartLE = (offset) => {
    let lo = 0, hi = baselineStartsSorted.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (baselineStartsSorted[mid] <= offset) lo = mid + 1;
      else hi = mid;
    }
    return lo > 0 ? baselineStartsSorted[lo - 1] : 0;
  };
  const firstStartGT = (offset) => {
    for (const s of baselineStartsSorted) if (s > offset) return s;
    return Number.POSITIVE_INFINITY;
  };

  for (let i = 0; i + 1 < instructions.length; i++) {
    const current = instructions[i];
    const next = instructions[i + 1];
    if (!TERMINATOR_MNEMONICS.has(current.opcode.mnemonic)) continue;
    if (starts.has(next.offset)) continue;
    if (branchTargets.has(next.offset)) continue;

    const methodStart = lastStartLE(current.offset);
    const methodEnd = firstStartGT(methodStart);
    const reaches = edges.some(
      ([src, tgt]) =>
        src >= methodStart && src < methodEnd &&
        tgt > next.offset && tgt < methodEnd,
    );
    if (reaches) continue;

    starts.add(next.offset);
  }
}

export function buildMethodGroups(instructions, manifest) {
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
  collectPostTerminatorStarts(instructions, starts);

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
        offset === entryOffset ? "script_entry" : `sub_0x${offset.toString(16).padStart(4, "0")}`,
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
