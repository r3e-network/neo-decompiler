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
// readable and statically typed.
export function rewriteFrameworkCallArguments(line, types = null) {
  const pattern = /\b(Contract\.Call|RoleManagement\.GetDesignatedByRole|StdLib\.MemorySearch|Runtime\.Log)\s*\(/g;
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
        args[2] = renderFrameworkEnumArgument(args[2], "CallFlags");
      }
    } else if (name === "RoleManagement.GetDesignatedByRole" && args[0]) {
      args[0] = renderFrameworkEnumArgument(args[0], "Role");
    } else if (name === "StdLib.MemorySearch") {
      if (args[2] && !/^\s*\(\s*int\s*\)/.test(args[2])) {
        args[2] = `(int)(dynamic)(${args[2]})`;
      }
      if (args[1] && isNumericExpression(args[1], types)) {
        args[1] = `((dynamic)(${args[1]}))`;
      }
    } else if (name === "Runtime.Log") {
      for (let index = 0; index < args.length; index += 1) {
        if (/^\s*Helper\.Range\s*\(/.test(args[index])) {
          args[index] = `((dynamic)(${args[index]}))`;
        }
      }
    }
    output += line.slice(cursor, match.index);
    output += `${name}(${args.join(", ")})`;
    cursor = close + 1;
    pattern.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function renderFrameworkEnumArgument(expression, type) {
  const source = expression.trim();
  if (new RegExp(`^\\(\\s*${type}\\s*\\)`).test(source)) return expression;
  if (/^-?(?:0x[0-9a-f]+|[0-9]+)$/i.test(source)) return `(${type})(${source})`;
  return `(${type})(dynamic)(${expression})`;
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
