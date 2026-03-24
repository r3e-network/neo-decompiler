import { makeUniqueIdentifier, sanitizeIdentifier } from "./manifest.js";
import { jumpTarget } from "./high-level-utils.js";

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

  return groups.map((group, index) => ({
    ...group,
    end: groups[index + 1]?.start ?? (instructions.at(-1)?.offset ?? group.start) + 1,
    instructions: instructions.filter(
      (instruction) =>
        instruction.offset >= group.start &&
        instruction.offset < (groups[index + 1]?.start ?? Number.POSITIVE_INFINITY),
    ),
  }));
}
