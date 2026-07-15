import { rewriteCSharpExpression, splitCallArguments } from "./csharp-expression.js";
import { csharpIdentifier } from "./csharp-identifiers.js";
export { inferDeclarationTypes } from "./csharp-types.js";
import { inferDeclarationTypes } from "./csharp-types.js";

export function renderBodyLine(line, declarationTypes = null) {
  const indentation = line.match(/^\s*/)?.[0] ?? "";
  const trimmed = line.trim();
  const finish = (rendered) => rewriteCSharpControlSyntax(rendered);
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

function rewriteCSharpControlSyntax(line) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("//")) return line;

  let output = line.replace(/\bleave\s+(label_0x[0-9A-Fa-f]+);/g, "goto $1;");
  output = output.replace(/\bfor\s*\(\s*let\b/g, "for (var");
  const loop = output.match(/^(\s*)loop\s*\{\s*$/);
  if (loop) return `${loop[1]}while (true) {`;

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
