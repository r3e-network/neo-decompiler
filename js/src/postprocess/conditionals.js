import {
  extractIfCondition,
  findMatchingClose,
  isElseOpen,
  isIfOpen,
} from "./helpers.js";

/** Normalize adjacent `else { if (...) {` blocks into `else if` chains. */
export function rewriteElseIfChains(statements) {
  let i = 0;
  while (i + 1 < statements.length) {
    if (isElseOpen(statements[i]) && isIfOpen(statements[i + 1])) {
      const condition = extractIfCondition(statements[i + 1]);
      if (condition !== null) {
        statements[i] = `} else if ${condition} {`;
        statements.splice(i + 1, 1);
        const closeIdx = findMatchingClose(statements, i);
        if (closeIdx >= 0) removeOneCloser(statements, closeIdx);
        continue;
      }
    }
    i++;
  }
}

function removeOneCloser(statements, closeIdx) {
  if (
    closeIdx + 1 < statements.length &&
    statements[closeIdx].trim() === "}" &&
    statements[closeIdx + 1].trim() === "}"
  ) {
    statements.splice(closeIdx + 1, 1);
    return;
  }
  if (statements[closeIdx].trim() === "}" && closeIdx > 0) {
    for (let i = closeIdx - 1; i >= 0; i--) {
      const previous = statements[i].trim();
      if (previous !== "") {
        if (previous === "}") statements.splice(closeIdx, 1);
        break;
      }
    }
  }
}
