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
        // Capture the original `else` wrapper's closer before removing its
        // nested `if` opener. The wrapper may contain suffix statements after
        // that nested branch, so its closer is not necessarily adjacent to
        // the nested branch's closer.
        const wrapperClose = findMatchingClose(statements, i);
        statements[i] = `} else if ${condition} {`;
        statements.splice(i + 1, 1);
        if (wrapperClose >= i + 2) {
          const adjustedClose = wrapperClose - 1;
          if (statements[adjustedClose]?.trim() === "}") {
            statements.splice(adjustedClose, 1);
          }
        }
        continue;
      }
    }
    i++;
  }
}
