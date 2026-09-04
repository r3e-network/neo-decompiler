/**
 * Small lexical helpers shared by the C# expression rewriter.
 *
 * These scanners deliberately avoid parsing C#; they only track quoted
 * literals, comments, and balanced delimiters so rewrite passes do not touch
 * non-code text or split nested calls at the wrong comma.
 */

export function nextOutsideMatch(text, pattern) {
  let match;
  while ((match = pattern.exec(text)) !== null) {
    if (!isInsideQuotedString(text, match.index)) return match;
  }
  return null;
}

export function isInsideQuotedString(text, end) {
  // Retain the shared API name, but comments must be opaque to every caller
  // for the same reason as quoted text: their contents are not expressions.
  for (let index = 0; index < end;) {
    const next = nonCodeEnd(text, index);
    if (next > index) {
      if (end < next) return true;
      index = next;
    } else {
      index += 1;
    }
  }
  return false;
}

export function findBracketClose(text, open) {
  return findDelimiterClose(text, open, "[", "]");
}

export function findQuotedLiteralClose(text, open) {
  const quote = text[open];
  const verbatim = quote === '"' && text[open - 1] === "@";
  for (let index = open + 1; index < text.length; index += 1) {
    if (!verbatim && text[index] === "\\") {
      index += 1;
    } else if (text[index] === quote) {
      if (verbatim && text[index + 1] === quote) {
        index += 1;
        continue;
      }
      return index;
    }
  }
  return -1;
}

export function findCallClose(text, open) {
  return findDelimiterClose(text, open, "(", ")");
}

function findDelimiterClose(text, open, left, right) {
  if (open < 0 || text[open] !== left) return -1;
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    const next = nonCodeEnd(text, index);
    if (next > index) {
      index = next - 1;
      continue;
    }
    if (text[index] === left) {
      depth += 1;
    } else if (text[index] === right && --depth === 0) {
      return index;
    }
  }
  return -1;
}

export function splitCallArguments(text, { typeDeclarations = false } = {}) {
  if (!text) return [];
  const result = [];
  let start = 0;
  let depth = 0;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    const next = nonCodeEnd(text, index);
    if (next > index) {
      index = next - 1;
      continue;
    }
    if (character === "<") {
      const close = findTypeArgumentsClose(text, index, typeDeclarations);
      if (close >= 0) {
        index = close;
        continue;
      }
    }
    if ("([{".includes(character)) depth += 1;
    else if (")]}".includes(character)) depth -= 1;
    else if (character === "," && depth === 0) {
      result.push(text.slice(start, index).trim());
      start = index + 1;
    }
  }
  const tail = text.slice(start).trim();
  if (tail) result.push(tail);
  return result;
}

function nonCodeEnd(text, index) {
  if (text[index] === '"' || text[index] === "'") {
    const close = findQuotedLiteralClose(text, index);
    return close < 0 ? text.length : close + 1;
  }
  if (text.startsWith("//", index)) {
    const close = text.indexOf("\n", index + 2);
    return close < 0 ? text.length : close;
  }
  if (text.startsWith("/*", index)) {
    const close = text.indexOf("*/", index + 2);
    return close < 0 ? text.length : close + 2;
  }
  return index;
}

function findTypeArgumentsClose(text, open, typeDeclarations) {
  // Generated generic types/calls attach '<' to the preceding type name.
  // Ordinary comparisons and shifts must not affect comma nesting. Requiring
  // a type-shaped, balanced suffix also handles compact comparisons x<y.
  if (!/[A-Za-z0-9_>\]]/.test(text[open - 1] ?? "")) return -1;
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    const character = text[index];
    if (character === "<") depth += 1;
    else if (character === ">") {
      if (--depth === 0) {
        // A following operand makes this a relational expression, e.g.
        // x<y, a> b. Parameter declarations opt into type grammar explicitly;
        // expression callers must see a delimiter/operator after the suffix.
        let next = index + 1;
        while (/\s/.test(text[next] ?? "")) next += 1;
        const following = text[next];
        if (typeDeclarations && next > index + 1 && /[@A-Za-z_]/.test(following ?? "")) return index;
        return following === undefined || /[()[\]{},.;?:|^&]/.test(following)
          || text.startsWith("==", next) || text.startsWith("!=", next)
          ? index : -1;
      }
    } else if (!/[A-Za-z0-9_@.,?\[\]\s:]/.test(character)) {
      return -1;
    }
  }
  return -1;
}
