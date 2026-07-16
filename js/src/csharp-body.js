import { rewriteCSharpExpression, splitCallArguments } from "./csharp-expression.js";
import { csharpIdentifier } from "./csharp-identifiers.js";
export { inferDeclarationTypes } from "./csharp-types.js";
import { inferDeclarationTypes } from "./csharp-types.js";

export function renderBodyLine(line, declarationTypes = null) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  const finish = (rendered) => rewriteCSharpControlSyntax(rendered, declarationTypes);
  if (trimmed.startsWith("//")) return line;
  const declaration = trimmed.match(
    /^let\s+([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*.*?;)(\s*\/\/.*)?$/,
  );
  if (declaration) {
    const declarationType = declarationTypes
      ? declarationTypes.get(declaration[1]) ?? "dynamic"
      : "var";
    return finish(rewriteCSharpExpression(
      `${indentation}${declarationType} ${csharpIdentifier(declaration[1])}${declaration[2]}`,
      declarationTypes,
    )) + (declaration[3] ?? "");
  }
  const assertExpression = trimmed.match(/^assert\((.*)\);$/);
  if (assertExpression) {
    const args = splitCallArguments(assertExpression[1]);
    const condition = renderCSharpAssertionCondition(args[0] ?? "null", declarationTypes);
    if (args.length > 1) {
      const message = rewriteCSharpExpression(args.slice(1).join(", ").trim(), declarationTypes);
      return finish(`${indentation}if (!${condition}) throw new InvalidOperationException(Convert.ToString(${message}));`);
    }
    return finish(`${indentation}global::Neo.SmartContract.Framework.ExecutionEngine.Assert(${condition});`);
  }
  if (trimmed === "throw();") {
    return finish(`${indentation}throw new Exception();`);
  }
  const throwExpression = trimmed.match(/^throw\((.*)\);$/);
  if (throwExpression) {
    const payload = rewriteCSharpExpression(throwExpression[1], declarationTypes);
    return finish(`${indentation}throw new Exception(Convert.ToString(${payload}));`);
  }
  const abortExpression = trimmed.match(/^abort\((.*)\);$/);
  if (abortExpression) {
    const payload = rewriteCSharpExpression(abortExpression[1].trim(), declarationTypes);
    return finish(payload
      ? `${indentation}throw new InvalidOperationException(Convert.ToString(${payload}));`
      : `${indentation}throw new InvalidOperationException();`);
  }
  if (trimmed === "abort" || trimmed === "abort;") {
    return finish(`${indentation}throw new InvalidOperationException();`);
  }
  return renderDiscardedExpression(
    finish(rewriteCSharpExpression(line, declarationTypes).replace(/\bunknown\b/g, "default")),
  );
}

// Manifest signatures describe the VM value crossing the public method
// boundary. Keep that contract explicit in generated C# when the body still
// contains a conservative object/ByteString/array expression.
export function coerceCSharpReturn(line, expectedType, declarationTypes = null) {
  if (!expectedType || expectedType === "void" || expectedType === "object" || expectedType === "dynamic") {
    return line;
  }
  const match = line.match(/^(\s*)return\s+(.+);$/);
  if (!match) return line;
  const indentation = match[1];
  const expression = match[2].trim();
  const actualType = renderedExpressionType(expression, declarationTypes);
  if (actualType === expectedType) return line;

  if (expectedType === "object[]" && /^new\s+[A-Za-z_][A-Za-z0-9_.]*\[\]\s*\{/.test(expression)) {
    return `${indentation}return ${expression.replace(/^new\s+[A-Za-z_][A-Za-z0-9_.]*\[\]/, "new object[]")};`;
  }
  if (/^[A-Za-z_][A-Za-z0-9_.<>\[\]]*$/.test(expectedType)) {
    return `${indentation}return (${expectedType})(dynamic)(${expression});`;
  }
  return line;
}

function renderedExpressionType(expression, declarationTypes) {
  if (/^new\s+object\[\]/.test(expression)) return "object[]";
  const array = expression.match(/^new\s+([A-Za-z_][A-Za-z0-9_.]*)\[\]/)?.[1];
  if (array) return `${array}[]`;
  if (/^"(?:[^"\\]|\\.)*"$/.test(expression)) return "string";
  if (/^-?\d+$/.test(expression)) return "BigInteger";
  if (/^(?:true|false)$/.test(expression)) return "bool";
  if (/^\(\s*ByteString\s*\)/.test(expression)) return "ByteString";
  if (/^!/.test(expression) || /^\(\s*bool\s*\)/.test(expression)) return "bool";
  if (/^BigInteger\./.test(expression)) return "BigInteger";
  if (/^(?:StdLib\.Itoa|Helper\.Concat)/.test(expression)) return "string";
  if (/(?:===|!==|==|!=|<=|>=|<|>|\bis\s+null\b)/.test(expression)) return "bool";
  const operands = expression.match(/@?[A-Za-z_][A-Za-z0-9_]*/g) ?? [];
  const operandTypes = [...new Set(operands
    .map((name) => declarationTypes?.get(name.replace(/^@/, "")))
    .filter(Boolean))];
  if (operandTypes.length === 1 && operands.length > 1) return operandTypes[0];
  const identifier = expression.match(/^@?([A-Za-z_][A-Za-z0-9_]*)$/)?.[1];
  return identifier ? declarationTypes?.get(identifier) ?? null : null;
}

// Stack lifting can leave a pure value on its own line when the VM later
// consumes it along another path. Such lines are meaningful in the lifted
// trace but are not legal C# expression statements (`null;`, `1;`, or
// `items[0];`). Preserve their evaluation with a harmless framework-neutral
// conversion call while leaving calls and ordinary assignments untouched.
function renderDiscardedExpression(line) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  if (!trimmed.endsWith(";") || trimmed.startsWith("//")) return line;
  if (/^(?:return|throw|break|continue|goto)\b/.test(trimmed)) return line;
  if (/^(?:if|while|for|switch|case|default|try|catch|finally|else)\b/.test(trimmed)) return line;
  if (/^}\s*(?:while|else|catch|finally)\b/.test(trimmed)) return line;
  if (/^(?:label_0x[0-9A-Fa-f]+):/.test(trimmed)) return line;
  if (hasTopLevelAssignment(trimmed)) return line;
  if (isInvocationStatement(trimmed)) return line;
  const expression = trimmed.slice(0, -1).trim();
  return `${indentation}global::System.Convert.ToString((object)(${expression}));`;
}

function hasTopLevelAssignment(source) {
  let depth = 0;
  let quote = null;
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
    } else if ("([{<".includes(character)) {
      depth += 1;
    } else if (")]}>".includes(character)) {
      depth = Math.max(0, depth - 1);
    } else if (character === "=" && depth === 0 && source[index - 1] !== "=" && source[index + 1] !== "=") {
      return true;
    }
  }
  return false;
}

function isInvocationStatement(source) {
  const expression = source.slice(0, -1).trim();
  if (/^default\s*\(/.test(expression) || /^new\b/.test(expression)) return false;
  if (/^(?:global::)?[A-Za-z_@][A-Za-z0-9_@]*(?:(?:\.|::)[A-Za-z_@][A-Za-z0-9_@]*)*\s*\(/.test(expression)) {
    return true;
  }
  const open = expression.lastIndexOf("(");
  if (open < 1 || !expression.endsWith(")")) return false;
  let cursor = open - 1;
  while (cursor >= 0 && /\s/.test(expression[cursor])) cursor -= 1;
  if (cursor < 0 || !/[A-Za-z0-9_@]/.test(expression[cursor])) return false;
  while (cursor >= 0 && /[A-Za-z0-9_@]/.test(expression[cursor])) cursor -= 1;
  const prefix = expression.slice(0, cursor + 1).trimEnd();
  if (/[+\-*\/%|&!~]/.test(prefix)) return false;
  return prefix === "" || /(?:\.|::|\)|\])$/.test(prefix);
}

function rewriteCSharpControlSyntax(line, declarationTypes = null) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("//")) return line;

  let output = line.replace(/\bleave\s+(label_0x[0-9A-Fa-f]+);/g, "goto $1;");
  const forDeclaration = output.match(
    /^(\s*for\s*\(\s*)let\s+(@?[A-Za-z_][A-Za-z0-9_]*)\s*(=)/,
  );
  if (forDeclaration) {
    const name = forDeclaration[2].replace(/^@/, "");
    const inferredType = declarationTypes?.get(name);
    const renderedType = inferredType && inferredType !== "dynamic" ? inferredType : "var";
    output = output.replace(
      forDeclaration[0],
      `${forDeclaration[1]}${renderedType} ${forDeclaration[2]} ${forDeclaration[3]}`,
    );
  } else {
    output = output.replace(/\bfor\s*\(\s*let\b/g, "for (var");
  }
  const loop = output.match(/^(\s*)loop\s*\{\s*$/);
  if (loop) return `${loop[1]}while (true) {`;

  const switchStatement = output.match(/^(\s*)switch\s+(.+?)\s*\{\s*$/);
  if (switchStatement) {
    return `${switchStatement[1]}switch (${switchStatement[2].trim()}) {`;
  }
  const caseStatement = output.match(/^(\s*)case\s+(.+?)\s*\{\s*$/);
  if (caseStatement) {
    return `${caseStatement[1]}case ${caseStatement[2].trim()}: {`;
  }
  const defaultStatement = output.match(/^(\s*)default\s*\{\s*$/);
  if (defaultStatement) return `${defaultStatement[1]}default: {`;

  const control = output.match(/^(\s*)((?:}\s*else\s+)?(?:if|while))\s+/);
  if (control) {
    const [, indentation, keyword] = control;
    const conditionStart = control[0].length;
    const openingBrace = findControlBodyOpen(output, conditionStart);
    if (openingBrace >= 0) {
      const condition = output.slice(conditionStart, openingBrace).trim();
      const tail = output.slice(openingBrace + 1);
      const body = tail.trim();
      if (body.endsWith("}")) {
        return `${indentation}${keyword} (${renderCSharpCondition(condition)}) { ${body.slice(0, -1).trim()} }`;
      }
      if (!body) {
        return `${indentation}${keyword} (${renderCSharpCondition(condition)}) {`;
      }
    }
  }

  const doWhile = output.match(/^(\s*}\s*while)\s+(.+?);$/);
  if (doWhile) {
    return `${doWhile[1]} (${renderCSharpCondition(doWhile[2])});`;
  }
  const label = output.match(/^(\s*label_0x[0-9A-Fa-f]+):\s*$/);
  if (label) return `${label[1]}: ;`;
  return output;
}

function findControlBodyOpen(line, start) {
  let quote = null;
  let parentheses = 0;
  let brackets = 0;
  for (let index = start; index < line.length; index += 1) {
    const character = line[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "(") {
      parentheses += 1;
    } else if (character === ")") {
      parentheses = Math.max(0, parentheses - 1);
    } else if (character === "[") {
      brackets += 1;
    } else if (character === "]") {
      brackets = Math.max(0, brackets - 1);
    } else if (character === "{" && parentheses === 0 && brackets === 0) {
      return index;
    }
  }
  return -1;
}

function renderCSharpCondition(condition) {
  let source = condition.trim();
  if (hasBalancedOuterParens(source)) source = source.slice(1, -1).trim();
  if (source === "true" || source === "false") return source;
  if (/^-?\d+$/.test(source)) return `${source} != 0`;
  if (source.startsWith("!")) {
    const operand = source.slice(1).trim();
    return `!(${renderCSharpBooleanOperand(operand)})`;
  }
  return renderCSharpBooleanOperand(source);
}

function renderCSharpBooleanOperand(source) {
  return /(?:===?|!==?|<=|>=|&&|\|\||\bis\b)/.test(source)
    ? source
    : `(bool)(dynamic)(${source})`;
}

function hasBalancedOuterParens(source) {
  if (!source.startsWith("(") || !source.endsWith(")")) return false;
  let depth = 0;
  for (let index = 0; index < source.length; index += 1) {
    if (source[index] === "(") depth += 1;
    else if (source[index] === ")") depth -= 1;
    if (depth === 0 && index < source.length - 1) return false;
  }
  return depth === 0;
}

function renderCSharpAssertionCondition(expression, declarationTypes) {
  const source = expression.trim();
  if (source === "null") return "false";
  if (source === "true" || source === "false") return source;
  if (/^-?\d+$/.test(source)) return `${source} != 0`;
  return `(bool)(object)(${rewriteCSharpExpression(source, declarationTypes)})`;
}
