import { scanSlotCounts, scanStaticSlotCount } from "./util.js";

// Best-effort type recovery via a per-method stack simulation, mirroring the
// Rust core (src/decompiler/analysis/types.rs `infer_types_in_slice`) so the
// `analysis.types` output matches across the two ports. Conservative: falls
// back to "unknown"/"any" rather than guessing.

export function inferTypes(instructions, methodGroups, manifest = null) {
  const staticCount = scanStaticSlotCount(instructions);
  const statics = Array.from({ length: staticCount }, () => "unknown");

  const methods = methodGroups.map((group) => {
    const [localCount, argCount] = scanSlotCounts(group.instructions);
    const locals = Array.from({ length: localCount }, () => "unknown");
    const argumentsTypes = Array.from({ length: argCount }, () => "unknown");

    const params = group.source?.parameters;
    if (params) {
      while (argumentsTypes.length < params.length) argumentsTypes.push("unknown");
      for (let index = 0; index < params.length; index += 1) {
        argumentsTypes[index] = joinType(argumentsTypes[index], manifestType(params[index].kind));
      }
    }

    inferInSlice(group.instructions, locals, argumentsTypes, statics);

    return {
      method: { offset: group.start, name: group.name },
      arguments: argumentsTypes,
      locals,
    };
  });

  return { methods, statics };
}

// ValueType lattice merge — mirrors Rust ValueType::join.
function joinType(a, b) {
  if (a === b) return a;
  if (a === "unknown") return b;
  if (b === "unknown") return a;
  // (Null, _) | (_, Null) and every other heterogeneous pair widen to Any.
  return "any";
}

function inferInSlice(instructions, locals, args, statics) {
  const stack = [];
  const unk = () => ({ ty: "unknown", lit: null });
  const popU = () => stack.pop() ?? unk();
  const ensure = (slots, idx) => {
    while (slots.length <= idx) slots.push("unknown");
  };

  for (const ins of instructions) {
    const m = ins.opcode.mnemonic;

    // Slot init: extend the slot vectors if a wider count is declared.
    if (m === "INITSSLOT") {
      if (ins.operand?.kind === "U8" && ins.operand.value > 0) ensure(statics, ins.operand.value - 1);
      continue;
    }
    if (m === "INITSLOT") {
      const v = ins.operand;
      if (v?.kind === "Bytes" && v.value.length >= 2) {
        if (v.value[0] > 0) ensure(locals, v.value[0] - 1);
        if (v.value[1] > 0) ensure(args, v.value[1] - 1);
      }
      continue;
    }

    // Slot load/store (LDLOC*/STLOC*/LDARG*/STARG*/LDSFLD*/STSFLD*).
    const sm = /^(LD|ST)(LOC|ARG|SFLD)(\d*)$/u.exec(m);
    if (sm) {
      const slots = sm[2] === "LOC" ? locals : sm[2] === "ARG" ? args : statics;
      const idx = sm[3] !== "" ? Number(sm[3]) : ins.operand?.kind === "U8" ? ins.operand.value : null;
      if (sm[1] === "LD") {
        if (idx === null) {
          stack.push(unk());
        } else {
          ensure(slots, idx);
          stack.push({ ty: slots[idx], lit: null });
        }
      } else if (idx === null) {
        popU(); // STxxx with a non-U8 operand: consume the value, store nothing.
      } else {
        const value = popU();
        ensure(slots, idx);
        slots[idx] = joinType(slots[idx], value.ty);
      }
      continue;
    }

    switch (m) {
      // Literals
      case "PUSHNULL":
        stack.push({ ty: "null", lit: null });
        break;
      case "PUSHT":
      case "PUSHF":
        stack.push({ ty: "bool", lit: null });
        break;
      case "PUSHDATA1":
      case "PUSHDATA2":
      case "PUSHDATA4":
        stack.push({ ty: "bytestring", lit: null });
        break;
      case "PUSHINT8":
      case "PUSHINT16":
      case "PUSHINT32":
      case "PUSHINT64":
      case "PUSHINT128":
      case "PUSHINT256":
      case "PUSHM1":
      case "PUSH0":
      case "PUSH1":
      case "PUSH2":
      case "PUSH3":
      case "PUSH4":
      case "PUSH5":
      case "PUSH6":
      case "PUSH7":
      case "PUSH8":
      case "PUSH9":
      case "PUSH10":
      case "PUSH11":
      case "PUSH12":
      case "PUSH13":
      case "PUSH14":
      case "PUSH15":
      case "PUSH16":
        stack.push({ ty: "integer", lit: intLiteral(ins.operand) });
        break;
      case "PUSHA":
        stack.push({ ty: "pointer", lit: null });
        break;

      // Stack manipulation
      case "CLEAR":
        stack.length = 0;
        break;
      case "DEPTH":
        stack.push({ ty: "integer", lit: stack.length });
        break;
      case "DROP":
        popU();
        break;
      case "DUP":
        stack.push(stack.length ? { ...stack[stack.length - 1] } : unk());
        break;
      case "SWAP":
        if (stack.length >= 2) {
          const n = stack.length;
          const t = stack[n - 1];
          stack[n - 1] = stack[n - 2];
          stack[n - 2] = t;
        }
        break;
      case "OVER":
        stack.push(stack.length >= 2 ? { ...stack[stack.length - 2] } : unk());
        break;
      case "NIP":
        if (stack.length >= 2) stack.splice(stack.length - 2, 1);
        break;
      case "ROT": {
        // Pops unconditionally (matching Rust): on a short stack the available
        // elements are still removed, and nothing is pushed back.
        const top = stack.pop();
        const mid = stack.pop();
        const bottom = stack.pop();
        if (top !== undefined && mid !== undefined && bottom !== undefined) {
          stack.push(mid, top, bottom);
        }
        break;
      }
      case "TUCK": {
        const top = stack.pop();
        const second = stack.pop();
        if (top !== undefined && second !== undefined) {
          stack.push(top, second, { ...top });
        }
        break;
      }
      case "PICK": {
        const depth = asCount(popU().lit);
        if (depth !== null) {
          const pos = stack.length - 1 - depth;
          if (pos >= 0) {
            stack.push({ ...stack[pos] });
            break;
          }
        }
        stack.push(unk());
        break;
      }
      case "ROLL": {
        const depth = asCount(popU().lit);
        if (depth !== null && depth < stack.length) {
          const [value] = stack.splice(stack.length - 1 - depth, 1);
          stack.push(value);
        }
        break;
      }
      case "XDROP": {
        const depth = asCount(popU().lit);
        if (depth !== null && depth < stack.length) {
          stack.splice(stack.length - 1 - depth, 1);
          break;
        }
        popU();
        break;
      }
      case "REVERSE3":
        reverseTop(stack, 3);
        break;
      case "REVERSE4":
        reverseTop(stack, 4);
        break;
      case "REVERSEN": {
        const depth = asCount(popU().lit);
        if (depth !== null) reverseTop(stack, depth);
        break;
      }

      // Collections
      case "NEWARRAY0":
        stack.push({ ty: "array", lit: null });
        break;
      case "NEWARRAY":
      case "NEWARRAY_T":
        popU();
        stack.push({ ty: "array", lit: null });
        break;
      case "NEWMAP":
        stack.push({ ty: "map", lit: null });
        break;
      case "NEWSTRUCT0":
        stack.push({ ty: "struct", lit: null });
        break;
      case "NEWSTRUCT":
        popU();
        stack.push({ ty: "struct", lit: null });
        break;
      case "NEWBUFFER":
        popU();
        stack.push({ ty: "buffer", lit: null });
        break;
      case "PACK": {
        const count = asCount(popU().lit);
        if (count !== null) for (let k = 0; k < Math.min(count, stack.length); k += 1) popU();
        stack.push({ ty: "array", lit: null });
        break;
      }
      case "PACKMAP": {
        const count = asCount(popU().lit);
        if (count !== null) for (let k = 0; k < Math.min(count * 2, stack.length); k += 1) popU();
        stack.push({ ty: "map", lit: null });
        break;
      }
      case "PACKSTRUCT": {
        const count = asCount(popU().lit);
        if (count !== null) for (let k = 0; k < Math.min(count, stack.length); k += 1) popU();
        stack.push({ ty: "struct", lit: null });
        break;
      }
      case "UNPACK":
        popU();
        stack.push(unk());
        break;
      case "PICKITEM":
        popU();
        popU();
        stack.push(unk());
        break;
      case "SETITEM":
        popU();
        popU();
        popU();
        break;
      case "APPEND":
      case "REMOVE":
        popU();
        popU();
        break;
      case "CLEARITEMS":
        popU();
        break;
      case "POPITEM":
        popU();
        popU();
        stack.push(unk());
        break;
      case "SIZE":
        popU();
        stack.push({ ty: "integer", lit: null });
        break;
      case "HASKEY":
        popU();
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "ISNULL":
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "ISTYPE":
        popU();
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "CONVERT":
        popU();
        stack.push({ ty: convertTargetType(ins.operand) ?? "any", lit: null });
        break;

      // Arithmetic + comparisons (subset)
      case "ADD":
      case "SUB":
      case "MUL":
      case "DIV":
      case "MOD":
      case "POW":
      case "MIN":
      case "MAX":
      case "SHL":
      case "SHR":
      case "AND":
      case "OR":
      case "XOR":
        popU();
        popU();
        stack.push({ ty: "integer", lit: null });
        break;
      case "MODMUL":
      case "MODPOW":
        popU();
        popU();
        popU();
        stack.push({ ty: "integer", lit: null });
        break;
      case "WITHIN":
        popU();
        popU();
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "SQRT":
      case "ABS":
      case "SIGN":
      case "INC":
      case "DEC":
      case "NEGATE":
      case "INVERT":
        popU();
        stack.push({ ty: "integer", lit: null });
        break;
      case "NOT":
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "BOOLAND":
      case "BOOLOR":
        popU();
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "EQUAL":
      case "NUMEQUAL":
      case "NOTEQUAL":
      case "NUMNOTEQUAL":
      case "GT":
      case "GE":
      case "LT":
      case "LE":
        popU();
        popU();
        stack.push({ ty: "bool", lit: null });
        break;
      case "NZ":
        // NZ is unary, but treating it as "pop 1" is enough for type recovery.
        popU();
        stack.push({ ty: "bool", lit: null });
        break;

      // Everything else is a no-op for typing purposes.
      default:
        break;
    }
  }
}

function reverseTop(stack, count) {
  if (count === 0 || stack.length < count) return;
  const start = stack.length - count;
  const slice = stack.slice(start).reverse();
  for (let i = 0; i < count; i += 1) stack[start + i] = slice[i];
}

// Decode an integer-literal operand to a Number, mirroring Rust
// int_literal_from_operand (I8/I16/I32/I64/U8/U16/U32). Larger literals
// (PUSHINT128/256) carry no usable count and return null.
function intLiteral(operand) {
  if (!operand) return null;
  switch (operand.kind) {
    case "I8":
    case "I16":
    case "I32":
    case "I64":
    case "U8":
    case "U16":
    case "U32": {
      const v = operand.value;
      return typeof v === "bigint" ? v : Number(v);
    }
    default:
      return null;
  }
}

// A literal usable as a non-negative count (Rust usize::try_from). Huge values
// are clamped against the real stack depth by the caller.
function asCount(lit) {
  if (lit === null || lit === undefined) return null;
  if (typeof lit === "bigint") {
    if (lit < 0n) return null;
    return lit > BigInt(Number.MAX_SAFE_INTEGER) ? Number.MAX_SAFE_INTEGER : Number(lit);
  }
  if (!Number.isInteger(lit) || lit < 0) return null;
  return lit;
}

function manifestType(kind) {
  const normalized = String(kind).toLowerCase();
  if (normalized === "any") return "any";
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

const CONVERT_TARGET_MAP = new Map([
  [0x00, "any"],
  [0x10, "pointer"],
  [0x20, "bool"],
  [0x21, "integer"],
  [0x28, "bytestring"],
  [0x30, "buffer"],
  [0x40, "array"],
  [0x41, "struct"],
  [0x48, "map"],
  [0x60, "interopinterface"],
]);

function convertTargetType(operand) {
  if (!operand || (operand.kind !== "U8" && operand.kind !== "I8")) {
    return null;
  }
  const byte = operand.kind === "U8" ? operand.value : operand.value & 0xff;
  return CONVERT_TARGET_MAP.get(byte) ?? null;
}
