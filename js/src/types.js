export function inferTypes(instructions, methodGroups, manifest = null) {
  const staticCount = scanStaticSlotCount(instructions);
  const statics = Array.from({ length: staticCount }, () => "unknown");

  const methods = methodGroups.map((group) => {
    const [localCount, argCount] = scanSlotCounts(group.instructions);
    const locals = Array.from({ length: localCount }, () => "unknown");
    const argumentsTypes = Array.from({ length: argCount }, () => "unknown");

    if (group.source?.parameters) {
      while (argumentsTypes.length < group.source.parameters.length) {
        argumentsTypes.push("unknown");
      }
      for (let index = 0; index < group.source.parameters.length; index += 1) {
        argumentsTypes[index] = manifestType(group.source.parameters[index].kind);
      }
    }

    for (let index = 0; index < group.instructions.length; index += 1) {
      const instruction = group.instructions[index];
      const next = group.instructions[index + 1];
      const store = next?.opcode?.mnemonic;
      if (instruction.opcode.mnemonic === "NEWMAP") {
        assignStoredType(locals, statics, store, next, "map");
      }
      if (
        instruction.opcode.mnemonic === "NEWSTRUCT0" ||
        instruction.opcode.mnemonic === "NEWSTRUCT" ||
        instruction.opcode.mnemonic === "PACKSTRUCT"
      ) {
        assignStoredType(locals, statics, store, next, "struct");
      }
      if (
        instruction.opcode.mnemonic === "NEWARRAY0" ||
        instruction.opcode.mnemonic === "NEWARRAY" ||
        instruction.opcode.mnemonic === "NEWARRAY_T"
      ) {
        assignStoredType(locals, statics, store, next, "array");
      }
      if (instruction.opcode.mnemonic === "NEWBUFFER") {
        assignStoredType(locals, statics, store, next, "buffer");
      }
      if (instruction.opcode.mnemonic === "PACKMAP") {
        assignStoredType(locals, statics, store, next, "map");
      }
      if (instruction.opcode.mnemonic === "CONVERT") {
        const target = convertTargetType(instruction.operand);
        if (target) {
          assignStoredType(locals, statics, store, next, target);
        }
      }
    }

    return {
      method: { offset: group.start, name: group.name },
      arguments: argumentsTypes,
      locals,
    };
  });

  return { methods, statics };
}

function scanSlotCounts(instructions) {
  for (const instruction of instructions) {
    if (
      instruction.opcode.mnemonic === "INITSLOT" &&
      instruction.operand?.kind === "Bytes" &&
      instruction.operand.value.length >= 2
    ) {
      return [instruction.operand.value[0], instruction.operand.value[1]];
    }
  }

  let maxLocal = -1;
  let maxArg = -1;
  for (const instruction of instructions) {
    const mnemonic = instruction.opcode.mnemonic;
    if (/^(?:LD|ST)LOC(?:\d+)?$/u.test(mnemonic)) {
      maxLocal = Math.max(maxLocal, slotIndex(mnemonic, instruction));
    }
    if (/^(?:LD|ST)ARG(?:\d+)?$/u.test(mnemonic)) {
      maxArg = Math.max(maxArg, slotIndex(mnemonic, instruction));
    }
  }
  return [maxLocal + 1, maxArg + 1];
}

function scanStaticSlotCount(instructions) {
  for (const instruction of instructions) {
    if (instruction.opcode.mnemonic === "INITSSLOT" && instruction.operand?.kind === "U8") {
      return instruction.operand.value;
    }
  }
  return 0;
}

function slotIndex(mnemonic, instruction) {
  const exact = mnemonic.match(/(?:LD|ST)(?:LOC|ARG|SFLD)(\d+)$/u);
  if (exact) {
    return Number(exact[1]);
  }
  if (instruction.operand?.kind === "U8") {
    return instruction.operand.value;
  }
  return 0;
}

function assignStoredType(locals, statics, store, instruction, type) {
  if (store?.startsWith("STLOC")) {
    locals[slotIndex(store, instruction)] = type;
  } else if (store?.startsWith("STSFLD")) {
    statics[slotIndex(store, instruction)] = type;
  }
}

function manifestType(kind) {
  const normalized = String(kind).toLowerCase();
  if (normalized === "boolean") return "bool";
  if (normalized === "integer") return "integer";
  if (
    normalized === "string" ||
    normalized === "bytearray" ||
    normalized === "signature" ||
    normalized === "hash160" ||
    normalized === "hash256"
  ) {
    return "bytestring";
  }
  if (normalized === "array") return "array";
  if (normalized === "map") return "map";
  if (normalized === "interopinterface") return "interopinterface";
  return "unknown";
}

function convertTargetType(operand) {
  if (!operand || (operand.kind !== "U8" && operand.kind !== "I8")) {
    return null;
  }
  const byte = operand.kind === "U8" ? operand.value : operand.value & 0xff;
  const map = {
    0x00: "any",
    0x10: "pointer",
    0x20: "bool",
    0x21: "integer",
    0x28: "bytestring",
    0x30: "buffer",
    0x40: "array",
    0x41: "struct",
    0x48: "map",
    0x60: "interopinterface",
  };
  return map[byte] ?? null;
}
