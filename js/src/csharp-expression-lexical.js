/**
 * Balanced-delimiter helpers shared by the C# expression rewriters.
 *
 * The expression scanner handles calls and literals; these helpers cover the
 * reverse scans needed when an operator or indexer is discovered first.
 */

export function findMatchingOpen(line, close, openCharacter, closeCharacter) {
  let depth = 0;
  for (let index = close; index >= 0; index -= 1) {
    if (line[index] === closeCharacter) depth += 1;
    else if (line[index] === openCharacter && --depth === 0) return index;
  }
  return -1;
}

export function scanIdentifierPathStart(line, end) {
  let cursor = end;
  while (cursor >= 0 && /[A-Za-z0-9_@.]/.test(line[cursor])) cursor -= 1;
  return cursor + 1;
}
