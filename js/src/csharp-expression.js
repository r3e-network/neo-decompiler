import { renderCSharpSyscall } from "./csharp-syscalls.js";
import {
  csharpIdentifier,
  isCSharpContextualKeyword,
} from "./csharp-identifiers.js";
import {
  createCSharpCollectionHelpers,
  renderCSharpTypeTest,
} from "./csharp-collections.js";
import {
  findCallClose,
  findQuotedLiteralClose,
  nextOutsideMatch,
  splitCallArguments,
} from "./csharp-expression-scanner.js";
import {
  rewriteCollectionLiterals,
  rewriteEmptyArrayLiterals,
  rewriteFunctionPointers,
  rewriteOversizedDecimalLiterals,
  rewriteOversizedHexLiterals,
  rewriteUnknownPlaceholders,
} from "./csharp-expression-literals.js";
import {
  rewriteConcatenation,
  rewriteDynamicCompoundAssignments,
  rewriteDynamicOperators,
  rewriteIndexOperands,
  rewriteNumericUnaryNot,
  rewriteShiftCounts,
} from "./csharp-expression-operators.js";

export { splitCallArguments } from "./csharp-expression-scanner.js";

const CSHARP_NATIVE_PROPERTIES = new Map([
  ["GasToken::Symbol", "GasToken.Symbol"],
  ["GasToken::Decimals", "GasToken.Decimals"],
  ["NeoToken::Symbol", "NeoToken.Symbol"],
  ["NeoToken::Decimals", "NeoToken.Decimals"],
  ["LedgerContract::CurrentHash", "LedgerContract.CurrentHash"],
  ["LedgerContract::CurrentIndex", "LedgerContract.CurrentIndex"],
]);

const CSHARP_COLLECTION_HELPERS = createCSharpCollectionHelpers(
  (expression, types) => rewriteCSharpExpression(expression, types),
);

export function rewriteCSharpExpression(line, types = null) {
  const lowered = rewriteUnknownPlaceholders(
    rewriteCollectionLiterals(
      rewriteEmptyArrayLiterals(
        rewriteConcatenation(
          rewriteFrameworkCallArguments(
            rewriteQualifiedCalls(rewriteKnownSyscalls(rewriteKnownHelpers(
            rewriteOversizedDecimalLiterals(rewriteOversizedHexLiterals(line)),
            types,
            ))),
            types,
          ),
        ),
      ),
      (element) => rewriteCSharpExpression(element),
    ),
  );
  return rewriteCSharpIdentifiers(
    rewriteNumericUnaryNot(
      rewriteFunctionPointers(
        rewriteDynamicOperators(
          rewriteDynamicCompoundAssignments(rewriteIndexOperands(rewriteShiftCounts(lowered), types)),
          types,
        ),
      ),
    ),
  );
}

// A few framework APIs have stronger C# parameter types than the VM values
// they consume. Add explicit boundary conversions only where the recovered
// expression is known to be a VM ByteString/number, keeping ordinary calls
// readable and statically typed.
function rewriteFrameworkCallArguments(line, types = null) {
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
  const tokens = new Map(
    methodTokens
      .filter((token) => token && typeof token.method === "string")
      .map((token) => [token.method, token]),
  );
  if (tokens.size === 0) return line;
  const pattern = /\b([A-Za-z_][A-Za-z0-9_]*)\s*\(/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const token = tokens.get(match[1]);
    if (!token || isQualifiedCallName(line, match.index)) continue;
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

function isQualifiedCallName(line, index) {
  let previous = index - 1;
  while (previous >= 0 && /\s/.test(line[previous])) previous -= 1;
  return previous >= 0 && line[previous] === ".";
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

function rewriteCSharpIdentifiers(line) {
  let output = "";
  for (let index = 0; index < line.length;) {
    const character = line[index];
    if (character === '"' || character === "'") {
      const close = findQuotedLiteralClose(line, index);
      output += line.slice(index, close < 0 ? line.length : close + 1);
      if (close < 0) break;
      index = close + 1;
      continue;
    }
    if (line.startsWith("//", index)) {
      output += line.slice(index);
      break;
    }
    if (line.startsWith("/*", index)) {
      const close = line.indexOf("*/", index + 2);
      output += line.slice(index, close < 0 ? line.length : close + 2);
      if (close < 0) break;
      index = close + 2;
      continue;
    }
    if (!/[A-Za-z_]/.test(character)) {
      output += character;
      index += 1;
      continue;
    }
    let end = index + 1;
    while (end < line.length && /[A-Za-z0-9_]/.test(line[end])) end += 1;
    const name = line.slice(index, end);
    const escaped = shouldEscapeCSharpIdentifier(line, index, name)
      ? csharpIdentifier(name)
      : name;
    output += escaped;
    index = end;
  }
  return output;
}

function shouldEscapeCSharpIdentifier(line, index, name) {
  if (index > 0 && line[index - 1] === "@") return false;
  if (name === "throw") {
    const tail = line.slice(index + name.length).trimStart();
    return !tail.startsWith("new");
  }
  if (!isCSharpContextualKeyword(name)) return false;
  if (name === "dynamic") return false;
  if (name === "global" && line.slice(index + name.length).startsWith("::")) return false;
  if (name === "let" && isForHeaderLet(line, index)) return false;
  return true;
}

function isForHeaderLet(line, index) {
  const prefix = line.slice(0, index);
  return /\bfor\s*\(\s*$/.test(prefix);
}

function rewriteQualifiedCalls(line) {
  const pattern = /\b([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)\s*\(/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, pattern)) !== null) {
    const property = CSHARP_NATIVE_PROPERTIES.get(`${match[1]}::${match[2]}`);
    if (property) {
      const open = line.indexOf("(", match.index);
      const close = findCallClose(line, open);
      if (close >= 0 && !line.slice(open + 1, close).trim()) {
        output += line.slice(cursor, match.index) + property;
        cursor = close + 1;
        pattern.lastIndex = cursor;
        continue;
      }
    }
    output += line.slice(cursor, match.index);
    output += `${match[1]}.${match[2]}(`;
    cursor = pattern.lastIndex;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function rewriteKnownHelpers(line, types) {
  let output = line;
  for (let pass = 0; pass < 32; pass += 1) {
    const match = nextOutsideMatch(
      output,
      /\b(new_array_t|new_array|new_buffer|new_struct|is_null|clear_items|remove_item|append|has_key|convert_to_integer|convert_to_bool|convert_to_bytestring|convert_to_buffer|convert|len|size|memcpy|unpack|unpack_item|keys|values|pack|pack_dynamic|pick|roll|Map|Struct|abs|sign|min|max|sqrt|modmul|modpow|pow|within|substr|left|right|pop_item|reverse_items|is_type_[A-Za-z0-9_]+)\s*\(/g,
    );
    if (!match) break;
    const open = output.indexOf("(", match.index);
    const close = findCallClose(output, open);
    if (close < 0) break;
    const args = splitCallArguments(output.slice(open + 1, close));
    const renderer = CSHARP_COLLECTION_HELPERS.get(match[1]);
    const replacement = match[1].startsWith("is_type_")
      ? renderCSharpTypeTest(match[1], args)
      : renderer?.(args, types) ?? renderUnresolvedStackHelper(match[1], args);
    if (!replacement) break;
    output = `${output.slice(0, match.index)}${replacement}${output.slice(close + 1)}`;
  }
  return output;
}

// A runtime-variable PICK/ROLL cannot be represented as an ordinary C#
// expression once the VM stack position has been lost. Keep the generated
// contract valid and make the loss explicit instead of leaking pseudo-code
// helpers that do not exist in the Neo framework.
function renderUnresolvedStackHelper(name, args) {
  if (name !== "pick" && name !== "roll") return null;
  return `default(dynamic) /* unresolved VM ${name.toUpperCase()}(${args.join(", ")}) */`;
}

function rewriteKnownSyscalls(line) {
  const marker = /syscall\("([^"]+)"/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, marker)) !== null) {
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (open < 0 || close < 0) continue;
    const argsText = line
      .slice(open + 1, close)
      .replace(/^\s*"[^"]*"\s*(?:,\s*)?/, "")
      .trim();
    // Rewrite nested syscall expressions before rendering the enclosing call.
    // A syscall argument can itself be a syscall (for example storage calls
    // receiving `GetReadOnlyContext`), and a single left-to-right scan would
    // otherwise skip the nested marker inside the replaced outer call.
    const args = splitCallArguments(argsText).map((arg) => rewriteKnownSyscalls(arg));
    const replacement = renderCSharpSyscall(match[1], args);
    if (!replacement) continue;
    output += line.slice(cursor, match.index) + replacement;
    cursor = close + 1;
    marker.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}
