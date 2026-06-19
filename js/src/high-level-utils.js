import { formatOperand } from "./disassembler.js";

export function jumpTarget(instruction) {
  const operand = instruction.operand;
  if (operand === null) {
    return null;
  }
  if (operand.kind === "Jump" || operand.kind === "Jump32") {
    return instruction.offset + operand.value;
  }
  return null;
}

export function resolvePackedValue(state, expression) {
  return state.packedValuesByExpression.get(expression) ?? state.packedValuesBySlot.get(expression) ?? null;
}

const CONVERT_TARGETS = new Map([
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

export function convertTargetName(operand) {
  if (!operand || (operand.kind !== "U8" && operand.kind !== "I8")) {
    return null;
  }
  const byte = operand.kind === "U8" ? operand.value : operand.value & 0xff;
  return CONVERT_TARGETS.get(byte) ?? null;
}

export function renderUntranslatedInstruction(instruction) {
  // Use the `// warning:` prefix (rather than the per-instruction
  // `// XXXX:` trace style) to mark this as a real hole in the
  // lifted source — distinct from the optional trace-comment
  // stream. Mirrors Rust's `warn(...)` which routes through the
  // same prefix so both ports surface untranslated opcodes
  // identically.
  const mnemonic =
    instruction.opcode.mnemonic === "UNKNOWN"
      ? `UNKNOWN_0x${instruction.opcode.byte.toString(16).padStart(2, "0").toUpperCase()}`
      : instruction.opcode.mnemonic;
  const operandText = instruction.operand !== null ? ` ${formatOperand(instruction.operand)}` : "";
  return `// warning: ${mnemonic}${operandText} (not yet translated)`;
}

const PRIMITIVE_OR_IDENT_RE = /^(?:-?\d+|0x[0-9A-Fa-f]+|true|false|null|[A-Za-z_][A-Za-z0-9_]*)$/u;
const ARRAY_LITERAL_RE = /^\[.*\]$/u;
const OBJECT_LITERAL_RE = /^\{.*\}$/u;
const CALL_PREFIX_RE = /^[A-Za-z_$][A-Za-z0-9_$]*\(/u;

/**
 * Returns true when `value` is a syntactically self-contained string
 * literal — opens and closes with matching `"` or `'`, with the closing
 * quote being the very last character (so `"a" + "b"` is *not* a single
 * literal even though both ends are quotes). Used by `wrapExpression`
 * to skip wrapping atomic string operands in extra parens.
 */
function isSelfContainedString(value) {
  if (value.length < 2) return false;
  const quote = value[0];
  if (quote !== '"' && quote !== "'") return false;
  if (value[value.length - 1] !== quote) return false;
  // Walk forward and confirm the trailing quote is the *first* unescaped
  // closing quote — anything else means the value contains additional
  // tokens past a closed string.
  let i = 1;
  while (i < value.length) {
    const ch = value[i];
    if (ch === "\\") {
      i += 2;
      continue;
    }
    if (ch === quote) {
      return i === value.length - 1;
    }
    i += 1;
  }
  return false;
}

/**
 * Returns true when `value` is shaped like a single function call —
 * `identifier(...)` whose closing paren is the final character and whose
 * paren depth never re-opens to zero before the end. Used by
 * `wrapExpression` to avoid redundant `(call())` parens around what is
 * already a syntactically self-contained operand.
 */
function isSelfContainedCall(value) {
  if (!value.endsWith(")") || !CALL_PREFIX_RE.test(value)) {
    return false;
  }
  const firstOpen = value.indexOf("(");
  let depth = 0;
  for (let i = firstOpen; i < value.length; i++) {
    const ch = value[i];
    if (ch === "(") {
      depth++;
    } else if (ch === ")") {
      depth--;
      if (depth === 0) {
        return i === value.length - 1;
      }
    }
  }
  return false;
}

export function wrapExpression(value) {
  if (
    PRIMITIVE_OR_IDENT_RE.test(value) ||
    ARRAY_LITERAL_RE.test(value) ||
    OBJECT_LITERAL_RE.test(value) ||
    isSelfContainedCall(value) ||
    isSelfContainedString(value)
  ) {
    return value;
  }
  if (value.startsWith("(") && value.endsWith(")")) {
    return value;
  }
  return `(${value})`;
}

export function stripOuterParens(value) {
  if (value.startsWith("(") && value.endsWith(")")) {
    // Verify the opening paren actually matches the closing one
    // by tracking brace depth — "(a) + (b)" must NOT be stripped.
    let depth = 0;
    for (let i = 0; i < value.length - 1; i++) {
      if (value[i] === "(") depth++;
      if (value[i] === ")") depth--;
      if (depth === 0) return value; // closed before the final char
    }
    return value.slice(1, -1);
  }
  return value;
}

// Resolve a stack value to a non-negative integer ONLY when it is a pure
// integer literal. ROLL/PICK/XDROP/REVERSEN/PACK take a count or index off the
// operand stack, which holds expression *strings*. `Number.parseInt("1 + 1",
// 10)` partial-parses to `1`, which would fold an arithmetically-computed
// count/index into a confidently-wrong static value (e.g. `pack` rendering one
// element instead of the dynamic form). Anything that is not a bare integer
// literal must fall to the honest dynamic path, mirroring the Rust port's
// `take_usize_literal` (which only reads the literal-values map).
export function literalIndex(text) {
  if (typeof text !== "string" || !/^-?\d+$/.test(text.trim())) {
    return Number.NaN;
  }
  return Number.parseInt(text, 10);
}

// Mirror Rust's `is_pure_rhs` (high_level/emitter/postprocess/simplify.rs):
// an expression has NO observable effect when its result is discarded iff it is
// a literal/identifier/arithmetic value or calls only known-pure NEO helpers. A
// division/modulo/indexing (which can fault) or a call to a side-effecting
// target (a syscall, an internal `sub_*`/named call, a CALLT/native method) is
// impure and must remain visible even when dropped.
const PURE_HELPER_IDENTIFIERS = new Set([
  "abs", "sign", "sqrt", "min", "max", "pow", "modpow", "modmul", "within",
  "left", "right", "substr", "is_null", "keys", "values", "has_key", "len",
]);

function isPureHelperIdentifier(ident) {
  return (
    PURE_HELPER_IDENTIFIERS.has(ident) ||
    ident.startsWith("is_type_") ||
    ident.startsWith("convert_to_")
  );
}

function rhsCallsOnlyPureHelpers(expr) {
  let inString = null;
  let i = 0;
  while (i < expr.length) {
    const ch = expr[i];
    if (inString !== null) {
      if (ch === "\\" && i + 1 < expr.length) {
        i += 2;
        continue;
      }
      if (ch === inString) {
        inString = null;
      }
      i += 1;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inString = ch;
      i += 1;
      continue;
    }
    if (ch === "(") {
      let start = i;
      while (start > 0 && /[A-Za-z0-9_]/.test(expr[start - 1])) {
        start -= 1;
      }
      if (start === i) {
        i += 1;
        continue;
      }
      if (!isPureHelperIdentifier(expr.slice(start, i))) {
        return false;
      }
    }
    i += 1;
  }
  return true;
}

export function isPureRhs(rhs) {
  const trimmed = rhs.trim();
  if (trimmed === "") {
    return false;
  }
  // Division, modulo, and indexing can fault, so treat them as impure.
  if (/[\/%[]/.test(trimmed)) {
    return false;
  }
  // No call site → pure (literals, identifiers, arithmetic).
  if (!trimmed.includes("(")) {
    return true;
  }
  return rhsCallsOnlyPureHelpers(trimmed);
}

export function formatOffset(offset) {
  return offset.toString(16).padStart(4, "0").toUpperCase();
}
