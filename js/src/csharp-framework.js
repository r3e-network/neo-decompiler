import {
  findCallClose,
  nextOutsideMatch,
  splitCallArguments,
} from "./csharp-expression-scanner.js";
import {
  METHOD_TOKEN_FALLBACK_CALL_PATTERN,
  methodTokenFallbackLabel,
} from "./method-token-label.js";
import { escapeCSharpStringContent } from "./visible-text.js";

// A few framework APIs have stronger C# parameter types than the VM values
// they consume. Add explicit boundary conversions only where the recovered
// expression is known to be a VM ByteString/number, keeping ordinary calls
// readable and statically typed. Keep these casts aligned with Rust
// `expr_native::render_native_args`.
export function rewriteFrameworkCallArguments(line, types = null) {
  const pattern = /\b(Contract\.Call|Contract\.CreateStandardAccount|Contract\.CreateMultisigAccount|ContractManagement\.HasMethod|RoleManagement\.GetDesignatedByRole|StdLib\.MemorySearch|PolicyContract\.GetAttributeFee|CryptoLib\.VerifyWithECDsa|CryptoLib\.Murmur32|Crypto\.CheckSig|Crypto\.CheckMultisig|Runtime\.CheckWitness|Runtime\.Log|Runtime\.BurnGas|Runtime\.GetNotifications|Runtime\.LoadScript)\s*\(/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (close < 0) continue;
    const args = splitCallArguments(line.slice(open + 1, close))
      .map((arg) => rewriteFrameworkCallArguments(arg, types));
    const name = match[1];
    if (name === "Runtime.CheckWitness") {
      // Mirror Rust SyscallArgument::Witness: only emit the framework
      // overload when the argument is proven UInt160/ECPoint. Otherwise
      // fail closed through the LoadScript syscall replay.
      if (args.length !== 1 || !isWitnessExpression(args[0], types)) {
        output += line.slice(cursor, match.index);
        output += renderCheckWitnessFallback(args[0] ?? "null");
        cursor = close + 1;
        pattern.lastIndex = cursor;
        continue;
      }
    } else if (name === "Contract.Call") {
      if (args[0] && isByteStringExpression(args[0], types)) {
        args[0] = `(UInt160)(dynamic)(${args[0]})`;
      }
      if (args[2] && !/^\s*\(\s*CallFlags\s*\)/.test(args[2])) {
        args[2] = renderFrameworkEnumArgument(args[2], "CallFlags", types);
      }
    } else if (name === "Contract.CreateStandardAccount" && args[0]) {
      // Mirror Rust SyscallArgument::Cast("ECPoint").
      args[0] = renderFrameworkCast(args[0], "ECPoint", types);
    } else if (name === "Contract.CreateMultisigAccount") {
      if (args[0] && !isExactIntExpression(args[0], types)) {
        args[0] = renderIntCast(args[0]);
      }
      if (args[1]) args[1] = renderFrameworkCast(args[1], "ECPoint[]", types);
    } else if (name === "ContractManagement.HasMethod") {
      // Framework: HasMethod(UInt160 hash, string method, int pcount).
      if (args[0]) args[0] = renderFrameworkCast(args[0], "UInt160", types);
      if (args[1]) args[1] = renderFrameworkCast(args[1], "string", types);
      if (args[2] && !isExactIntExpression(args[2], types)) {
        args[2] = renderIntCast(args[2]);
      }
    } else if (name === "RoleManagement.GetDesignatedByRole" && args[0]) {
      args[0] = renderFrameworkIntEnumArgument(args[0], "Role", types);
    } else if (name === "StdLib.MemorySearch") {
      // Integer search values are VM numbers; the framework overload expects
      // ByteString. Cast the pattern, then int-cast the start index.
      if (args[1] && isNumericExpression(args[1], types)) {
        args[1] = `(ByteString)(${args[1]})`;
      }
      if (args[2] && !isExactIntExpression(args[2], types)) {
        args[2] = renderIntCast(args[2]);
      }
    } else if (name === "PolicyContract.GetAttributeFee" && args[0]) {
      args[0] = renderFrameworkIntEnumArgument(
        args[0],
        "TransactionAttributeType",
        types,
      );
    } else if (name === "CryptoLib.VerifyWithECDsa" && args[3]) {
      args[3] = renderFrameworkIntEnumArgument(args[3], "NamedCurveHash", types);
    } else if (name === "CryptoLib.Murmur32" && args[1]) {
      // Framework seed parameter is uint.
      args[1] = renderUintCast(args[1], types);
    } else if (name === "Crypto.CheckSig") {
      if (args[0]) args[0] = renderFrameworkCast(args[0], "ECPoint", types);
      if (args[1]) args[1] = renderFrameworkCast(args[1], "ByteString", types);
    } else if (name === "Crypto.CheckMultisig") {
      if (args[0]) args[0] = renderFrameworkCast(args[0], "ECPoint[]", types);
      if (args[1]) args[1] = renderFrameworkCast(args[1], "ByteString[]", types);
    } else if (name === "Runtime.Log" && args[0]) {
      // Mirror Rust SyscallArgument::Cast("string"). Helper.Range values stay
      // dynamically bound because they are not ordinary string expressions.
      if (/^\s*Helper\.Range\s*\(/.test(args[0])) {
        args[0] = `((dynamic)(${args[0]}))`;
      } else {
        args[0] = renderFrameworkCast(args[0], "string", types);
      }
    } else if (name === "Runtime.BurnGas" && args[0]) {
      // Framework takes long; VM gas values are BigInteger-shaped.
      args[0] = renderLongIntegerCast(args[0], types);
    } else if (name === "Runtime.GetNotifications" && args[0]) {
      args[0] = renderFrameworkCast(args[0], "UInt160", types);
    } else if (name === "Runtime.LoadScript") {
      if (args[0]) args[0] = renderFrameworkCast(args[0], "ByteString", types);
      if (args[1] && !/^\s*\(\s*CallFlags\s*\)/.test(args[1])) {
        args[1] = renderFrameworkEnumArgument(args[1], "CallFlags", types);
      }
      if (args[2]) args[2] = renderFrameworkCast(args[2], "object[]", types);
    }
    output += line.slice(cursor, match.index);
    output += `${name}(${args.join(", ")})`;
    cursor = close + 1;
    pattern.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function renderFrameworkCast(expression, type, types = null) {
  const source = expression.trim();
  if (new RegExp(`^\\(\\s*${escapeRegExp(type)}\\s*\\)`).test(source)) {
    return expression;
  }
  if (hasExactFrameworkType(source, type, types)) return expression;
  // Low-level LoadScript fallbacks already emit `new object[] { ... }`.
  if (type === "object[]" && /^new\s+object\s*\[/.test(source)) {
    return expression;
  }
  if (type === "ByteString" && (/^\(\s*ByteString\s*\)/.test(source) || /^new\s+byte\s*\[/.test(source))) {
    return expression;
  }
  return `(${type})(${expression})`;
}

function renderLongIntegerCast(expression, types = null) {
  const source = expression.trim();
  if (/^\(\s*long\s*\)/.test(source)) return expression;
  if (hasExactFrameworkType(source, "long", types)) return expression;
  // Avoid double-wrapping an existing BigInteger cast.
  if (/^\(\s*BigInteger\s*\)/.test(source)) return `(long)${source}`;
  return `(long)(BigInteger)(${expression})`;
}

function renderUintCast(expression, types = null) {
  const source = expression.trim();
  if (/^\(\s*uint\s*\)/.test(source)) return expression;
  if (hasExactFrameworkType(source, "uint", types)) return expression;
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) return `(uint)(${source})`;
  return `(uint)(${expression})`;
}

// System.Runtime.CheckWitness syscall hash 0x8CEC27F8, little-endian after
// the SYSCALL opcode (0x41), matching Rust's low-level replay encoding.
const CHECK_WITNESS_LOADSCRIPT =
  "Runtime.LoadScript((ByteString)new byte[] { 0x41, 0xF8, 0x27, 0xEC, 0x8C }, CallFlags.All, new object[] { ";

function renderCheckWitnessFallback(argument) {
  return `(bool)${CHECK_WITNESS_LOADSCRIPT}${argument} })`;
}

function isWitnessExpression(expression, types = null) {
  const source = expression.trim();
  if (/^\(\s*(UInt160|ECPoint)\s*\)/.test(source)) return true;
  return hasExactFrameworkType(source, "UInt160", types)
    || hasExactFrameworkType(source, "ECPoint", types);
}

function escapeRegExp(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function renderFrameworkEnumArgument(expression, type, types = null) {
  const source = expression.trim();
  if (new RegExp(`^\\(\\s*${type}\\s*\\)`).test(source)) return expression;
  if (hasExactFrameworkType(source, type, types)) return expression;
  // Already a framework enum member such as CallFlags.All.
  if (new RegExp(`^${escapeRegExp(type)}\\.`).test(source)) return expression;
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) return `(${type})(${source})`;
  return `(${type})(dynamic)(${expression})`;
}

// Integer-backed framework enums (Role, NamedCurveHash, …) follow the Rust
// renderer: `(Type)(int)(expr)` unless the value is already that enum type.
function renderFrameworkIntEnumArgument(expression, type, types = null) {
  const source = expression.trim();
  if (new RegExp(`^\\(\\s*${type}\\s*\\)`).test(source)) return expression;
  if (hasExactFrameworkType(source, type, types)) return expression;
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) {
    return `(${type})(int)(${source})`;
  }
  return `(${type})(int)(${expression})`;
}

function renderIntCast(expression) {
  const source = expression.trim();
  if (/^\(\s*int\s*\)/.test(source)) return expression;
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) return `(int)(${source})`;
  return `(int)(${expression})`;
}

function hasExactFrameworkType(expression, type, types) {
  const identifier = expression.trim().match(/^@?([A-Za-z_][A-Za-z0-9_]*)$/)?.[1];
  return identifier ? types?.get(identifier) === type : false;
}

function isExactIntExpression(expression, types) {
  const source = expression.trim();
  if (/^\(\s*int\s*\)/.test(source)) return true;
  const identifier = source.match(/^@?([A-Za-z_][A-Za-z0-9_]*)$/)?.[1];
  return identifier ? types?.get(identifier) === "int" : false;
}

function isByteStringExpression(expression, types) {
  const source = expression.trim();
  if (/^\(?\s*(?:ByteString|byte\[\])\b/.test(source) || /^\(?\s*new\s+byte\s*\[/.test(source)) {
    return true;
  }
  const identifier = source.match(/^@?([A-Za-z_][A-Za-z0-9_]*)$/)?.[1];
  return identifier ? types?.get(identifier) === "ByteString" : false;
}

function isNumericExpression(expression, types) {
  const source = expression.trim();
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) return true;
  const identifier = source.match(/^@?([A-Za-z_][A-Za-z0-9_]*)$/)?.[1];
  return identifier ? types?.get(identifier) === "BigInteger" : false;
}

// Non-native/restricted CALLT labels are inert identifiers derived from the
// token-table index. Resolve only that identifier here: matching by the raw
// method name is both ambiguous (different hashes may expose the same name)
// and unsafe because the method text originates in an untrusted NEF.
export function rewriteCSharpMethodTokenCalls(line, methodTokens = null) {
  if (!Array.isArray(methodTokens) || methodTokens.length === 0) return line;
  const pattern = new RegExp(METHOD_TOKEN_FALLBACK_CALL_PATTERN.source, "g");
  const edits = [];
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const index = Number(match[2]);
    const token = methodTokens[index];
    if (!token || typeof token.method !== "string"
        || !Number.isSafeInteger(index)
        || methodTokenFallbackLabel(index) !== match[1]
        || isQualifiedCallName(line, match.index)) continue;
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (close < 0) continue;
    // Rewrite only the call's delimiters. Keeping the argument text intact
    // preserves comments and evaluation order, and lets the same scan visit
    // nested CALLTs without recursive rewrites or rebuilding the token table.
    edits.push({ start: match.index, end: open + 1, text: renderMethodTokenCallPrefix(token) });
    edits.push({ start: close, end: close + 1, text: " })" });
  }
  if (edits.length === 0) return line;
  edits.sort((left, right) => left.start - right.start);
  let output = "";
  let cursor = 0;
  for (const edit of edits) {
    output += line.slice(cursor, edit.start) + edit.text;
    cursor = edit.end;
  }
  return output + line.slice(cursor);
}

function isQualifiedCallName(line, index) {
  if (index > 0 && /[@\p{L}\p{N}\p{M}\p{Pc}\p{Cf}]/u.test(line[index - 1])) return true;
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous -= 1;
  // High-level native labels use `Contract::Method`, while C# surface uses
  // `Contract.Method`. Both must skip bare token rewriting so the qualified
  // native rewriter can map framework APIs or Contract.Call fallbacks.
  return previous >= 0 && (line[previous] === "." || line[previous] === ":");
}

function renderMethodTokenCallPrefix(token) {
  const bytes = token.hash instanceof Uint8Array
    ? [...token.hash]
    : Array.isArray(token.hash) ? token.hash : [];
  const address = bytes.length === 20
    ? `(UInt160)new byte[] { ${bytes.map((byte) => `0x${Number(byte).toString(16).padStart(2, "0").toUpperCase()}`).join(", ")} }`
    : "default(UInt160)";
  const flags = Number.isInteger(token.callFlags) ? token.callFlags : 0;
  return `Contract.Call(${address}, "${escapeCSharpStringContent(token.method)}", (CallFlags)(${flags}), new object[] { `;
}
