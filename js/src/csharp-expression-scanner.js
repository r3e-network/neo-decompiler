/**
 * Small lexical helpers shared by the C# expression rewriter.
 *
 * These scanners deliberately avoid parsing C#; they only track quoted
 * literals and balanced delimiters so rewrite passes do not touch text inside
 * strings or split nested calls at the wrong comma.
 */

export function nextOutsideMatch(text, pattern) {
  let match;
  while ((match = pattern.exec(text)) !== null) {
    if (!isInsideQuotedString(text, match.index)) return match;
  }
  return null;
}

export function isInsideQuotedString(text, end) {
  let quote = null;
  for (let index = 0; index < end; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
    } else if (character === '"' || character === "'") {
      quote = character;
    }
  }
  return quote !== null;
}

export function findBracketClose(text, open) {
  let depth = 0;
  let quote = null;
  for (let index = open; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") quote = character;
    else if (character === "[") depth += 1;
    else if (character === "]" && --depth === 0) return index;
  }
  return -1;
}

export function findQuotedLiteralClose(text, open) {
  const quote = text[open];
  for (let index = open + 1; index < text.length; index += 1) {
    if (text[index] === "\\") {
      index += 1;
    } else if (text[index] === quote) {
      return index;
    }
  }
  return -1;
}

export function findCallClose(text, open) {
  if (open < 0) return -1;
  let depth = 0;
  let quote = null;
  for (let index = open; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
    } else if (character === "(") {
      depth += 1;
    } else if (character === ")" && --depth === 0) {
      return index;
    }
  }
  return -1;
}

export function splitCallArguments(text) {
  if (!text) return [];
  const result = [];
  let start = 0;
  let depth = 0;
  let quote = null;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (character === "\\") index += 1;
      else if (character === quote) quote = null;
      continue;
    }
    if (character === '"' || character === "'") quote = character;
    else if ("([{<".includes(character)) depth += 1;
    else if (")]} >".replace(" ", "").includes(character)) depth -= 1;
    else if (character === "," && depth === 0) {
      result.push(text.slice(start, index).trim());
      start = index + 1;
    }
  }
  const tail = text.slice(start).trim();
  if (tail) result.push(tail);
  return result;
}
