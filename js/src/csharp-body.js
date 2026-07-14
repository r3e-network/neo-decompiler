import { rewriteCSharpExpression, splitCallArguments } from "./csharp-expression.js";
import { csharpIdentifier } from "./csharp-identifiers.js";

function inferExpressionType(expression) {
  let value = expression.trim();
  while (value.startsWith("(") && value.endsWith(")")) {
    value = value.slice(1, -1).trim();
  }
  if (/^(?:true|false)$/i.test(value)) return "bool";
  if (/^-?\d+$/.test(value)) return "BigInteger";
  if (/^"(?:[^"\\]|\\.)*"$/.test(value)) return "string";
  if (/^new_array_t\s*\(/i.test(value)) {
    const type = value.match(/,\s*"?([a-z]+)"?\s*\)\s*$/i)?.[1]?.toLowerCase();
    return {
      bool: "bool[]",
      boolean: "bool[]",
      int: "BigInteger[]",
      integer: "BigInteger[]",
      buffer: "byte[]",
      bytes: "ByteString[]",
      bytestring: "ByteString[]",
    }[type] ?? "object[]";
  }
  if (/^(?:new_array|pack)\s*\(/i.test(value)) return "object[]";
  if (/^\[.*\]$/s.test(value)) return "object[]";
  if (/^Map\s*\(/i.test(value)) return "Map<object, object>";
  if (/^(?:is_null|is_type(?:_[A-Za-z0-9_]+)?|has_key|within|equals|not_equals|not|is_valid)\s*\(/i.test(value)) {
    return "bool";
  }
  if (/^(?:len|size|abs|sqrt|min|max|sign|convert_to_integer)\s*\(/i.test(value)) {
    return "BigInteger";
  }
  if (/^convert_to_bool\s*\(/i.test(value)) return "bool";
  if (/^convert_to_bytestring\s*\(/i.test(value)) return "ByteString";
  if (/[<>=!]=?|&&|\|\|/.test(value)) return "bool";
  if (/[+\-*\/%]|\b(?:and|or|xor|shl|shr)\b/.test(value)) return "BigInteger";
  return "dynamic";
}

export function inferDeclarationTypes(lines) {
  const observed = new Map();
  for (const line of lines) {
    const match = line.trim().match(/^(?:let\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+);$/);
    if (!match) continue;
    const type = inferExpressionType(match[2]);
    if (!observed.has(match[1])) observed.set(match[1], new Set());
    observed.get(match[1]).add(type);
  }
  return new Map(
    [...observed].map(([name, types]) => [
      name,
      types.size === 1 && !types.has("dynamic") ? [...types][0] : "dynamic",
    ]),
  );
}

export function renderBodyLine(line, declarationTypes = null) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  const declaration = trimmed.match(/^let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*.*)?;$/);
  if (declaration) {
    const declarationType = declarationTypes
      ? declarationTypes.get(declaration[1]) ?? "dynamic"
      : "var";
    return rewriteCSharpExpression(
      `${indentation}${declarationType} ${csharpIdentifier(declaration[1])}${declaration[2] ?? ""};`,
      declarationTypes,
    );
  }
  const assertExpression = trimmed.match(/^assert\((.*)\);$/);
  if (assertExpression) {
    const args = splitCallArguments(assertExpression[1]);
    const condition = renderCSharpAssertionCondition(args[0] ?? "null", declarationTypes);
    if (args.length > 1) {
      const message = rewriteCSharpExpression(args.slice(1).join(", ").trim(), declarationTypes);
      return `${indentation}if (!${condition}) throw new InvalidOperationException(Convert.ToString(${message}));`;
    }
    return `${indentation}global::Neo.SmartContract.Framework.ExecutionEngine.Assert(${condition});`;
  }
  const throwExpression = trimmed.match(/^throw\((.*)\);$/);
  if (throwExpression) {
    const payload = rewriteCSharpExpression(throwExpression[1], declarationTypes);
    return `${indentation}throw new Exception(Convert.ToString(${payload}));`;
  }
  const abortExpression = trimmed.match(/^abort\((.*)\);$/);
  if (abortExpression) {
    const payload = rewriteCSharpExpression(abortExpression[1].trim(), declarationTypes);
    return payload
      ? `${indentation}throw new InvalidOperationException(Convert.ToString(${payload}));`
      : `${indentation}throw new InvalidOperationException();`;
  }
  if (trimmed === "abort" || trimmed === "abort;") {
    return `${indentation}throw new InvalidOperationException();`;
  }
  return rewriteCSharpExpression(line, declarationTypes).replace(/\bunknown\b/g, "default");
}

function renderCSharpAssertionCondition(expression, declarationTypes) {
  const source = expression.trim();
  if (source === "null") return "false";
  if (source === "true" || source === "false") return source;
  if (/^-?\d+$/.test(source)) return `${source} != 0`;
  return `(bool)(object)(${rewriteCSharpExpression(source, declarationTypes)})`;
}
