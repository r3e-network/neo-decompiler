import { scanSlotCounts, scanStaticSlotCount } from "./util.js";

export function buildXrefs(instructions, methodGroups) {
  const staticCount = scanStaticSlotCount(instructions);
  return {
    methods: methodGroups.map((group) => {
      const slice = group.instructions;
      const [localCount, argCount] = scanSlotCounts(slice);
      const locals = Array.from({ length: localCount }, (_, index) => ({
        index,
        reads: [],
        writes: [],
      }));
      const argumentsXrefs = Array.from({ length: argCount }, (_, index) => ({
        index,
        reads: [],
        writes: [],
      }));
      const statics = Array.from({ length: staticCount }, (_, index) => ({
        index,
        reads: [],
        writes: [],
      }));

      for (const instruction of slice) {
        const access = slotAccess(instruction);
        if (!access) continue;
        const target =
          access.kind === "local"
            ? locals
            : access.kind === "argument"
              ? argumentsXrefs
              : statics;
        while (target.length <= access.index) {
          target.push({ index: target.length, reads: [], writes: [] });
        }
        if (access.isWrite) {
          target[access.index].writes.push(instruction.offset);
        } else {
          target[access.index].reads.push(instruction.offset);
        }
      }

      return {
        method: { offset: group.start, name: group.name },
        locals,
        arguments: argumentsXrefs,
        statics,
      };
    }),
  };
}

const LDLOC_NUM_RE = /^LDLOC\d+$/u;
const STLOC_NUM_RE = /^STLOC\d+$/u;
const LDARG_NUM_RE = /^LDARG\d+$/u;
const STARG_NUM_RE = /^STARG\d+$/u;
const LDSFLD_NUM_RE = /^LDSFLD\d+$/u;
const STSFLD_NUM_RE = /^STSFLD\d+$/u;

function slotAccess(instruction) {
  const mnemonic = instruction.opcode.mnemonic;
  if (LDLOC_NUM_RE.test(mnemonic)) {
    return { kind: "local", index: Number(mnemonic.slice(5)), isWrite: false };
  }
  if (mnemonic === "LDLOC" && instruction.operand?.kind === "U8") {
    return { kind: "local", index: instruction.operand.value, isWrite: false };
  }
  if (STLOC_NUM_RE.test(mnemonic)) {
    return { kind: "local", index: Number(mnemonic.slice(5)), isWrite: true };
  }
  if (mnemonic === "STLOC" && instruction.operand?.kind === "U8") {
    return { kind: "local", index: instruction.operand.value, isWrite: true };
  }

  if (LDARG_NUM_RE.test(mnemonic)) {
    return { kind: "argument", index: Number(mnemonic.slice(5)), isWrite: false };
  }
  if (mnemonic === "LDARG" && instruction.operand?.kind === "U8") {
    return { kind: "argument", index: instruction.operand.value, isWrite: false };
  }
  if (STARG_NUM_RE.test(mnemonic)) {
    return { kind: "argument", index: Number(mnemonic.slice(5)), isWrite: true };
  }
  if (mnemonic === "STARG" && instruction.operand?.kind === "U8") {
    return { kind: "argument", index: instruction.operand.value, isWrite: true };
  }

  if (LDSFLD_NUM_RE.test(mnemonic)) {
    return { kind: "static", index: Number(mnemonic.slice(6)), isWrite: false };
  }
  if (mnemonic === "LDSFLD" && instruction.operand?.kind === "U8") {
    return { kind: "static", index: instruction.operand.value, isWrite: false };
  }
  if (STSFLD_NUM_RE.test(mnemonic)) {
    return { kind: "static", index: Number(mnemonic.slice(6)), isWrite: true };
  }
  if (mnemonic === "STSFLD" && instruction.operand?.kind === "U8") {
    return { kind: "static", index: instruction.operand.value, isWrite: true };
  }
  return null;
}

