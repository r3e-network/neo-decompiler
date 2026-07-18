import {
  findCallClose,
  nextOutsideMatch,
  splitCallArguments,
} from "./csharp-expression-scanner.js";
import {
  describeMethodToken,
  frameworkNativeMethod,
} from "./native-contracts.js";

// A few framework APIs have stronger C# parameter types than the VM values
// they consume. Add explicit boundary conversions only where the recovered
// expression is known to be a VM ByteString/number, keeping ordinary calls
// readable and statically typed. Keep these casts aligned with Rust
// `expr_native::render_native_args`.
export function rewriteFrameworkCallArguments(line, types = null) {
  const pattern = /\b(Contract\.Call|RoleManagement\.GetDesignatedByRole|StdLib\.MemorySearch|PolicyContract\.GetAttributeFee|CryptoLib\.VerifyWithECDsa|Runtime\.Log|Runtime\.BurnGas|Runtime\.GetNotifications|Runtime\.LoadScript)\s*\(/g;
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
    if (name === "Contract.Call") {
      if (args[0] && isByteStringExpression(args[0], types)) {
        args[0] = `(UInt160)(dynamic)(${args[0]})`;
      }
      if (args[2] && !/^\s*\(\s*CallFlags\s*\)/.test(args[2])) {
        args[2] = renderFrameworkEnumArgument(args[2], "CallFlags", types);
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

// CALLT labels are intentionally kept as readable bare names in the
// high-level surface. When a token is available, render that call through the
// framework's Contract.Call API so the generated C# remains self-contained.
export function rewriteCSharpMethodTokenCalls(line, methodTokens = null) {
  if (!Array.isArray(methodTokens) || methodTokens.length === 0) return line;
  if (line.trimStart().startsWith("//")) return line;
  const tokens = new Map(
    methodTokens
      .filter((token) => token && typeof token.method === "string")
      .flatMap((token) => [
        [token.method, token],
        [token.method.toLowerCase(), token],
      ]),
  );
  if (tokens.size === 0) return line;
  const pattern = /\b([A-Za-z_][A-Za-z0-9_]*)\s*\(/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const token = tokens.get(match[1]) ?? tokens.get(match[1].toLowerCase());
    if (!token || isQualifiedCallName(line, match.index)) continue;
    if (isDirectNativeToken(token)) continue;
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (close < 0) continue;
    const args = splitCallArguments(line.slice(open + 1, close));
    output += line.slice(cursor, match.index);
    output += renderMethodTokenCall(token, args);
    cursor = close + 1;
    pattern.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function isDirectNativeToken(token) {
  if (token.callFlags !== 0x0F) return false;
  const hash = token.hash instanceof Uint8Array
    ? token.hash
    : Array.isArray(token.hash) ? Uint8Array.from(token.hash) : null;
  if (hash?.length !== 20) return false;
  const hint = describeMethodToken(hash, token.method);
  if (!hint?.hasExactMethod() || !hint.canonicalMethod) return false;
  // Only skip Contract.Call when the Neo C# framework exposes the method.
  // Catalog-only protocol methods (Oracle.Finish, Governance.*) still need
  // the hash-preserving fallback.
  return frameworkNativeMethod(hint.contract, hint.canonicalMethod) != null;
}

function isQualifiedCallName(line, index) {
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous -= 1;
  // High-level native labels use `Contract::Method`, while C# surface uses
  // `Contract.Method`. Both must skip bare token rewriting so the qualified
  // native rewriter can map framework APIs or Contract.Call fallbacks.
  return previous >= 0 && (line[previous] === "." || line[previous] === ":");
}

function renderMethodTokenCall(token, args) {
  const bytes = token.hash instanceof Uint8Array
    ? [...token.hash]
    : Array.isArray(token.hash) ? token.hash : [];
  const address = bytes.length === 20
    ? `(UInt160)new byte[] { ${bytes.map((byte) => `0x${Number(byte).toString(16).padStart(2, "0").toUpperCase()}`).join(", ")} }`
    : "default(UInt160)";
  const flags = Number.isInteger(token.callFlags) ? token.callFlags : 0;
  return `Contract.Call(${address}, "${escapeCSharpTokenString(token.method)}", (CallFlags)(${flags}), new object[] { ${args.join(", ")} })`;
}

function escapeCSharpTokenString(value) {
  return String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"');
}
