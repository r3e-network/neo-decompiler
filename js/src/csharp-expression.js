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
import { rewriteConstantExpressions } from "./csharp-expression-constants.js";
import {
  rewriteFrameworkCallArguments,
} from "./csharp-framework.js";
import {
  frameworkNativeMethod,
  nativeContractHash,
} from "./native-contracts.js";

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
    rewriteConstantExpressions(rewriteCollectionLiterals(
      rewriteEmptyArrayLiterals(
        rewriteConcatenation(
          rewriteFrameworkCallArguments(
            rewriteQualifiedCalls(
              rewriteKnownSyscalls(
                rewriteKnownHelpers(
                  rewriteOversizedDecimalLiterals(rewriteOversizedHexLiterals(line)),
                  types,
                ),
              ),
            ),
            types,
          ),
        ),
      ),
      (element) => rewriteCSharpExpression(element),
    )),
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
    const contract = match[1];
    const rawMethod = match[2];
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (open < 0 || close < 0) continue;

    const framework = frameworkNativeMethod(contract, rawMethod);
    if (framework) {
      const property = CSHARP_NATIVE_PROPERTIES.get(`${contract}::${framework}`);
      if (property && !line.slice(open + 1, close).trim()) {
        output += line.slice(cursor, match.index) + property;
        cursor = close + 1;
        pattern.lastIndex = cursor;
        continue;
      }
      output += line.slice(cursor, match.index);
      output += `${contract}.${framework}(`;
      cursor = pattern.lastIndex;
      continue;
    }

    // Catalogued natives without a framework binding must not claim a direct
    // API (Governance.GetCommittee, OracleContract.Finish, …). Fall back to
    // hash-preserving Contract.Call when the native script hash is known.
    const hash = nativeContractHash(contract);
    if (hash) {
      const args = line.slice(open + 1, close).trim();
      output += line.slice(cursor, match.index);
      output += renderUnsupportedNativeCall(hash, rawMethod, args);
      cursor = close + 1;
      pattern.lastIndex = cursor;
      continue;
    }

    output += line.slice(cursor, match.index);
    output += `${contract}.${rawMethod}(`;
    cursor = pattern.lastIndex;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}

function renderUnsupportedNativeCall(hash, method, args) {
  const bytes = [...hash]
    .map((byte) => `0x${Number(byte).toString(16).padStart(2, "0").toUpperCase()}`)
    .join(", ");
  const escaped = String(method)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"');
  return `(dynamic)Contract.Call((UInt160)new byte[] { ${bytes} }, "${escaped}", (CallFlags)(15), new object[] { ${args} })`;
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
  const marker = /\bsyscall\(\s*(?:"([^"]+)"|(0x[0-9a-fA-F]+))/g;
  let output = "";
  let cursor = 0;
  let match;
  while ((match = nextOutsideMatch(line, marker)) !== null) {
    const open = line.indexOf("(", match.index);
    const close = findCallClose(line, open);
    if (open < 0 || close < 0) continue;
    const argsText = line
      .slice(open + 1, close)
      .replace(/^\s*(?:"[^"]*"|0x[0-9a-fA-F]+)\s*(?:,\s*)?/, "")
      .trim();
    // Rewrite nested syscall expressions before rendering the enclosing call.
    // A syscall argument can itself be a syscall (for example storage calls
    // receiving `GetReadOnlyContext`), and a single left-to-right scan would
    // otherwise skip the nested marker inside the replaced outer call.
    const args = splitCallArguments(argsText).map((arg) => rewriteKnownSyscalls(arg));
    const replacement = renderCSharpSyscall(match[1] ?? match[2], args);
    if (!replacement) continue;
    output += line.slice(cursor, match.index) + replacement;
    cursor = close + 1;
    marker.lastIndex = cursor;
  }
  return cursor === 0 ? line : output + line.slice(cursor);
}
