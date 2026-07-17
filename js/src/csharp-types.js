import {
  csharpSyscallReturnType,
  csharpSyscallReturnsValue,
} from "./csharp-syscalls.js";
import { csharpType } from "./csharp-type-map.js";
import {
  nativeMethodReturnType,
  nativeMethodReturnsValue,
} from "./native-contracts.js";

function inferExpressionType(
  expression,
  knownTypes = new Map(),
  knownCallTypes = new Map(),
) {
  let value = expression.trim();
  while (value.startsWith("(") && value.endsWith(")")) {
    value = value.slice(1, -1).trim();
  }
  if (value === "null") return "null";
  if (/^(?:true|false)$/i.test(value)) return "bool";
  if (/^-?\d+$/.test(value)) return "BigInteger";
  if (/^\"(?:[^\"\\]|\\.)*\"$/.test(value)) return "string";
  if (/^new_array_t\s*\(/i.test(value)) {
    const type = value.match(/,\s*\"?([a-z]+)\"?\s*\)\s*$/i)?.[1]?.toLowerCase();
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
  const syscall = value.match(/^syscall\s*\(\s*\"([^\"]+)\"/i);
  if (syscall) {
    const syscallType = csharpSyscallReturnType(syscall[1]);
    if (syscallType) return syscallType;
    if (csharpSyscallReturnsValue(syscall[1]) === false) return "void";
    return "dynamic";
  }
  // Catalogued native helpers (`StdLib::Itoa`) carry a stable framework return
  // type once the contract identity is known. Known void natives stay `void`
  // so the body renderer can emit a statement instead of an illegal assignment.
  // Unknown natives stay dynamic.
  const native = value.match(
    /^([A-Za-z_][A-Za-z0-9_]*)(?:::|\.)([A-Za-z_][A-Za-z0-9_]*)\s*\(/,
  );
  if (native) {
    const nativeType = nativeMethodReturnType(native[1], native[2]);
    if (nativeType) return nativeType;
    if (nativeMethodReturnsValue(native[1], native[2]) === false) return "void";
    return "dynamic";
  }
  const call = value.match(/^@?([A-Za-z_][A-Za-z0-9_]*)\s*\(/);
  if (call) {
    const knownCallType = knownCallTypes.get(call[1]);
    if (knownCallType && knownCallType !== "void") return knownCallType;
    return "dynamic";
  }
  // Operators inside call arguments (for example `hanoiTower(n - 1, ...)`)
  // do not describe the call's result. Keep unknown method calls dynamic
  // instead of falsely inferring a numeric return from an argument expression.
  if (/^[A-Za-z_][A-Za-z0-9_.]*\s*\(/.test(value)) return "dynamic";
  const identifier = value.match(/^@?([A-Za-z_][A-Za-z0-9_]*)$/)?.[1];
  if (identifier) return knownTypes.get(identifier) ?? "dynamic";
  if (hasComparisonOperator(value)) return "bool";
  if (/[+\-*\/%]|<<|>>|\b(?:and|or|xor|shl|shr)\b/.test(value)) return "BigInteger";
  return "dynamic";
}

// Do not mistake the individual angle brackets in VM shift expressions for
// relational operators. BigInteger shift values are numeric, while a genuine
// comparison still has an isolated `<` or `>` token.
function hasComparisonOperator(value) {
  if (/(?:===|!==|==|!=|<=|>=|&&|\|\||!)/.test(value)) return true;
  return /(?:^|[^<])<(?:[^<]|$)/.test(value)
    || /(?:^|[^>])>(?:[^>]|$)/.test(value);
}

function isConcreteType(type) {
  return type !== "dynamic" && type !== "null";
}

function mapsEqual(left, right) {
  if (left.size !== right.size) return false;
  return [...left].every(([name, type]) => right.get(name) === type);
}

function resolveDefinitionType(definitions, knownTypes, knownCallTypes) {
  const observed = definitions.map((definition) => inferExpressionType(
    definition,
    knownTypes,
    knownCallTypes,
  ));
  if (observed.includes("dynamic")) return null;
  const concrete = observed.filter((type) => type !== "dynamic" && type !== "null");
  if (concrete.length === 0 || new Set(concrete).size !== 1) return null;
  const [type] = concrete;
  // A neutral VM null slot can safely join the generic object-array shape.
  // Keep typed arrays and value-like types dynamic at this ambiguous boundary.
  if (observed.includes("null") && type !== "object[]") return null;
  return isConcreteType(type) ? type : null;
}

/**
 * Infer stable C# declaration types within one high-level method.
 *
 * Definitions are solved to a small fixed point so aliases such as
 * `let alias = source;` inherit a concrete collection type without treating
 * unresolved expressions as a type claim.
 */
export function inferDeclarationTypes(lines, knownCallTypes = new Map()) {
  const definitions = new Map();
  const parameterTypes = inferParameterTypes(lines);
  for (const line of lines) {
    const trimmed = line.trim();
    const forMatch = trimmed.match(
      /^for\s*\(\s*let\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([^;]+);/,
    );
    const match = forMatch ?? trimmed.match(
      /^(?:let\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+);$/,
    );
    if (!match) continue;
    if (!definitions.has(match[1])) definitions.set(match[1], []);
    definitions.get(match[1]).push(match[2]);
  }

  let knownTypes = new Map(parameterTypes);
  for (let iteration = 0; iteration <= definitions.size; iteration += 1) {
    const nextTypes = new Map(parameterTypes);
    for (const [name, values] of definitions) {
      const type = resolveDefinitionType(values, knownTypes, knownCallTypes);
      if (type) nextTypes.set(name, type);
    }
    if (mapsEqual(nextTypes, knownTypes)) break;
    knownTypes = nextTypes;
  }

  return new Map([
    ...parameterTypes,
    ...[...definitions.keys()].map((name) => [name, knownTypes.get(name) ?? "dynamic"]),
  ]);
}

/**
 * Collect C#-mapped return types for methods available in the high-level
 * surface. `any` and unannotated methods stay dynamic/void so a call cannot
 * create a false concrete declaration type.
 */
export function inferKnownMethodReturnTypes(lines) {
  const returnTypes = new Map();
  for (const line of lines) {
    const match = line.match(
      /^\s*fn\s+([A-Za-z_][A-Za-z0-9_]*)\(.*\)(?:\s*->\s*([^\s{]+))?\s*\{\s*$/,
    );
    if (!match) continue;
    const raw = String(match[2] ?? "void").trim().toLowerCase();
    const type = raw === "any" ? "dynamic" : csharpType(raw);
    const previous = returnTypes.get(match[1]);
    if (previous && previous !== type) {
      returnTypes.set(match[1], "dynamic");
    } else if (!previous || previous === "dynamic") {
      returnTypes.set(match[1], type);
    }
  }
  return returnTypes;
}

function inferParameterTypes(lines) {
  const signature = lines.find((line) => /^\s*fn\s+/.test(line))
    ?.match(/^\s*fn\s+[A-Za-z_][A-Za-z0-9_]*\((.*?)\)/);
  if (!signature) return new Map();
  const parameters = signature[1].split(",").map((parameter) => parameter.trim());
  const types = new Map();
  for (const parameter of parameters) {
    const separator = parameter.indexOf(":");
    if (separator < 0) {
      const name = parameter.trim();
      if (name) types.set(name, "object");
      continue;
    }
    const name = parameter.slice(0, separator).trim();
    const type = parameter.slice(separator + 1).trim().toLowerCase();
    if (!name) continue;
    types.set(name, {
      void: "void",
      bool: "bool",
      boolean: "bool",
      int: "BigInteger",
      integer: "BigInteger",
      string: "string",
      bytes: "ByteString",
      bytestring: "ByteString",
      bytearray: "ByteString",
      array: "object[]",
      map: "Map<object, object>",
      any: "dynamic",
    }[type] ?? "dynamic");
  }
  return types;
}
